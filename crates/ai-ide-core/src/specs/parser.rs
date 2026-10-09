use crate::error::{IdeError, IdeResult};
use crate::models::{SpecCategory, SpecLocation, SpecNode};
use std::path::Path;

pub struct SpecParser;

impl SpecParser {
    pub fn infer_category(path: &Path, content: &serde_json::Value) -> SpecCategory {
        let path_str = path.to_string_lossy().replace('\\', "/");
        if path_str.contains("specs/system.yaml") || path_str.ends_with("system.yaml") {
            return SpecCategory::System;
        }
        if path_str.contains("specs/api/") || content.get("api").is_some() {
            return SpecCategory::Api;
        }
        if path_str.contains("specs/ui/") || content.get("component").is_some() {
            return SpecCategory::Ui;
        }
        if path_str.contains("specs/database/") || content.get("entity").is_some() {
            return SpecCategory::Database;
        }
        if path_str.contains("specs/business-logic/") || content.get("workflow").is_some() {
            return SpecCategory::BusinessLogic;
        }
        if path_str.contains("specs/security/") || content.get("permissions").is_some() {
            return SpecCategory::Security;
        }
        SpecCategory::Api
    }

    pub fn parse_yaml_str(yaml_str: &str, file_path: &str) -> IdeResult<SpecNode> {
        let yaml_val: serde_yml::Value = serde_yml::from_str(yaml_str).map_err(|e| {
            let (line, column) = if let Some(loc) = e.location() {
                (loc.line(), loc.column())
            } else {
                (1, 1)
            };
            IdeError::SpecParse {
                file: file_path.to_string(),
                line,
                column,
                message: e.to_string(),
            }
        })?;

        let json_val: serde_json::Value = serde_json::to_value(yaml_val)
            .map_err(|e| IdeError::Serialization(e))?;

        let category = Self::infer_category(Path::new(file_path), &json_val);

        // Derive node name and ID
        let name = if let Some(api) = json_val.get("api").and_then(|a| a.get("name")).and_then(|n| n.as_str()) {
            api.to_string()
        } else if let Some(entity) = json_val.get("entity").and_then(|e| e.get("name")).and_then(|n| n.as_str()) {
            entity.to_string()
        } else if let Some(comp) = json_val.get("component").and_then(|c| c.get("name")).and_then(|n| n.as_str()) {
            comp.to_string()
        } else if let Some(wf) = json_val.get("workflow").and_then(|w| w.get("name")).and_then(|n| n.as_str()) {
            wf.to_string()
        } else if let Some(proj) = json_val.get("project").and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
            proj.to_string()
        } else {
            Path::new(file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        };

        let id = format!("{}:{}", category.as_str(), name);

        Ok(SpecNode {
            id,
            category,
            name,
            file_path: file_path.to_string(),
            raw_yaml: yaml_str.to_string(),
            content: json_val,
            location: SpecLocation {
                file_path: file_path.to_string(),
                line: 1,
                column: 1,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_spec() {
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
        assert_eq!(node.category, SpecCategory::Api);
        assert_eq!(node.name, "getUser");
        assert_eq!(node.id, "api:getUser");
    }
}
