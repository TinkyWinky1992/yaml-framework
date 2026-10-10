use ai_ide_core::graph::{DependencyGraph, EdgeType};
use ai_ide_core::models::{FilePatch, PatchAction};
use ai_ide_core::patch::PatchEngine;
use ai_ide_core::specs::SpecRegistry;
use ai_ide_core::workspace::WorkspaceManager;
use ai_ide_core::IdeError;
use std::path::PathBuf;

/// A fresh, empty workspace under the system temp dir. Callers remove it when done.
fn temp_workspace() -> (PathBuf, WorkspaceManager) {
    let dir = std::env::temp_dir().join(format!("ai_ide_edge_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let wm = WorkspaceManager::new(&dir).unwrap();
    (dir, wm)
}

#[test]
fn manual_edit_after_generation_makes_apply_fail_with_conflict() {
    let (dir, wm) = temp_workspace();
    wm.write_file("src/routes/users.ts", "old\n").unwrap();

    // The AI generated its patch against "old\n".
    let patch = PatchEngine::create_diff("src/routes/users.ts", PatchAction::Modify, Some("old\n"), "new\n");

    // The user edits the file by hand before approving.
    wm.write_file("src/routes/users.ts", "manual edit\n").unwrap();

    let err = PatchEngine::apply_patch(&wm, &patch).unwrap_err();
    assert!(matches!(err, IdeError::PatchConflict { .. }), "expected PatchConflict, got {err:?}");
    assert_eq!(wm.read_file("src/routes/users.ts").unwrap(), "manual edit\n");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rollback_after_manual_edit_is_refused_and_file_stays_untouched() {
    let (dir, wm) = temp_workspace();
    wm.write_file("src/routes/users.ts", "old\n").unwrap();

    let patch = PatchEngine::create_diff("src/routes/users.ts", PatchAction::Modify, Some("old\n"), "new\n");
    PatchEngine::apply_patch(&wm, &patch).unwrap();

    // The user keeps editing after the AI change was applied.
    wm.write_file("src/routes/users.ts", "new\nmy own line\n").unwrap();

    let err = PatchEngine::revert_patch(&wm, &patch).unwrap_err();
    assert!(matches!(err, IdeError::PatchConflict { .. }), "expected PatchConflict, got {err:?}");
    assert_eq!(wm.read_file("src/routes/users.ts").unwrap(), "new\nmy own line\n");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn circular_spec_dependency_is_rejected_and_the_edge_is_not_kept() {
    let mut graph = DependencyGraph::new();
    graph.add_or_get_node("api:getUser", "specs/api/users.yaml", true, false);
    graph.add_or_get_node("ui:UserProfileView", "specs/ui/users.yaml", true, false);

    graph
        .add_edge("api:getUser", "ui:UserProfileView", EdgeType::References)
        .unwrap();

    let err = graph
        .add_edge("ui:UserProfileView", "api:getUser", EdgeType::References)
        .unwrap_err();
    assert!(matches!(err, IdeError::CircularDependency(_)), "expected CircularDependency, got {err:?}");
    assert_eq!(graph.edge_count(), 1, "the rejected edge must not be kept");
}

#[test]
fn spec_reference_to_a_missing_spec_is_reported() {
    let (dir, wm) = temp_workspace();
    wm.write_file(
        "specs/api/users.yaml",
        "api:\n  name: getUser\n  method: GET\n  path: /api/users/{id}\n",
    )
    .unwrap();
    wm.write_file(
        "specs/ui/users.yaml",
        "component:\n  name: UserProfileView\n  type: Page\n  api_dependencies:\n    - getUser\n    - deleteUser\n",
    )
    .unwrap();

    let mut registry = SpecRegistry::new();
    let warnings = registry.load_from_workspace(&wm).unwrap();

    assert!(
        warnings.iter().any(|w| w.contains("UserProfileView") && w.contains("missing API 'deleteUser'")),
        "expected a missing-reference warning, got {warnings:?}"
    );
    assert!(
        !warnings.iter().any(|w| w.contains("missing API 'getUser'")),
        "getUser exists, so it must not be reported: {warnings:?}"
    );

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn patch_paths_with_parent_traversal_are_rejected() {
    let (dir, wm) = temp_workspace();
    let escape_target = dir.parent().unwrap().join(format!("ai_ide_escape_{}.txt", uuid::Uuid::new_v4()));

    let patch = FilePatch {
        path: format!("../{}", escape_target.file_name().unwrap().to_string_lossy()),
        action: PatchAction::Create,
        original_content: None,
        patched_content: "should never be written\n".into(),
        unified_diff: String::new(),
    };

    let err = PatchEngine::apply_patch(&wm, &patch).unwrap_err();
    assert!(matches!(err, IdeError::Security(_)), "expected Security error, got {err:?}");
    assert!(!escape_target.exists(), "a traversal patch must not write outside the workspace");

    let _ = std::fs::remove_dir_all(dir);
}
