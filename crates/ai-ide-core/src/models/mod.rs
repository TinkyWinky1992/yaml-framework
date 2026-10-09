use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpecCategory {
    System,
    Api,
    Ui,
    Database,
    BusinessLogic,
    Security,
}

impl SpecCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Api => "api",
            Self::Ui => "ui",
            Self::Database => "database",
            Self::BusinessLogic => "business-logic",
            Self::Security => "security",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecLocation {
    pub file_path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecNode {
    pub id: String,
    pub category: SpecCategory,
    pub name: String,
    pub file_path: String,
    pub raw_yaml: String,
    pub content: serde_json::Value,
    pub location: SpecLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticChange {
    pub node_id: String,
    pub category: SpecCategory,
    pub kind: ChangeKind,
    pub path: Vec<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ChangeSet {
    pub changed_specs: Vec<SemanticChange>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImpactCategory {
    Direct,
    Transitive,
    VerificationOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ImpactAnalysis {
    pub changes: Vec<SemanticChange>,
    pub directly_affected_files: Vec<String>,
    pub transitively_affected_files: Vec<String>,
    pub verification_only_files: Vec<String>,
    pub unaffected_file_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchAction {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilePatch {
    pub path: String,
    pub action: PatchAction,
    pub original_content: Option<String>,
    pub patched_content: String,
    pub unified_diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ImplementationPlan {
    pub summary: String,
    pub rationale: String,
    pub patches: Vec<FilePatch>,
    pub affected_dependencies: Vec<String>,
    pub suggested_tests: Vec<String>,
    pub potential_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task_description: String,
    pub spec_changes: Vec<SemanticChange>,
    pub relevant_specs: Vec<SpecNode>,
    pub impacted_files: HashMap<String, String>,
    pub conventions: serde_json::Value,
    pub failing_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckResult {
    pub name: String,
    pub command: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationSummary {
    pub overall_success: bool,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepairAttempt {
    pub attempt: usize,
    pub max_attempts: usize,
    pub errors_fed: Vec<String>,
    pub plan: ImplementationPlan,
    pub validation: ValidationSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DriftSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriftDiagnostic {
    pub spec_id: String,
    pub spec_file: String,
    pub target_file: String,
    pub severity: DriftSeverity,
    pub code_symbol: Option<String>,
    pub message: String,
    pub expected: String,
    pub found: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub spec_version_hash: String,
    pub plan_summary: String,
    pub patches: Vec<FilePatch>,
    pub git_base_commit: Option<String>,
    pub status: String,
}
