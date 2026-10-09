use ai_ide_core::ai::{AiProvider, MockAiProvider};
use ai_ide_core::changes::SemanticChangeDetector;
use ai_ide_core::context::ContextBuilder;
use ai_ide_core::drift::DriftDetector;
use ai_ide_core::git::GitManager;
use ai_ide_core::graph::DependencyGraph;
use ai_ide_core::models::{FilePatch, GenerationRecord, ImplementationPlan};

use ai_ide_core::patch::PatchEngine;
use ai_ide_core::specs::{SpecParser, SpecRegistry};
use ai_ide_core::storage::GenerationStore;
use ai_ide_core::validation::runner::ValidationRunner;
use ai_ide_core::workspace::WorkspaceManager;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use tracing::info;

struct GraphState {
    graph: DependencyGraph,
    files: Vec<String>,
}

#[derive(Clone)]
struct AppState {
    workspace: Arc<WorkspaceManager>,
    registry: Arc<Mutex<SpecRegistry>>,
    graph: Arc<Mutex<GraphState>>,
    store: Arc<Mutex<GenerationStore>>,
}

fn build_graph(wm: &WorkspaceManager, reg: &SpecRegistry) -> GraphState {
    match DependencyGraph::build_from_workspace(wm, &reg.all_nodes()) {
        Ok((graph, files)) => GraphState { graph, files },
        Err(e) => {
            tracing::warn!("Failed to build dependency graph: {}", e);
            GraphState { graph: DependencyGraph::new(), files: Vec::new() }
        }
    }
}

impl AppState {
    fn rebuild_graph(&self) {
        let fresh = {
            let reg = self.registry.lock().unwrap();
            build_graph(&self.workspace, &reg)
        };
        *self.graph.lock().unwrap() = fresh;
    }

