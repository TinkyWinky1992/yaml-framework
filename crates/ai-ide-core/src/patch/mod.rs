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
}
