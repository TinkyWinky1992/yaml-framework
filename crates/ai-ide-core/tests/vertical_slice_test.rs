use ai_ide_core::ai::{AiProvider, MockAiProvider};
use ai_ide_core::changes::SemanticChangeDetector;
use ai_ide_core::context::ContextBuilder;
use ai_ide_core::graph::{DependencyGraph, EdgeType};
use ai_ide_core::models::ChangeKind;

use ai_ide_core::patch::PatchEngine;
use ai_ide_core::specs::{SpecParser, SpecRegistry};
use ai_ide_core::workspace::WorkspaceManager;
use std::fs;

#[tokio::test]
async fn test_complete_vertical_slice_workflow() {
    let temp_dir = std::env::temp_dir().join(format!("ai_ide_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(temp_dir.join("specs/api")).unwrap();
    fs::create_dir_all(temp_dir.join("src/routes")).unwrap();
    fs::create_dir_all(temp_dir.join("tests")).unwrap();

    let wm = WorkspaceManager::new(&temp_dir).unwrap();

    // 1. Initial Spec
    let initial_spec = r#"
api:
  name: getUser
  method: GET
  path: /api/users/{id}
  responses:
    200:
      description: OK
      schema:
        id: string
"#;
    wm.write_file("specs/api/users.yaml", initial_spec).unwrap();

    // Baseline source files
    wm.write_file("src/routes/users.ts", "export const getUser = () => ({ id: '1' });\n").unwrap();
    wm.write_file("tests/users.test.ts", "test('getUser', () => { expect(getUser().id).toBe('1'); });\n").unwrap();

    // 2. Load & Validate Spec in Registry
    let mut registry = SpecRegistry::new();
    let warnings = registry.load_from_workspace(&wm).unwrap();
    assert!(warnings.is_empty(), "Expected clean schema validation");
    assert_eq!(registry.len(), 1);

    // 3. User edits specification (adds phone to schema)
    let updated_spec = r#"
api:
  name: getUser
  method: GET
  path: /api/users/{id}
  responses:
    200:
      description: OK
      schema:
        id: string
        phone: string
"#;
    let old_node = registry.get("api:getUser").cloned().unwrap();
    let new_node = SpecParser::parse_yaml_str(updated_spec, "specs/api/users.yaml").unwrap();

    // 4. Deterministic Semantic Change Detection
    let changes = SemanticChangeDetector::diff(Some(&old_node), Some(&new_node));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, ChangeKind::Added);
    assert_eq!(changes[0].path, vec!["api", "responses", "200", "schema", "phone"]);

    // 5. Dependency Graph Impact Analysis
    let mut graph = DependencyGraph::new();
    graph.add_or_get_node(&old_node.id, "specs/api/users.yaml", true, false);
    graph.add_or_get_node("file:src/routes/users.ts", "src/routes/users.ts", false, false);
    graph.add_or_get_node("test:tests/users.test.ts", "tests/users.test.ts", false, true);

    graph.add_edge(&old_node.id, "file:src/routes/users.ts", EdgeType::Implements).unwrap();
    graph.add_edge("file:src/routes/users.ts", "test:tests/users.test.ts", EdgeType::Tests).unwrap();

    let all_files = vec![
        "src/routes/users.ts".into(),
        "tests/users.test.ts".into(),
        "src/unrelated.ts".into(),
    ];
    let (direct, _trans, verify, unaffected) = graph.compute_impact(&[old_node.id.clone()], &all_files);

    assert_eq!(direct, vec!["src/routes/users.ts"]);
    assert_eq!(verify, vec!["tests/users.test.ts"]);
    assert_eq!(unaffected, 1);

    // 6. Build Minimal Sufficient Context
    let mut affected_files = direct.clone();
    affected_files.extend(verify.clone());
    let context = ContextBuilder::build(
        &wm,
        "Update implementation for getUser phone response field",
        changes.clone(),
        vec![new_node.clone()],
        &affected_files,
        serde_json::json!({ "naming": "camelCase" }),
        vec![],
    ).unwrap();

    let tokens = ContextBuilder::estimate_tokens(&context);
    assert!(tokens > 0 && tokens < 500, "Context must remain token-bounded");

    // 7. AI Generation (using Mock provider)
    let ai_provider = MockAiProvider;
    let plan = ai_provider.generate_plan(&context).await.unwrap();
    assert_eq!(plan.patches.len(), 2);

    // 8. Visual Diff & Patch Conflict Verification
    for patch in &plan.patches {
        assert!(patch.unified_diff.contains('+'));
        // Check conflict before application
        assert!(PatchEngine::check_conflict(&wm, patch).is_ok());
    }

    // 9. Apply Approved Patch
    for patch in &plan.patches {
        PatchEngine::apply_patch(&wm, patch).unwrap();
    }

    // 10. Verify Patched Files
    let updated_route = wm.read_file("src/routes/users.ts").unwrap();
    assert!(updated_route.contains("phone: 'active'"));
    let updated_test = wm.read_file("tests/users.test.ts").unwrap();
    assert!(updated_test.contains("phone"));

    // Cleanup temp dir
    let _ = fs::remove_dir_all(temp_dir);
}
