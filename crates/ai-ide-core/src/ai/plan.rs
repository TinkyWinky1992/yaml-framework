//! Plan format shared by the AI providers.
//!
//! The model returns the complete new content of each file it changes. The IDE builds
//! the unified diffs itself, so they are always correct, and every path is checked
//! against the workspace before anything is written.

use crate::error::{IdeError, IdeResult};
use crate::models::{FilePatch, ImplementationPlan, PatchAction, TaskContext};
use crate::patch::PatchEngine;
use crate::workspace::WorkspaceManager;
use serde::Deserialize;
use std::collections::HashSet;

pub const SYSTEM_PROMPT: &str = "You are the code generation engine of an AI spec IDE. \
Specs are the source of truth. Given spec changes and the current files, change the code so it matches the specs. \
Reply with one JSON object only, no prose and no markdown fences, in this shape: \
{\"summary\": string, \"rationale\": string, \"patches\": [{\"path\": string, \"patched_content\": string}], \
\"affected_dependencies\": [string], \"suggested_tests\": [string], \"potential_risks\": [string]}. \
Each patch must contain the COMPLETE new content of that file. Use forward slashes and workspace-relative paths only. \
Never use '..' or absolute paths. Only change files that the task needs.";

/// What the model is asked to return. Diffs and file actions are derived locally.
#[derive(Debug, Deserialize)]
pub struct WirePlan {
    pub summary: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub patches: Vec<WirePatch>,
    #[serde(default)]
    pub affected_dependencies: Vec<String>,
    #[serde(default)]
    pub suggested_tests: Vec<String>,
    #[serde(default)]
    pub potential_risks: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct WirePatch {
    pub path: String,
    pub patched_content: String,
}

/// Builds the user prompt from the task context. Files are sorted so prompts are stable.
pub fn build_prompt(context: &TaskContext) -> String {
    let mut out = format!("## Task\n{}\n\n", context.task_description);

    out.push_str("## Spec changes\n");
    out.push_str(&serde_json::to_string_pretty(&context.spec_changes).unwrap_or_default());
    out.push_str("\n\n");

    if !context.relevant_specs.is_empty() {
        out.push_str("## Specs (YAML)\n");
        for spec in &context.relevant_specs {
            out.push_str(&format!("### {}\n{}\n", spec.file_path, spec.raw_yaml));
        }
        out.push('\n');
    }

    out.push_str("## Current files\n");
    let mut files: Vec<_> = context.impacted_files.iter().collect();
    files.sort_by(|a, b| a.0.cmp(b.0));
    for (path, content) in files {
        out.push_str(&format!("### {}\n```\n{}\n```\n\n", path, content));
    }

    if !context.failing_checks.is_empty() {
        out.push_str("## Failing checks (fix these)\n");
        for failure in &context.failing_checks {
            out.push_str(&format!("{}\n\n", failure));
        }
    }

    out.push_str("Return the JSON object only.");
    out
}

/// Pulls the plan object out of a model reply. Tolerates prose or code fences around it.
pub fn parse_plan_text(text: &str, provider: &str) -> IdeResult<WirePlan> {
    let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) else {
        return Err(provider_error(provider, "response contains no JSON object"));
    };
    serde_json::from_str::<WirePlan>(&text[start..=end])
        .map_err(|e| provider_error(provider, format!("response is not a valid plan: {}", e)))
}

/// Turns a model plan into an `ImplementationPlan`, reading original content from disk.
///
/// Reading from disk (not from a stale snapshot) keeps `PatchEngine`'s conflict checks valid
/// when a repair runs after an earlier patch has already changed a file.
pub fn build_plan(wire: WirePlan, wm: &WorkspaceManager, provider: &str) -> IdeResult<ImplementationPlan> {
    let mut patches: Vec<FilePatch> = Vec::new();
    let mut seen = HashSet::new();

    for wire_patch in wire.patches {
        // Rejects '..' and absolute paths with IdeError::Security.
        wm.resolve_safe_path(&wire_patch.path)?;
        if !seen.insert(wire_patch.path.clone()) {
            return Err(provider_error(provider, format!("plan contains two patches for '{}'", wire_patch.path)));
        }

        let original = wm.read_file(&wire_patch.path).ok();
        if original.as_deref() == Some(wire_patch.patched_content.as_str()) {
            continue;
        }
        let action = if original.is_some() { PatchAction::Modify } else { PatchAction::Create };
        patches.push(PatchEngine::create_diff(
            &wire_patch.path,
            action,
            original.as_deref(),
            &wire_patch.patched_content,
        ));
    }

    let affected: Vec<String> = patches.iter().map(|p| p.path.clone()).collect();
    Ok(ImplementationPlan {
        summary: wire.summary,
        rationale: wire.rationale,
        patches,
        affected_dependencies: if wire.affected_dependencies.is_empty() { affected } else { wire.affected_dependencies },
        suggested_tests: wire.suggested_tests,
        potential_risks: wire.potential_risks,
    })
}

pub fn provider_error(provider: &str, message: impl Into<String>) -> IdeError {
    IdeError::AiProvider {
        provider: provider.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_workspace() -> (PathBuf, WorkspaceManager) {
        let dir = std::env::temp_dir().join(format!("ai_ide_plan_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let wm = WorkspaceManager::new(&dir).unwrap();
        (dir, wm)
    }

    #[test]
    fn parse_plan_text_ignores_prose_and_fences() {
        let text = "Here you go:\n```json\n{\"summary\": \"Add phone\", \"patches\": [{\"path\": \"src/a.ts\", \"patched_content\": \"x\"}]}\n```";
        let wire = parse_plan_text(text, "Test").unwrap();
        assert_eq!(wire.summary, "Add phone");
        assert_eq!(wire.patches[0].path, "src/a.ts");
        assert!(wire.affected_dependencies.is_empty());
    }

    #[test]
    fn parse_plan_text_rejects_non_json() {
        let err = parse_plan_text("I cannot do that.", "Test").unwrap_err();
        assert!(err.to_string().contains("no JSON object"));
    }

    #[test]
    fn build_plan_creates_modifies_and_rejects_traversal() {
        let (dir, wm) = temp_workspace();
        wm.write_file("src/a.ts", "old\n").unwrap();

        let wire = WirePlan {
            summary: "s".into(),
            rationale: String::new(),
            patches: vec![
                WirePatch { path: "src/a.ts".into(), patched_content: "new\n".into() },
                WirePatch { path: "src/b.ts".into(), patched_content: "created\n".into() },
            ],
            affected_dependencies: vec![],
            suggested_tests: vec![],
            potential_risks: vec![],
        };
        let plan = build_plan(wire, &wm, "Test").unwrap();
        assert_eq!(plan.patches[0].action, PatchAction::Modify);
        assert_eq!(plan.patches[0].original_content.as_deref(), Some("old\n"));
        assert!(plan.patches[0].unified_diff.contains("-old"));
        assert_eq!(plan.patches[1].action, PatchAction::Create);
        assert_eq!(plan.affected_dependencies, vec!["src/a.ts", "src/b.ts"]);

        let escape = WirePlan {
            summary: String::new(),
            rationale: String::new(),
            patches: vec![WirePatch { path: "../outside.ts".into(), patched_content: "x".into() }],
            affected_dependencies: vec![],
            suggested_tests: vec![],
            potential_risks: vec![],
        };
        assert!(matches!(build_plan(escape, &wm, "Test"), Err(IdeError::Security(_))));

        let _ = std::fs::remove_dir_all(dir);
    }
}
