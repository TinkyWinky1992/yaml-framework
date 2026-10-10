use crate::error::{IdeError, IdeResult};
use crate::models::{FilePatch, PatchAction};
use crate::workspace::WorkspaceManager;
use similar::TextDiff;

pub struct PatchEngine;

impl PatchEngine {
    pub fn create_diff(
        path: &str,
        action: PatchAction,
        original_content: Option<&str>,
        patched_content: &str,
    ) -> FilePatch {
        let orig = original_content.unwrap_or("");
        let diff = TextDiff::from_lines(orig, patched_content);
        let unified_diff = diff
            .unified_diff()
            .header(path, path)
            .to_string();

        FilePatch {
            path: path.to_string(),
            action,
            original_content: original_content.map(|s| s.to_string()),
            patched_content: patched_content.to_string(),
            unified_diff,
        }
    }

    /// Merges patches that touch the same file into one, keeping the first original and the last
    /// patched content. Rollback checks each recorded patch against the file's final state, so
    /// a multi-step repair has to be stored as one patch per file.
    pub fn collapse_patches(patches: &[FilePatch]) -> Vec<FilePatch> {
        let mut collapsed: Vec<FilePatch> = Vec::new();
        for patch in patches {
            match collapsed.iter_mut().find(|p| p.path == patch.path) {
                Some(existing) => {
                    let original = existing.original_content.clone();
                    *existing = Self::create_diff(&patch.path, existing.action, original.as_deref(), &patch.patched_content);
                }
                None => collapsed.push(patch.clone()),
            }
        }
        collapsed
    }

