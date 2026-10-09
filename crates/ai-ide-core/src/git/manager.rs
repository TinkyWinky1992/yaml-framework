use crate::error::{IdeError, IdeResult};
use crate::workspace::WorkspaceManager;
use tokio::process::Command;

pub struct GitManager;

impl GitManager {
    async fn run_git_cmd(wm: &WorkspaceManager, args: &[&str]) -> IdeResult<String> {
        let mut cmd = Command::new("git");
        cmd.args(args);
        cmd.current_dir(wm.root());

        let output = cmd.output().await.map_err(|e| {
            IdeError::Git(format!("Failed to execute 'git {}': {}", args.join(" "), e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IdeError::Git(format!(
                "Git command failed: git {}\n{}",
                args.join(" "),
                stderr
            )));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    pub async fn get_current_branch(wm: &WorkspaceManager) -> IdeResult<String> {
        Self::run_git_cmd(wm, &["branch", "--show-current"]).await
    }

    pub async fn get_head_commit(wm: &WorkspaceManager) -> IdeResult<String> {
        Self::run_git_cmd(wm, &["rev-parse", "HEAD"]).await
    }

    pub async fn get_status(wm: &WorkspaceManager) -> IdeResult<String> {
        Self::run_git_cmd(wm, &["status", "--porcelain"]).await
    }

    pub async fn get_diff(wm: &WorkspaceManager) -> IdeResult<String> {
        Self::run_git_cmd(wm, &["diff"]).await
    }

    pub async fn stage_all(wm: &WorkspaceManager) -> IdeResult<()> {
        Self::run_git_cmd(wm, &["add", "."]).await?;
        Ok(())
    }

    pub async fn commit(wm: &WorkspaceManager, message: &str) -> IdeResult<String> {
        Self::stage_all(wm).await?;
        Self::run_git_cmd(wm, &["commit", "-m", message]).await?;
        Self::get_head_commit(wm).await
    }

    /// Creates a checkpoint branch before applying AI changes.
    pub async fn create_checkpoint(wm: &WorkspaceManager, checkpoint_name: &str) -> IdeResult<()> {
        Self::run_git_cmd(wm, &["checkout", "-b", checkpoint_name]).await?;
        Ok(())
    }

    /// Reverts all working tree changes back to HEAD safely.
    pub async fn rollback_working_tree(wm: &WorkspaceManager) -> IdeResult<()> {
        Self::run_git_cmd(wm, &["checkout", "--", "."]).await?;
        // Clean untracked files generated during the session
        Self::run_git_cmd(wm, &["clean", "-fd"]).await?;
        Ok(())
    }
}
