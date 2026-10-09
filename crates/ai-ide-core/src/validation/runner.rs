use crate::error::{IdeError, IdeResult};
use crate::models::{CheckResult, ValidationSummary};
use crate::workspace::WorkspaceManager;
use std::time::Instant;
use tokio::process::Command;

pub struct ValidationRunner;

impl ValidationRunner {
    pub async fn run_check(
        wm: &WorkspaceManager,
        name: &str,
        command_line: &str,
    ) -> IdeResult<CheckResult> {
        let start = Instant::now();

        // cmd.exe does not understand the \"-escaping std applies to args, so pass the line verbatim.
        #[cfg(target_os = "windows")]
        let mut cmd = Command::new("cmd");
        #[cfg(target_os = "windows")]
        cmd.arg("/C").raw_arg(command_line);

        #[cfg(not(target_os = "windows"))]
        let mut cmd = Command::new("sh");
        #[cfg(not(target_os = "windows"))]
        cmd.args(["-c", command_line]);

        cmd.current_dir(wm.root());

        let output = cmd.output().await.map_err(|e| IdeError::ValidationFailed {
            code: -1,
            output: format!("Failed to spawn command '{}': {}", command_line, e),
        })?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let success = output.status.success();
        let exit_code = output.status.code();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(CheckResult {
            name: name.to_string(),
            command: command_line.to_string(),
            success,
            exit_code,
            stdout,
            stderr,
            duration_ms,
        })
    }

    pub async fn run_all(
        wm: &WorkspaceManager,
        checks: &[(&str, &str)],
    ) -> IdeResult<ValidationSummary> {
        let mut results = Vec::new();
        let mut overall_success = true;

        for (name, cmd) in checks {
            let res = Self::run_check(wm, name, cmd).await?;
            if !res.success {
                overall_success = false;
            }
            results.push(res);
        }

        Ok(ValidationSummary {
            overall_success,
            checks: results,
        })
    }
}
