use crate::ai::provider::AiProvider;
use crate::error::IdeResult;
use crate::models::{RepairAttempt, TaskContext};
use crate::patch::PatchEngine;

use crate::validation::runner::ValidationRunner;
use crate::workspace::WorkspaceManager;

pub struct RepairOrchestrator;

impl RepairOrchestrator {
    pub async fn run_bounded_repair(
        wm: &WorkspaceManager,
        provider: &dyn AiProvider,
        context: &TaskContext,
        checks: &[(&str, &str)],
        max_attempts: usize,
    ) -> IdeResult<(bool, Vec<RepairAttempt>)> {
        let mut attempts = Vec::new();

        // Initial validation
        let initial_val = ValidationRunner::run_all(wm, checks).await?;
        if initial_val.overall_success {
            return Ok((true, attempts));
        }

        let mut current_val = initial_val;

        for attempt_idx in 1..=max_attempts {
            // Collect failure messages
            let failures: Vec<String> = current_val.checks.iter()
                .filter(|c| !c.success)
                .map(|c| format!("Check '{}' failed (exit {:?}):\nSTDOUT: {}\nSTDERR: {}", c.name, c.exit_code, c.stdout, c.stderr))
                .collect();

            // Ask AI to generate repair patch
            let repair_plan = provider.repair_patch(context, &failures).await?;

            // Apply repair patches
            for patch in &repair_plan.patches {
                PatchEngine::apply_patch(wm, patch)?;
            }

            // Re-run checks
            let next_val = ValidationRunner::run_all(wm, checks).await?;
            let success = next_val.overall_success;

            attempts.push(RepairAttempt {
                attempt: attempt_idx,
                max_attempts,
                errors_fed: failures,
                plan: repair_plan,
                validation: next_val.clone(),
            });

            if success {
                return Ok((true, attempts));
            }

            current_val = next_val;
        }

        Ok((false, attempts))
    }
}
