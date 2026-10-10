//! Live file watching. Raw filesystem notifications become filtered, debounced
//! `created` / `modified` / `removed` events with workspace-relative, forward-slash paths.

use crate::error::{IdeError, IdeResult};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Quiet time after the last change before a batch is published.
pub const DEBOUNCE: Duration = Duration::from_millis(200);

/// Directories whose contents are never reported (build output, dependencies, VCS).
const IGNORED_DIRS: [&str; 4] = ["target", "node_modules", "dist", ".git"];
/// The history database and its SQLite journal files, which the IDE itself writes.
const IGNORED_FILE_PREFIX: &str = ".ai-ide-history.db";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileEventKind {
    Created,
    Modified,
    Removed,
}

/// One published batch: every path that had `kind` in the debounce window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileEvent {
    pub kind: FileEventKind,
    pub paths: Vec<String>,
}

/// True for paths under build, dependency or VCS directories, and for the history database.
pub fn is_ignored(relative: &str) -> bool {
    let parts: Vec<&str> = relative.split('/').collect();
    let last = parts.len().saturating_sub(1);
    parts.iter().enumerate().any(|(i, part)| {
        (i < last && IGNORED_DIRS.contains(part)) || part.starts_with(IGNORED_FILE_PREFIX)
    })
}

/// Maps a notify event kind to ours. Access and metadata-only events are not reported.
pub fn kind_of(kind: &EventKind) -> Option<FileEventKind> {
    match kind {
        EventKind::Create(_) => Some(FileEventKind::Created),
        EventKind::Modify(_) => Some(FileEventKind::Modified),
        EventKind::Remove(_) => Some(FileEventKind::Removed),
        _ => None,
    }
}

/// Returns `path` relative to `root` with forward slashes, or `None` if it lies outside `root`.
///
/// Windows canonical paths carry a `\\?\` prefix, so both sides are normalised before comparing.
pub fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let root = normalize(root);
    let path = normalize(path);
    let prefix = format!("{}/", root.trim_end_matches('/'));
    path.strip_prefix(prefix.as_str()).map(str::to_string)
}

fn normalize(path: &Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
}

/// Combines two events for the same path inside one batch. A file created and then edited
/// is still reported as created; any other sequence reports the latest kind.
fn merge(previous: FileEventKind, next: FileEventKind) -> FileEventKind {
    match (previous, next) {
        (FileEventKind::Created, FileEventKind::Modified) => FileEventKind::Created,
        (_, next) => next,
    }
}

/// Changes collected during one debounce window.
#[derive(Debug, Default)]
pub struct PendingBatch {
    changes: BTreeMap<String, FileEventKind>,
}

impl PendingBatch {
    pub fn push(&mut self, kind: FileEventKind, path: String) {
        let merged = match self.changes.get(&path) {
            Some(previous) => merge(*previous, kind),
            None => kind,
        };
        self.changes.insert(path, merged);
    }

    /// Groups the batch into one event per kind, in a stable order.
    pub fn into_events(self) -> Vec<FileEvent> {
        let mut by_kind: BTreeMap<FileEventKind, BTreeSet<String>> = BTreeMap::new();
        for (path, kind) in self.changes {
            by_kind.entry(kind).or_default().insert(path);
        }
        by_kind
            .into_iter()
            .map(|(kind, paths)| FileEvent {
                kind,
                paths: paths.into_iter().collect(),
            })
            .collect()
    }
}

/// Keeps a recursive watch on a workspace alive. Dropping it stops the watch.
pub struct WorkspaceWatcher {
    _watcher: RecommendedWatcher,
}

impl WorkspaceWatcher {
    /// Watches `root` and publishes debounced events on `sender`.
    /// Must be called inside a Tokio runtime.
    pub fn start(root: &Path, sender: broadcast::Sender<FileEvent>) -> IdeResult<Self> {
        let (raw_tx, raw_rx) = mpsc::unbounded_channel::<(FileEventKind, String)>();
        let root_for_callback = root.to_path_buf();

        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<Event>| {
                let Ok(event) = result else { return };
                let Some(kind) = kind_of(&event.kind) else { return };
                for path in &event.paths {
                    // A changed file also bumps its parent directory's mtime; that is not a file change.
                    if path.is_dir() {
                        continue;
                    }
                    if let Some(relative) = relative_path(&root_for_callback, path) {
                        if !is_ignored(&relative) {
                            let _ = raw_tx.send((kind, relative));
                        }
                    }
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| IdeError::Workspace(format!("Failed to start file watcher: {}", e)))?;

        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|e| IdeError::Workspace(format!("Failed to watch {}: {}", root.display(), e)))?;

        tokio::spawn(debounce_loop(raw_rx, sender));
        Ok(Self { _watcher: watcher })
    }
}