    /// Checks for conflicts between patch base content and current disk content.
    /// Never silently overwrites manual modifications.
    pub fn check_conflict(wm: &WorkspaceManager, patch: &FilePatch) -> IdeResult<()> {
        let target_path = wm.resolve_safe_path(&patch.path)?;

        match patch.action {
            PatchAction::Create => {
                if target_path.exists() {
                    return Err(IdeError::PatchConflict {
                        file: patch.path.clone(),
                        message: "Target file already exists on disk. Manual creation conflict.".into(),
                    });
                }
            }
            PatchAction::Modify => {
                if !target_path.exists() {
                    return Err(IdeError::PatchConflict {
                        file: patch.path.clone(),
                        message: "Target file does not exist on disk for modification.".into(),
                    });
                }
                if let Some(expected_base) = &patch.original_content {
                    let disk_content = wm.read_file(&patch.path)?;
                    if disk_content != *expected_base {
                        return Err(IdeError::PatchConflict {
                            file: patch.path.clone(),
                            message: "File has been modified on disk since generation base. Preserving manual changes.".into(),
                        });
                    }
                }
            }
            PatchAction::Delete => {
                if !target_path.exists() {
                    return Err(IdeError::PatchConflict {
                        file: patch.path.clone(),
                        message: "File to delete does not exist on disk.".into(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Atomically applies an approved patch to the workspace after verifying no conflicts.
    pub fn apply_patch(wm: &WorkspaceManager, patch: &FilePatch) -> IdeResult<()> {
        Self::check_conflict(wm, patch)?;

        match patch.action {
            PatchAction::Create | PatchAction::Modify => {
                wm.write_file(&patch.path, &patch.patched_content)?;
            }
            PatchAction::Delete => {
                let target = wm.resolve_safe_path(&patch.path)?;
                if target.exists() {
                    std::fs::remove_file(target)?;
                }
            }
        }
        Ok(())
    }

    /// Checks that a previously applied patch can be undone: the file must still hold
    /// exactly what the patch wrote, so manual edits made afterwards are never lost.
    pub fn check_revert(wm: &WorkspaceManager, patch: &FilePatch) -> IdeResult<()> {
        let target_path = wm.resolve_safe_path(&patch.path)?;
        let conflict = |message: &str| IdeError::PatchConflict {
            file: patch.path.clone(),
            message: message.into(),
        };

        match patch.action {
            PatchAction::Create | PatchAction::Modify => {
                if !target_path.exists() {
                    return Err(conflict("File no longer exists; cannot undo the AI change."));
                }
                if wm.read_file(&patch.path)? != patch.patched_content {
                    return Err(conflict("File was edited after the AI change was applied. Preserving manual changes."));
                }
                if patch.action == PatchAction::Modify && patch.original_content.is_none() {
                    return Err(conflict("No original content recorded; cannot restore."));
                }
            }
            PatchAction::Delete => {
                if target_path.exists() {
                    return Err(conflict("File was recreated after the AI deleted it. Preserving manual changes."));
                }
                if patch.original_content.is_none() {
                    return Err(conflict("No original content recorded; cannot restore."));
                }
            }
        }
        Ok(())
    }

    /// Undoes an applied patch: removes created files and restores modified/deleted ones.
    pub fn revert_patch(wm: &WorkspaceManager, patch: &FilePatch) -> IdeResult<()> {
        Self::check_revert(wm, patch)?;

        match patch.action {
            PatchAction::Create => {
                std::fs::remove_file(wm.resolve_safe_path(&patch.path)?)?;
            }
            PatchAction::Modify | PatchAction::Delete => {
                wm.write_file(&patch.path, patch.original_content.as_deref().unwrap_or_default())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_diff_generation() {
        let old = "function getUser() {\n  return { id: 1 };\n}\n";
        let new = "function getUser() {\n  return { id: 1, phone: \"555\" };\n}\n";
        let patch = PatchEngine::create_diff("src/users.ts", PatchAction::Modify, Some(old), new);

        assert!(patch.unified_diff.contains("+  return { id: 1, phone: \"555\" };"));
        assert!(patch.unified_diff.contains("-  return { id: 1 };"));
    }

    #[test]
    fn test_collapse_keeps_first_original_and_last_patched() {
        let first = PatchEngine::create_diff("src/a.ts", PatchAction::Modify, Some("v0\n"), "v1\n");
        let second = PatchEngine::create_diff("src/a.ts", PatchAction::Modify, Some("v1\n"), "v2\n");
        let other = PatchEngine::create_diff("src/b.ts", PatchAction::Create, None, "b\n");

        let collapsed = PatchEngine::collapse_patches(&[first, other, second]);
        assert_eq!(collapsed.len(), 2);
        assert_eq!(collapsed[0].original_content.as_deref(), Some("v0\n"));
        assert_eq!(collapsed[0].patched_content, "v2\n");
        assert!(collapsed[0].unified_diff.contains("-v0"));
        assert!(collapsed[0].unified_diff.contains("+v2"));
        assert_eq!(collapsed[1].path, "src/b.ts");
    }

    #[test]
    fn test_revert_restores_files_and_protects_manual_edits() {
        let dir = std::env::temp_dir().join(format!("ai_ide_revert_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let wm = WorkspaceManager::new(&dir).unwrap();
        wm.write_file("src/a.ts", "old\n").unwrap();

        let modify = PatchEngine::create_diff("src/a.ts", PatchAction::Modify, Some("old\n"), "new\n");
        let create = PatchEngine::create_diff("src/b.ts", PatchAction::Create, None, "created\n");
        PatchEngine::apply_patch(&wm, &modify).unwrap();
        PatchEngine::apply_patch(&wm, &create).unwrap();

        PatchEngine::revert_patch(&wm, &modify).unwrap();
        PatchEngine::revert_patch(&wm, &create).unwrap();
        assert_eq!(wm.read_file("src/a.ts").unwrap(), "old\n");
        assert!(!dir.join("src/b.ts").exists());

        // A file edited by hand after the AI change must not be reverted.
        PatchEngine::apply_patch(&wm, &modify).unwrap();
        wm.write_file("src/a.ts", "new\nmanual edit\n").unwrap();
        assert!(PatchEngine::revert_patch(&wm, &modify).is_err());
        assert_eq!(wm.read_file("src/a.ts").unwrap(), "new\nmanual edit\n");

        let _ = std::fs::remove_dir_all(dir);
    }
}
