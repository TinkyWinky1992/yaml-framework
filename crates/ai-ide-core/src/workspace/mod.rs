use crate::error::{IdeError, IdeResult};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorkspaceManager {
    root_dir: PathBuf,
}

impl WorkspaceManager {
    pub fn new<P: AsRef<Path>>(root_dir: P) -> IdeResult<Self> {
        let abs_path = std::fs::canonicalize(root_dir.as_ref())
            .map_err(|e| IdeError::Workspace(format!("Failed to resolve workspace path: {}", e)))?;
        
        if !abs_path.is_dir() {
            return Err(IdeError::Workspace("Workspace path is not a directory".into()));
        }

        Ok(Self { root_dir: abs_path })
    }

    pub fn root(&self) -> &Path {
        &self.root_dir
    }

    pub fn specs_dir(&self) -> PathBuf {
        self.root_dir.join("specs")
    }

    pub fn resolve_safe_path<P: AsRef<Path>>(&self, relative_path: P) -> IdeResult<PathBuf> {
        let path = relative_path.as_ref();

        // Check for suspicious components like '..' or absolute paths
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    return Err(IdeError::Security(format!(
                        "Path traversal attempt detected in '{}'",
                        path.display()
                    )));
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(IdeError::Security(format!(
                        "Absolute path is not permitted in relative workspace access: '{}'",
                        path.display()
                    )));
                }
                _ => {}
            }
        }

        let combined = self.root_dir.join(path);
        Ok(combined)
    }

    pub fn read_file<P: AsRef<Path>>(&self, relative_path: P) -> IdeResult<String> {
        let target = self.resolve_safe_path(relative_path)?;
        std::fs::read_to_string(&target).map_err(|e| {
            IdeError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to read file '{}': {}", target.display(), e),
            ))
        })
    }

    pub fn write_file<P: AsRef<Path>>(&self, relative_path: P, content: &str) -> IdeResult<()> {
        let target = self.resolve_safe_path(relative_path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, content)?;
        Ok(())
    }

    pub fn list_spec_files(&self) -> IdeResult<Vec<PathBuf>> {
        let specs_path = self.specs_dir();
        if !specs_path.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        self.collect_yaml_files(&specs_path, &mut results)?;
        results.sort();
        Ok(results)
    }

    /// Lists files under a workspace-relative directory as forward-slash relative paths,
    /// skipping dependency and build directories.
    pub fn list_files_under(&self, relative_dir: &str) -> IdeResult<Vec<String>> {
        let dir = self.resolve_safe_path(relative_dir)?;
        let mut acc = Vec::new();
        if dir.is_dir() {
            self.collect_files(&dir, &mut acc)?;
        }
        let mut rel: Vec<String> = acc
            .into_iter()
            .filter_map(|p| {
                p.strip_prefix(&self.root_dir)
                    .ok()
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
            })
            .collect();
        rel.sort();
        Ok(rel)
    }

    fn collect_files(&self, dir: &Path, acc: &mut Vec<PathBuf>) -> IdeResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !matches!(name, "node_modules" | "target" | "dist" | ".git") {
                    self.collect_files(&path, acc)?;
                }
            } else {
                acc.push(path);
            }
        }
        Ok(())
    }

    fn collect_yaml_files(&self, dir: &Path, acc: &mut Vec<PathBuf>) -> IdeResult<()> {
        if !dir.is_dir() {
            return Ok(());
        }
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.collect_yaml_files(&path, acc)?;
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext == "yaml" || ext == "yml" {
                    acc.push(path);
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
    fn test_path_traversal_detection() {
        let temp_dir = std::env::temp_dir();
        let wm = WorkspaceManager::new(&temp_dir).unwrap();

        assert!(wm.resolve_safe_path("../etc/passwd").is_err());
        assert!(wm.resolve_safe_path("specs/../../root").is_err());
        assert!(wm.resolve_safe_path("specs/api/users.yaml").is_ok());
    }
}