async fn debounce_loop(
    mut raw: mpsc::UnboundedReceiver<(FileEventKind, String)>,
    sender: broadcast::Sender<FileEvent>,
) {
    loop {
        let Some((kind, path)) = raw.recv().await else { break };
        let mut batch = PendingBatch::default();
        batch.push(kind, path);

        // Keep collecting until no change arrives for DEBOUNCE.
        let mut closed = false;
        loop {
            match tokio::time::timeout(DEBOUNCE, raw.recv()).await {
                Ok(Some((kind, path))) => batch.push(kind, path),
                Ok(None) => {
                    closed = true;
                    break;
                }
                Err(_) => break,
            }
        }

        for event in batch.into_events() {
            // No subscribers is fine: nobody is connected yet.
            let _ = sender.send(event);
        }
        if closed {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, ModifyKind, RemoveKind};

    #[test]
    fn ignores_build_dependency_and_history_paths() {
        assert!(is_ignored("target/debug/ai-ide-server.exe"));
        assert!(is_ignored("frontend/node_modules/react/index.js"));
        assert!(is_ignored("frontend/dist/index.html"));
        assert!(is_ignored(".git/HEAD"));
        assert!(is_ignored(".ai-ide-history.db"));
        assert!(is_ignored(".ai-ide-history.db-wal"));

        assert!(!is_ignored("src/routes/users.ts"));
        assert!(!is_ignored("specs/api/users.yaml"));
        // A file that happens to be named like a directory is still reported.
        assert!(!is_ignored("src/dist"));
    }

    #[test]
    fn maps_notify_kinds_and_skips_access_events() {
        assert_eq!(kind_of(&EventKind::Create(CreateKind::File)), Some(FileEventKind::Created));
        assert_eq!(kind_of(&EventKind::Modify(ModifyKind::Any)), Some(FileEventKind::Modified));
        assert_eq!(kind_of(&EventKind::Remove(RemoveKind::File)), Some(FileEventKind::Removed));
        assert_eq!(kind_of(&EventKind::Access(notify::event::AccessKind::Any)), None);
    }

    #[test]
    fn relative_paths_use_forward_slashes_and_handle_windows_prefix() {
        let root = Path::new(r"\\?\C:\work\ai-ide");
        let file = Path::new(r"C:\work\ai-ide\src\routes\users.ts");
        assert_eq!(relative_path(root, file).as_deref(), Some("src/routes/users.ts"));
        assert_eq!(relative_path(Path::new("/w/proj"), Path::new("/w/proj/src/a.ts")).as_deref(), Some("src/a.ts"));
        assert_eq!(relative_path(Path::new("/w/proj"), Path::new("/w/other/a.ts")), None);
    }

    #[test]
    fn batch_merges_repeated_changes_and_groups_by_kind() {
        let mut batch = PendingBatch::default();
        batch.push(FileEventKind::Modified, "src/routes/users.ts".into());
        batch.push(FileEventKind::Modified, "src/routes/users.ts".into());
        batch.push(FileEventKind::Created, "src/new.ts".into());
        batch.push(FileEventKind::Created, "src/temp.ts".into());
        batch.push(FileEventKind::Modified, "src/temp.ts".into());

        let events = batch.into_events();
        assert_eq!(
            events,
            vec![
                FileEvent {
                    kind: FileEventKind::Created,
                    paths: vec!["src/new.ts".into(), "src/temp.ts".into()],
                },
                FileEvent {
                    kind: FileEventKind::Modified,
                    paths: vec!["src/routes/users.ts".into()],
                },
            ]
        );
    }

    #[test]
    fn file_event_serializes_to_frontend_shape() {
        let json = serde_json::to_value(FileEvent {
            kind: FileEventKind::Modified,
            paths: vec!["src/routes/users.ts".into()],
        })
        .unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "modified", "paths": ["src/routes/users.ts"] }));
    }
}
