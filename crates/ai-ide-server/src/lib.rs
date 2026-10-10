use ai_ide_core::ai::{default_provider_id, list_providers, resolve_provider, ProvidersResponse};
use ai_ide_core::changes::SemanticChangeDetector;
use ai_ide_core::context::ContextBuilder;
use ai_ide_core::drift::DriftDetector;
use ai_ide_core::graph::DependencyGraph;
use ai_ide_core::IdeResult;
use ai_ide_core::models::{FilePatch, GenerationRecord, ImplementationPlan, TaskContext};

use ai_ide_core::patch::PatchEngine;
use ai_ide_core::specs::{SpecParser, SpecRegistry};
use ai_ide_core::storage::GenerationStore;
use ai_ide_core::validation::runner::ValidationRunner;
use ai_ide_core::validation::RepairOrchestrator;
use ai_ide_core::workspace::WorkspaceManager;
use ai_ide_core::watch::{FileEvent, WorkspaceWatcher};
use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use serde::{Deserialize, Serialize};
use std::future::Future;
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
    /// Id of the provider used by Generate and Repair (see `ai::registry::PROVIDER_IDS`).
    active_provider: Arc<Mutex<String>>,
    /// Debounced file changes, fanned out to every `/ws` client.
    events: broadcast::Sender<FileEvent>,
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


