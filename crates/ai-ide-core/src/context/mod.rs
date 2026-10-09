use crate::error::IdeResult;
use crate::models::{SemanticChange, SpecNode, TaskContext};
use crate::workspace::WorkspaceManager;
use std::collections::HashMap;

pub struct ContextBuilder;

impl ContextBuilder {
    pub fn build(
        wm: &WorkspaceManager,
        description: &str,
        spec_changes: Vec<SemanticChange>,
        relevant_specs: Vec<SpecNode>,
        affected_file_paths: &[String],
        conventions: serde_json::Value,
        failing_checks: Vec<String>,
    ) -> IdeResult<TaskContext> {
        let mut impacted_files = HashMap::new();

        for path in affected_file_paths {
            if let Ok(content) = wm.read_file(path) {
                impacted_files.insert(path.clone(), content);
            }
        }

        Ok(TaskContext {
            task_description: description.to_string(),
            spec_changes,
            relevant_specs,
            impacted_files,
            conventions,
            failing_checks,
        })
    }

    /// Estimates token count roughly (standard 4 chars per token rule of thumb)
    pub fn estimate_tokens(context: &TaskContext) -> usize {
        let mut char_count = 0;
        char_count += context.task_description.len();
        for change in &context.spec_changes {
            char_count += change.description.len();
        }
        for spec in &context.relevant_specs {
            char_count += spec.raw_yaml.len();
        }
        for (path, content) in &context.impacted_files {
            char_count += path.len() + content.len();
        }
        for check in &context.failing_checks {
            char_count += check.len();
        }
        char_count / 4
    }
}
