// Mirrors crates/ai-ide-core/src/models/mod.rs (serde field names).

export type SpecCategory = 'system' | 'api' | 'ui' | 'database' | 'businesslogic' | 'security';

export interface SpecLocation {
  file_path: string;
  line: number;
  column: number;
}

export interface SpecNode {
  id: string;
  category: SpecCategory;
  name: string;
  file_path: string;
  raw_yaml: string;
  content: unknown;
  location: SpecLocation;
}

export type ChangeKind = 'added' | 'modified' | 'removed';

export interface SemanticChange {
  node_id: string;
  category: SpecCategory;
  kind: ChangeKind;
  path: string[];
  old_value: unknown | null;
  new_value: unknown | null;
  description: string;
}

export interface ImpactResult {
  directly_affected: string[];
  transitively_affected: string[];
  verification_only: string[];
  unaffected_count: number;
}

export type PatchAction = 'create' | 'modify' | 'delete';

export interface FilePatch {
  path: string;
  action: PatchAction;
  original_content: string | null;
  patched_content: string;
  unified_diff: string;
}

export interface ImplementationPlan {
  summary: string;
  rationale: string;
  patches: FilePatch[];
  affected_dependencies: string[];
  suggested_tests: string[];
  potential_risks: string[];
}

export interface CheckResult {
  name: string;
  command: string;
  success: boolean;
  exit_code: number | null;
  stdout: string;
  stderr: string;
  duration_ms: number;
}

export interface ValidationSummary {
  overall_success: boolean;
  checks: CheckResult[];
}

export type DriftSeverity = 'info' | 'warning' | 'error';

export interface DriftDiagnostic {
  spec_id: string;
  spec_file: string;
  target_file: string;
  severity: DriftSeverity;
  code_symbol: string | null;
  message: string;
  expected: string;
  found: string | null;
}

export interface GenerationRecord {
  id: string;
  timestamp: string;
  spec_version_hash: string;
  plan_summary: string;
  patches: FilePatch[];
  git_base_commit: string | null;
  status: string;
}

export interface WorkspaceResponse {
  root: string;
  files: string[];
}

export interface ProviderInfo {
  id: string;
  label: string;
  available: boolean;
}

export interface ProvidersResponse {
  active: string;
  providers: ProviderInfo[];
}

export interface FileEvent {
  kind: 'created' | 'modified' | 'removed';
  paths: string[];
}

export type BottomTab = 'tests' | 'drift' | 'history' | 'log';

export interface LogEntry {
  id: number;
  time: string;
  level: 'info' | 'success' | 'error';
  message: string;
}

export interface RepairAttempt {
  attempt: number;
  max_attempts: number;
  errors_fed: string[];
  plan: ImplementationPlan;
  validation: ValidationSummary;
}

export interface RepairReport {
  success: boolean;
  attempts: RepairAttempt[];
  applied_patches: FilePatch[];
  error: string | null;
}
