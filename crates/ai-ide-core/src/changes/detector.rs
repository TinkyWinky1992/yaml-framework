use crate::models::{ChangeKind, SemanticChange, SpecNode};
use serde_json::Value;

pub struct SemanticChangeDetector;

impl SemanticChangeDetector {
    pub fn diff(old_node: Option<&SpecNode>, new_node: Option<&SpecNode>) -> Vec<SemanticChange> {
        let mut changes = Vec::new();

        match (old_node, new_node) {
            (None, Some(new_n)) => {
                // Entire spec added
                changes.push(SemanticChange {
                    node_id: new_n.id.clone(),
                    category: new_n.category,
                    kind: ChangeKind::Added,
                    path: vec![],
                    old_value: None,
                    new_value: Some(new_n.content.clone()),
                    description: format!("Added specification '{}'", new_n.name),
                });
            }
            (Some(old_n), None) => {
                // Entire spec removed
                changes.push(SemanticChange {
                    node_id: old_n.id.clone(),
                    category: old_n.category,
                    kind: ChangeKind::Removed,
                    path: vec![],
                    old_value: Some(old_n.content.clone()),
                    new_value: None,
                    description: format!("Removed specification '{}'", old_n.name),
                });
            }
            (Some(old_n), Some(new_n)) => {
                let mut current_path = Vec::new();
                Self::diff_values(
                    &old_n.id,
                    new_n.category,
                    &old_n.content,
                    &new_n.content,
                    &mut current_path,
                    &mut changes,
                );
            }
            (None, None) => {}
        }

        changes
    }

    fn diff_values(
        node_id: &str,
        category: crate::models::SpecCategory,
        old_val: &Value,
        new_val: &Value,
        current_path: &mut Vec<String>,
        acc: &mut Vec<SemanticChange>,
    ) {
        if old_val == new_val {
            return;
        }

        match (old_val, new_val) {
            (Value::Object(old_map), Value::Object(new_map)) => {
                // Check removed and modified keys
                for (k, old_v) in old_map {
                    current_path.push(k.clone());
                    if let Some(new_v) = new_map.get(k) {
                        Self::diff_values(node_id, category, old_v, new_v, current_path, acc);
                    } else {
                        acc.push(SemanticChange {
                            node_id: node_id.to_string(),
                            category,
                            kind: ChangeKind::Removed,
                            path: current_path.clone(),
                            old_value: Some(old_v.clone()),
                            new_value: None,
                            description: format!("Removed property '{}'", current_path.join(".")),
                        });
                    }
                    current_path.pop();
                }

                // Check added keys
                for (k, new_v) in new_map {
                    if !old_map.contains_key(k) {
                        current_path.push(k.clone());
                        acc.push(SemanticChange {
                            node_id: node_id.to_string(),
                            category,
                            kind: ChangeKind::Added,
                            path: current_path.clone(),
                            old_value: None,
                            new_value: Some(new_v.clone()),
                            description: format!("Added property '{}'", current_path.join(".")),
                        });
                        current_path.pop();
                    }
                }
            }
            (Value::Array(old_arr), Value::Array(new_arr)) => {
                // If array contents differ, record as modification of the array property
                acc.push(SemanticChange {
                    node_id: node_id.to_string(),
                    category,
                    kind: ChangeKind::Modified,
                    path: current_path.clone(),
                    old_value: Some(old_val.clone()),
                    new_value: Some(new_val.clone()),
                    description: format!(
                        "Modified array '{}' (changed from {} to {} items)",
                        current_path.join("."),
                        old_arr.len(),
                        new_arr.len()
                    ),
                });
            }
            _ => {
                // Scalar value changed
                acc.push(SemanticChange {
                    node_id: node_id.to_string(),
                    category,
                    kind: ChangeKind::Modified,
                    path: current_path.clone(),
                    old_value: Some(old_val.clone()),
                    new_value: Some(new_val.clone()),
                    description: format!(
                        "Changed '{}' from {:?} to {:?}",
                        current_path.join("."),
                        old_val,
                        new_val
                    ),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specs::parser::SpecParser;

    #[test]
    fn test_detect_semantic_field_addition() {
        let old_yaml = r#"
api:
  name: getUser
  method: GET
  path: /api/users/{id}
  responses:
    200:
      schema: UserResponse
"#;
        let new_yaml = r#"
api:
  name: getUser
  method: GET
  path: /api/users/{id}
  responses:
    200:
      schema: UserResponse
    404:
      description: Not Found
"#;
        let old_node = SpecParser::parse_yaml_str(old_yaml, "specs/api/users.yaml").unwrap();
        let new_node = SpecParser::parse_yaml_str(new_yaml, "specs/api/users.yaml").unwrap();

        let changes = SemanticChangeDetector::diff(Some(&old_node), Some(&new_node));
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ChangeKind::Added);
        assert_eq!(changes[0].path, vec!["api", "responses", "404"]);
    }
}
