use crate::ai::provider::AiProvider;
use crate::error::IdeResult;
use crate::models::{FilePatch, RepairAttempt, TaskContext};
use crate::patch::PatchEngine;

use crate::validation::runner::ValidationRunner;
use crate::workspace::WorkspaceManager;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RepairReport {
    pub success: bool,
    pub attempts: Vec<RepairAttempt>,
    /// Every patch written to disk, in order, so the caller can record it for rollback.
    pub applied_patches: Vec<FilePatch>,
    /// Set when the loop stopped early (provider error, write conflict, or a failed check run).
    pub error: Option<String>,
}

pub struct RepairOrchestrator;

impl RepairOrchestrator {
    /// Runs checks, asks the provider for fixes, applies them, and repeats up to `max_attempts`.
    ///
    /// Errors after the first write are returned inside the report, not as `Err`, so the caller
    /// still learns about every patch that reached disk.
    pub async fn run_bounded_repair(
        wm: &WorkspaceManager,
        provider: &dyn AiProvider,
        context: &TaskContext,
        checks: &[(&str, &str)],
        max_attempts: usize,
    ) -> IdeResult<RepairReport> {
        let mut report = RepairReport {
            success: false,
            attempts: Vec::new(),
            applied_patches: Vec::new(),
            error: None,
        };

        let mut current_val = ValidationRunner::run_all(wm, checks).await?;
        if current_val.overall_success {
            report.success = true;
            return Ok(report);
        }

        let mut context = context.clone();
        for attempt_idx in 1..=max_attempts {
            let failures: Vec<String> = current_val.checks.iter()
                .filter(|c| !c.success)
                .map(|c| format!("Check '{}' failed (exit {:?}):\nSTDOUT: {}\nSTDERR: {}", c.name, c.exit_code, c.stdout, c.stderr))
                .collect();

            // Earlier repairs changed files on disk, so send the current contents, not the originals.
            refresh_impacted_files(wm, &mut context);

            let repair_plan = match provider.repair_patch(&context, &failures).await {
                Ok(plan) => plan,
                Err(e) => {
                    report.error = Some(e.to_string());
                    return Ok(report);
                }
            };

            for patch in &repair_plan.patches {
                if let Err(e) = PatchEngine::apply_patch(wm, patch) {
                    report.error = Some(e.to_string());
                    return Ok(report);
                }
                report.applied_patches.push(patch.clone());
            }

            let next_val = match ValidationRunner::run_all(wm, checks).await {
                Ok(val) => val,
                Err(e) => {
                    report.error = Some(e.to_string());
                    return Ok(report);
                }
            };
            let success = next_val.overall_success;

            report.attempts.push(RepairAttempt {
                attempt: attempt_idx,
                max_attempts,
                errors_fed: failures,
                plan: repair_plan,
                validation: next_val.clone(),
            });

            if success {
                report.success = true;
                return Ok(report);
            }

            current_val = next_val;
        }

        Ok(report)
    }
}

/// Reloads every file in the context from disk. Files that no longer exist keep their last content.
fn refresh_impacted_files(wm: &WorkspaceManager, context: &mut TaskContext) {
    for (path, content) in context.impacted_files.iter_mut() {
        if let Ok(fresh) = wm.read_file(path) {
            *content = fresh;
        }
    }
}