    fn impact_for(&self, spec_ids: &[String]) -> (Vec<String>, Vec<String>, Vec<String>, usize) {
        let g = self.graph.lock().unwrap();
        g.graph.compute_impact(spec_ids, &g.files)
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let root_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    info!("Starting AI-IDE Server in workspace: {}", root_path.display());

    let wm = Arc::new(WorkspaceManager::new(&root_path).expect("Failed to initialize workspace"));
    let mut reg = SpecRegistry::new();
    let _ = reg.load_from_workspace(&wm);

    let graph = build_graph(&wm, &reg);
    info!(
        "Dependency graph: {} nodes, {} edges",
        graph.graph.node_count(),
        graph.graph.edge_count()
    );

    let store_path = root_path.join(".ai-ide-history.db");
    let store = Arc::new(Mutex::new(
        GenerationStore::open(&store_path).unwrap_or_else(|_| GenerationStore::in_memory().unwrap()),
    ));

    let state = AppState {
        workspace: wm,
        registry: Arc::new(Mutex::new(reg)),
        graph: Arc::new(Mutex::new(graph)),
        store,
    };

    let app = Router::new()
        .route("/api/workspace", get(get_workspace))
        .route("/api/file", get(get_file_content).post(save_file_content))
        .route("/api/specs", get(get_specs).post(save_spec))
        .route("/api/diff", post(compute_diff))
        .route("/api/impact", post(compute_impact))
        .route("/api/generate", post(generate_plan))
        .route("/api/patch/apply", post(apply_patches))
        .route("/api/validation/run", post(run_validation))
        .route("/api/git/status", get(get_git_status))
        .route("/api/git/rollback", post(rollback_git))
        .route("/api/drift", get(get_drift))
        .route("/api/history", get(get_history))
        .fallback_service(ServeDir::new("frontend/dist"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
    info!("AI-IDE Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[derive(Serialize)]
struct WorkspaceResponse {
    root: String,
    files: Vec<String>,
}

async fn get_workspace(State(state): State<AppState>) -> impl IntoResponse {
    let mut files = Vec::new();
    if let Ok(spec_files) = state.workspace.list_spec_files() {
        for f in spec_files {
            if let Ok(rel) = f.strip_prefix(state.workspace.root()) {
                files.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for dir in ["src", "tests"] {
        files.extend(state.workspace.list_files_under(dir).unwrap_or_default());
    }

    Json(WorkspaceResponse {
        root: state.workspace.root().to_string_lossy().to_string(),
        files,
    })
}

#[derive(Deserialize)]
struct FileQuery {
    path: String,
}

#[derive(Deserialize)]
struct SaveFileRequest {
    path: String,
    content: String,
}

async fn get_file_content(
    State(state): State<AppState>,
    Query(query): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match state.workspace.read_file(&query.path) {
        Ok(content) => Ok(Json(serde_json::json!({ "path": query.path, "content": content }))),
        Err(e) => Err((StatusCode::NOT_FOUND, e.to_string())),
    }
}

async fn save_file_content(
    State(state): State<AppState>,
    Json(payload): Json<SaveFileRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    match state.workspace.write_file(&payload.path, &payload.content) {
        Ok(_) => {
            state.rebuild_graph();
            Ok(Json(serde_json::json!({ "success": true, "path": payload.path })))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

async fn get_specs(State(state): State<AppState>) -> impl IntoResponse {
    let reg = state.registry.lock().unwrap();
    let nodes: Vec<_> = reg.all_nodes().into_iter().cloned().collect();
    Json(nodes)
}

#[derive(Deserialize)]
struct SaveSpecRequest {
    path: String,
    content: String,
}

async fn save_spec(
    State(state): State<AppState>,
    Json(payload): Json<SaveSpecRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state.workspace.write_file(&payload.path, &payload.content).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    let parsed_node = SpecParser::parse_yaml_str(&payload.content, &payload.path).map_err(|e| {
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;

    {
        let mut reg = state.registry.lock().unwrap();
        reg.register_or_update(parsed_node.clone()).map_err(|e| {
            (StatusCode::BAD_REQUEST, e.to_string())
        })?;
    }
    state.rebuild_graph();

    Ok(Json(serde_json::json!({
        "success": true,
        "node": parsed_node
    })))
}

#[derive(Deserialize)]
struct DiffRequest {
    spec_id: String,
    new_yaml: String,
}

async fn compute_diff(
    State(state): State<AppState>,
    Json(payload): Json<DiffRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let reg = state.registry.lock().unwrap();
    let old_node = reg.get(&payload.spec_id);

    let file_path = old_node.map(|n| n.file_path.as_str()).unwrap_or("specs/api/spec.yaml");
    let new_node = SpecParser::parse_yaml_str(&payload.new_yaml, file_path).map_err(|e| {
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;

    let changes = SemanticChangeDetector::diff(old_node, Some(&new_node));
    Ok(Json(serde_json::json!({ "changes": changes })))
}

#[derive(Deserialize)]
struct ImpactRequest {
    changed_spec_ids: Vec<String>,
}

async fn compute_impact(
    State(state): State<AppState>,
    Json(payload): Json<ImpactRequest>,
) -> impl IntoResponse {
    let (direct, trans, verify, unaffected) = state.impact_for(&payload.changed_spec_ids);

    Json(serde_json::json!({
        "directly_affected": direct,
        "transitively_affected": trans,
        "verification_only": verify,
        "unaffected_count": unaffected,
    }))
}

#[derive(Deserialize)]
struct GenerateRequest {
    description: String,
    spec_id: String,
    new_yaml: String,
}

async fn generate_plan(
    State(state): State<AppState>,
    Json(payload): Json<GenerateRequest>,
) -> Result<Json<ImplementationPlan>, (StatusCode, String)> {
    let old_node = {
        let reg = state.registry.lock().unwrap();
        reg.get(&payload.spec_id).cloned()
    };
    let file_path = old_node.as_ref().map(|n| n.file_path.as_str()).unwrap_or("specs/api/spec.yaml");

    let new_node = SpecParser::parse_yaml_str(&payload.new_yaml, file_path).map_err(|e| {
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;

    let changes = SemanticChangeDetector::diff(old_node.as_ref(), Some(&new_node));

    let (direct, trans, verify, _) = state.impact_for(&[payload.spec_id.clone()]);
    let affected_files: Vec<String> = direct.into_iter().chain(trans).chain(verify).collect();
    let context = ContextBuilder::build(
        &state.workspace,
        &payload.description,
        changes,
        vec![new_node],
        &affected_files,
        serde_json::json!({ "naming": "camelCase" }),
        vec![],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let provider = MockAiProvider;
    let plan = provider.generate_plan(&context).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    Ok(Json(plan))
}

#[derive(Deserialize)]
struct ApplyPatchRequest {
    patches: Vec<FilePatch>,
}

async fn apply_patches(
    State(state): State<AppState>,
    Json(payload): Json<ApplyPatchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    for patch in &payload.patches {
        PatchEngine::apply_patch(&state.workspace, patch).map_err(|e| {
            (StatusCode::CONFLICT, e.to_string())
        })?;
    }

    // Record in history
    let record = GenerationRecord {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now(),
        spec_version_hash: "hash".into(),
        plan_summary: format!("Applied {} patches safely", payload.patches.len()),
        patches: payload.patches,
        git_base_commit: None,
        status: "applied".into(),
    };

    {
        let store = state.store.lock().unwrap();
        let _ = store.record_generation(&record);
    }
    state.rebuild_graph();

    Ok(Json(serde_json::json!({ "success": true, "record_id": record.id })))
}

#[derive(Deserialize)]
struct ValidationRequest {
    checks: Vec<(String, String)>,
}

async fn run_validation(
    State(state): State<AppState>,
    Json(payload): Json<ValidationRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let check_refs: Vec<(&str, &str)> = payload.checks.iter()
        .map(|(n, c)| (n.as_str(), c.as_str()))
        .collect();

    let summary = ValidationRunner::run_all(&state.workspace, &check_refs).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    Ok(Json(serde_json::to_value(summary).unwrap()))
}

async fn get_git_status(State(state): State<AppState>) -> impl IntoResponse {
    let branch = GitManager::get_current_branch(&state.workspace).await.unwrap_or_else(|_| "main".into());
    let status = GitManager::get_status(&state.workspace).await.unwrap_or_default();
    let diff = GitManager::get_diff(&state.workspace).await.unwrap_or_default();

    Json(serde_json::json!({
        "branch": branch,
        "status": status,
        "diff": diff,
    }))
}

async fn rollback_git(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    GitManager::rollback_working_tree(&state.workspace).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;
    {
        let mut reg = state.registry.lock().unwrap();
        let _ = reg.load_from_workspace(&state.workspace);
    }
    state.rebuild_graph();

    Ok(Json(serde_json::json!({ "success": true, "message": "Rolled back working tree" })))
}

async fn get_drift(State(state): State<AppState>) -> impl IntoResponse {
    let reg = state.registry.lock().unwrap();
    let nodes: Vec<_> = reg.all_nodes();
    let diagnostics = DriftDetector::scan_for_drift(&state.workspace, &nodes).unwrap_or_default();
    Json(diagnostics)
}

async fn get_history(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.store.lock().unwrap();
    let history = store.list_history().unwrap_or_default();
    Json(history)
}
