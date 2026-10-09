use crate::error::IdeResult;
use crate::graph::builder::{declares_identifier, is_test_file};
use crate::models::{DriftDiagnostic, DriftSeverity, SpecCategory, SpecNode};
use crate::workspace::WorkspaceManager;

pub struct DriftDetector;

impl DriftDetector {
    /// Inspects the implementation files for compliance with authoritative specs.
    pub fn scan_for_drift(
        wm: &WorkspaceManager,
        specs: &[&SpecNode],
    ) -> IdeResult<Vec<DriftDiagnostic>> {
        let mut diagnostics = Vec::new();

        for spec in specs {
            match spec.category {
                SpecCategory::Api => {
                    let endpoint_name = spec.content.get("api")
                        .and_then(|a| a.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or(&spec.name);

                    let implementation = wm
                        .list_files_under("src")?
                        .into_iter()
                        .filter(|f| !is_test_file(f))
                        .find_map(|f| {
                            wm.read_file(&f)
                                .ok()
                                .filter(|c| declares_identifier(c, endpoint_name))
                                .map(|c| (f, c))
                        });

                    match implementation {
                        Some((candidate_file, content)) => {
                            // Check response fields
                            if let Some(responses) = spec.content.get("api").and_then(|a| a.get("responses")) {
                                if let Some(ok_resp) = responses.get("200") {
                                    if let Some(schema) = ok_resp.get("schema") {
                                        if let Some(schema_obj) = schema.as_object() {
                                            for key in schema_obj.keys() {
                                                if !content.contains(key) {
                                                    diagnostics.push(DriftDiagnostic {
                                                        spec_id: spec.id.clone(),
                                                        spec_file: spec.file_path.clone(),
                                                        target_file: candidate_file.clone(),
                                                        severity: DriftSeverity::Warning,
                                                        code_symbol: Some(key.clone()),
                                                        message: format!("Response field '{}' defined in spec is not returned in implementation", key),
                                                        expected: format!("Property '{}'", key),
                                                        found: None,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        None => {
                            diagnostics.push(DriftDiagnostic {
                                spec_id: spec.id.clone(),
                                spec_file: spec.file_path.clone(),
                                target_file: "src/".into(),
                                severity: DriftSeverity::Error,
                                code_symbol: Some(endpoint_name.to_string()),
                                message: format!("No source file under src/ declares API '{}'", endpoint_name),
                                expected: format!("Exported function or route handler named '{}'", endpoint_name),
                                found: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specs::parser::SpecParser;

    #[test]
    fn test_drift_detection_missing_endpoint() {
        let temp_dir = std::env::temp_dir();
        let wm = WorkspaceManager::new(&temp_dir).unwrap();

        let yaml = r#"
api:
  name: getUser
  method: GET
  path: /api/users/{id}
  responses:
    200:
      schema: UserResponse
"#;
        let node = SpecParser::parse_yaml_str(yaml, "specs/api/users.yaml").unwrap();
        let diagnostics = DriftDetector::scan_for_drift(&wm, &[&node]).unwrap();

        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].severity, DriftSeverity::Error);
    }

    #[test]
    fn test_drift_finds_declaring_file_and_missing_field() {
        let dir = std::env::temp_dir().join(format!("ai_ide_drift_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let wm = WorkspaceManager::new(&dir).unwrap();
        wm.write_file("src/routes/users.ts", "export function getUser() { return { id: '1' }; }\n").unwrap();

        let yaml = "api:\n  name: getUser\n  responses:\n    200:\n      schema:\n        id: string\n        phone: string\n";
        let node = SpecParser::parse_yaml_str(yaml, "specs/api/users.yaml").unwrap();
        let diagnostics = DriftDetector::scan_for_drift(&wm, &[&node]).unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, DriftSeverity::Warning);
        assert_eq!(diagnostics[0].target_file, "src/routes/users.ts");
        assert_eq!(diagnostics[0].code_symbol.as_deref(), Some("phone"));

        let _ = std::fs::remove_dir_all(dir);
    }
}