/// Aborts the file-event reactor when the server stops, so a restarted server does not leave it behind.
struct AbortOnDrop(tokio::task::JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Serves the AI-IDE API, the live `/ws` stream and the built frontend for the workspace at `root`,
/// until `shutdown` resolves. The CLI binary and the desktop app both call this.
pub async fn serve<F>(root: PathBuf, listener: tokio::net::TcpListener, shutdown: F) -> IdeResult<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    info!("Starting AI-IDE Server in workspace: {}", root.display());

    let wm = Arc::new(WorkspaceManager::new(&root)?);
    let mut reg = SpecRegistry::new();
    let _ = reg.load_from_workspace(&wm);

    let graph = build_graph(&wm, &reg);
    info!(
        "Dependency graph: {} nodes, {} edges",
        graph.graph.node_count(),
        graph.graph.edge_count()
    );

    let store_path = root.join(".ai-ide-history.db");
    let store = match GenerationStore::open(&store_path) {
        Ok(store) => store,
        Err(e) => {
            tracing::warn!(
                "Could not open history database {}: {}. History will not be saved this session.",
                store_path.display(),
                e
            );
            GenerationStore::in_memory()?
        }
    };

    let (events, _) = broadcast::channel::<FileEvent>(256);
    let state = AppState {
        workspace: wm,
        registry: Arc::new(Mutex::new(reg)),
        graph: Arc::new(Mutex::new(graph)),
        store: Arc::new(Mutex::new(store)),
        active_provider: Arc::new(Mutex::new(default_provider_id().to_string())),
        events: events.clone(),
    };

    // Kept alive for the life of this call; dropping it stops the watch.
    let _watcher = match WorkspaceWatcher::start(state.workspace.root(), events) {
        Ok(watcher) => Some(watcher),
        Err(e) => {
            tracing::warn!("Live file updates are disabled: {}", e);
            None
        }
    };
    let _reactor = AbortOnDrop(tokio::spawn(react_to_file_events(state.clone())));

    let app = Router::new()
        .route("/api/workspace", get(get_workspace))
        .route("/api/file", get(get_file_content).post(save_file_content))
        .route("/api/specs", get(get_specs).post(save_spec))
        .route("/api/diff", post(compute_diff))
        .route("/api/impact", post(compute_impact))
        .route("/api/generate", post(generate_plan))
        .route("/api/repair", post(repair_plan))
        .route("/api/providers", get(get_providers).post(set_provider))
        .route("/api/patch/apply", post(apply_patches))
        .route("/api/validation/run", post(run_validation))
        .route("/api/rollback", post(rollback_last_change))
        .route("/api/drift", get(get_drift))
        .route("/api/history", get(get_history))
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("frontend/dist"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    if let Ok(addr) = listener.local_addr() {
        info!("AI-IDE Server listening on http://{}", addr);
    }
    axum::serve(listener, app).with_graceful_shutdown(shutdown).await?;
    Ok(())
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

/// Builds the AI context for a spec edit: the semantic changes plus the files its impact touches.
fn build_task_context(
    state: &AppState,
    description: &str,
    spec_id: &str,
    new_yaml: &str,
) -> Result<TaskContext, (StatusCode, String)> {
    let old_node = {
        let reg = state.registry.lock().unwrap();
        reg.get(spec_id).cloned()
    };
    let file_path = old_node.as_ref().map(|n| n.file_path.as_str()).unwrap_or("specs/api/spec.yaml");

    let new_node = SpecParser::parse_yaml_str(new_yaml, file_path).map_err(|e| {
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;

    let changes = SemanticChangeDetector::diff(old_node.as_ref(), Some(&new_node));

    let (direct, trans, verify, _) = state.impact_for(&[spec_id.to_string()]);
    let affected_files: Vec<String> = direct.into_iter().chain(trans).chain(verify).collect();
    ContextBuilder::build(
        &state.workspace,
        description,
        changes,
        vec![new_node],
        &affected_files,
        serde_json::json!({ "naming": "camelCase" }),
        vec![],
    ).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

/// Id of the provider the user selected in the navbar.
fn active_provider_id(state: &AppState) -> String {
    state.active_provider.lock().unwrap().clone()
}

async fn generate_plan(
    State(state): State<AppState>,
    Json(payload): Json<GenerateRequest>,
) -> Result<Json<ImplementationPlan>, (StatusCode, String)> {
    let context = build_task_context(&state, &payload.description, &payload.spec_id, &payload.new_yaml)?;

    let provider = resolve_provider(&active_provider_id(&state), &state.workspace)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let plan = provider.generate_plan(&context).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;

    Ok(Json(plan))
}

#[derive(Deserialize)]
struct RepairRequest {
    description: String,
    spec_id: String,
    new_yaml: String,
    checks: Vec<(String, String)>,
    #[serde(default = "default_repair_attempts")]
    max_attempts: usize,
}

const MAX_REPAIR_ATTEMPTS: usize = 3;

fn default_repair_attempts() -> usize {
    MAX_REPAIR_ATTEMPTS
}

/// Runs the checks, and while they fail, sends the output back to the provider and applies its fixes.
/// Every patch that reaches disk is recorded in history so Rollback can undo it.
async fn repair_plan(
    State(state): State<AppState>,
    Json(payload): Json<RepairRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let context = build_task_context(&state, &payload.description, &payload.spec_id, &payload.new_yaml)?;
    let provider = resolve_provider(&active_provider_id(&state), &state.workspace)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let check_refs: Vec<(&str, &str)> = payload.checks.iter()
        .map(|(n, c)| (n.as_str(), c.as_str()))
        .collect();
    let max_attempts = payload.max_attempts.clamp(1, MAX_REPAIR_ATTEMPTS);

    let report = RepairOrchestrator::run_bounded_repair(
        &state.workspace,
        provider.as_ref(),
        &context,
        &check_refs,
        max_attempts,
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !report.applied_patches.is_empty() {
        let record = GenerationRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            spec_version_hash: "hash".into(),
            plan_summary: format!("Auto-repair: {} attempt(s)", report.attempts.len()),
            patches: PatchEngine::collapse_patches(&report.applied_patches),
            git_base_commit: None,
            status: "applied".into(),
        };
        let store = state.store.lock().unwrap();
        let _ = store.record_generation(&record);
        drop(store);
        state.rebuild_graph();
    }

    Ok(Json(serde_json::to_value(&report).unwrap_or_default()))
}

async fn get_providers(State(state): State<AppState>) -> impl IntoResponse {
    Json(list_providers(&active_provider_id(&state), &state.workspace).await)
}

#[derive(Deserialize)]
struct SetProviderRequest {
    id: String,
}

async fn set_provider(
    State(state): State<AppState>,
    Json(payload): Json<SetProviderRequest>,
) -> Result<Json<ProvidersResponse>, (StatusCode, String)> {
    let status = list_providers(&payload.id, &state.workspace).await;
    let Some(info) = status.providers.iter().find(|p| p.id == payload.id) else {
        return Err((StatusCode::BAD_REQUEST, format!("Unknown provider '{}'", payload.id)));
    };
    if !info.available {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Provider '{}' is not configured. Set its API key or endpoint first.", payload.id),
        ));
    }

    *state.active_provider.lock().unwrap() = payload.id.clone();
    Ok(Json(list_providers(&payload.id, &state.workspace).await))
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

/// Undoes the most recent applied AI patch set using the contents saved in history.
/// Only files the AI wrote are touched, and only if they were not edited since.
async fn rollback_last_change(State(state): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let record = {
        let store = state.store.lock().unwrap();
        store
            .list_history()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .into_iter()
            .find(|r| r.status == "applied")
    };
    let Some(record) = record else {
        return Err((StatusCode::NOT_FOUND, "No applied AI change to roll back".into()));
    };

    // Verify every file first so the rollback is all-or-nothing.
    for patch in &record.patches {
        PatchEngine::check_revert(&state.workspace, patch).map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
    }
    for patch in record.patches.iter().rev() {
        PatchEngine::revert_patch(&state.workspace, patch)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }
    {
        let store = state.store.lock().unwrap();
        let _ = store.update_status(&record.id, "rolled_back");
    }
    {
        let mut reg = state.registry.lock().unwrap();
        let _ = reg.load_from_workspace(&state.workspace);
    }
    state.rebuild_graph();

    Ok(Json(serde_json::json!({ "success": true, "message": format!("Rolled back AI change ({} file(s))", record.patches.len()) })))
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

/// Keeps the spec registry and dependency graph in step with the files on disk.
/// Specs reload the registry; source and test changes only rebuild the graph.
async fn react_to_file_events(state: AppState) {
    let mut rx = state.events.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => {
                let touches_specs = event.paths.iter().any(|p| p.starts_with("specs/"));
                let touches_code = event.paths.iter().any(|p| p.starts_with("src/") || p.starts_with("tests/"));
                if touches_specs {
                    let mut reg = state.registry.lock().unwrap();
                    let _ = reg.load_from_workspace(&state.workspace);
                }
                if touches_specs || touches_code {
                    state.rebuild_graph();
                }
            }
            // Missed some events: a full refresh is the only safe way to catch up.
            Err(broadcast::error::RecvError::Lagged(_)) => {
                {
                    let mut reg = state.registry.lock().unwrap();
                    let _ = reg.load_from_workspace(&state.workspace);
                }
                state.rebuild_graph();
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let rx = state.events.subscribe();
    ws.on_upgrade(move |socket| forward_file_events(socket, rx))
}

/// Sends each file event to one client as `{"kind": ..., "paths": [...]}` until it disconnects.
async fn forward_file_events(socket: WebSocket, mut rx: broadcast::Receiver<FileEvent>) {
    let (mut sender, mut incoming) = socket.split();
    loop {
        tokio::select! {
            event = rx.recv() => match event {
                Ok(event) => {
                    let Ok(text) = serde_json::to_string(&event) else { continue };
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            },
            message = incoming.next() => match message {
                // The protocol is server-to-client only; other frames are ignored.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
        }
    }
}
