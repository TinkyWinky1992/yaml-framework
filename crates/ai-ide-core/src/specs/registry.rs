use crate::error::IdeResult;
use crate::models::{SpecCategory, SpecNode};

use crate::specs::parser::SpecParser;
use crate::specs::schema::SchemaValidator;
use crate::workspace::WorkspaceManager;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct SpecRegistry {
    nodes: HashMap<String, SpecNode>, // indexed by node.id
    file_to_id: HashMap<String, String>,
}

impl SpecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&SpecNode> {
        self.nodes.get(id)
    }

    pub fn get_by_file(&self, file_path: &str) -> Option<&SpecNode> {
        let norm = file_path.replace('\\', "/");
        self.file_to_id.get(&norm).and_then(|id| self.nodes.get(id))
    }

    pub fn all_nodes(&self) -> Vec<&SpecNode> {
        self.nodes.values().collect()
    }

    pub fn load_from_workspace(&mut self, wm: &WorkspaceManager) -> IdeResult<Vec<String>> {
        self.nodes.clear();
        self.file_to_id.clear();

        let files = wm.list_spec_files()?;
        let mut warnings = Vec::new();

        for file_path in files {
            let relative = file_path
                .strip_prefix(wm.root())
                .unwrap_or(&file_path)
                .to_string_lossy()
                .replace('\\', "/");

            let content = wm.read_file(&relative)?;
            match SpecParser::parse_yaml_str(&content, &relative) {
                Ok(node) => {
                    // Validate schema
                    if let Err(e) = SchemaValidator::validate(node.category, &node.content, &relative) {
                        warnings.push(format!("Schema warning in '{}': {}", relative, e));
                    }
                    self.file_to_id.insert(relative.clone(), node.id.clone());
                    self.nodes.insert(node.id.clone(), node);
                }
                Err(e) => {
                    warnings.push(format!("Parse warning in '{}': {}", relative, e));
                }
            }
        }

        // Verify cross-references
        self.verify_cross_references(&mut warnings);

        Ok(warnings)
    }

    pub fn register_or_update(&mut self, node: SpecNode) -> IdeResult<()> {
        SchemaValidator::validate(node.category, &node.content, &node.file_path)?;
        let norm_file = node.file_path.replace('\\', "/");
        self.file_to_id.insert(norm_file, node.id.clone());
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    fn verify_cross_references(&self, warnings: &mut Vec<String>) {
        for node in self.nodes.values() {
            if node.category == SpecCategory::Ui {
                if let Some(deps) = node.content.get("component")
                    .and_then(|c| c.get("api_dependencies"))
                    .and_then(|d| d.as_array())
                {
                    for dep in deps {
                        if let Some(api_name) = dep.as_str() {
                            let expected_id = format!("api:{}", api_name);
                            if !self.nodes.contains_key(&expected_id) {
                                warnings.push(format!(
                                    "UI component '{}' references missing API '{}'",
                                    node.name, api_name
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}
