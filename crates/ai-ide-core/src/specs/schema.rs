use crate::error::{IdeError, IdeResult};
use crate::models::SpecCategory;
use jsonschema::Validator;
use serde_json::json;
use std::sync::OnceLock;

static SYSTEM_SCHEMA: OnceLock<Validator> = OnceLock::new();
static API_SCHEMA: OnceLock<Validator> = OnceLock::new();
static UI_SCHEMA: OnceLock<Validator> = OnceLock::new();
static DATABASE_SCHEMA: OnceLock<Validator> = OnceLock::new();
static LOGIC_SCHEMA: OnceLock<Validator> = OnceLock::new();
static SECURITY_SCHEMA: OnceLock<Validator> = OnceLock::new();

pub struct SchemaValidator;

impl SchemaValidator {
    pub fn get_schema_for_category(category: SpecCategory) -> &'static Validator {
        match category {
            SpecCategory::System => SYSTEM_SCHEMA.get_or_init(|| {
                let schema_json = json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "required": ["project"],
                    "properties": {
                        "project": {
                            "type": "object",
                            "required": ["name", "version"],
                            "properties": {
                                "name": { "type": "string" },
                                "version": { "type": "string" },
                                "description": { "type": "string" },
                                "language": { "type": "string" },
                                "framework": { "type": "string" }
                            }
                        },
                        "conventions": {
                            "type": "object"
                        },
                        "validation": {
                            "type": "object",
                            "properties": {
                                "test_command": { "type": "string" },
                                "lint_command": { "type": "string" },
                                "typecheck_command": { "type": "string" }
                            }
                        }
                    }
                });
                Validator::new(&schema_json).expect("Invalid SYSTEM schema")
            }),
            SpecCategory::Api => API_SCHEMA.get_or_init(|| {
                let schema_json = json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "required": ["api"],
                    "properties": {
                        "api": {
                            "type": "object",
                            "required": ["name", "method", "path", "responses"],
                            "properties": {
                                "name": { "type": "string" },
                                "method": { "type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"] },
                                "path": { "type": "string" },
                                "description": { "type": "string" },
                                "security": {
                                    "type": "object",
                                    "properties": {
                                        "authentication": { "type": "string" },
                                        "permission": { "type": "string" }
                                    }
                                },
                                "parameters": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "required": ["name", "in", "type"],
                                        "properties": {
                                            "name": { "type": "string" },
                                            "in": { "type": "string", "enum": ["path", "query", "header"] },
                                            "type": { "type": "string" },
                                            "required": { "type": "boolean" }
                                        }
                                    }
                                },
                                "requestBody": {
                                    "type": "object",
                                    "properties": {
                                        "schema": { "type": ["string", "object"] }
                                    }
                                },
                                "responses": {
                                    "type": "object"
                                }
                            }
                        }
                    }
                });
                Validator::new(&schema_json).expect("Invalid API schema")
            }),
            SpecCategory::Database => DATABASE_SCHEMA.get_or_init(|| {
                let schema_json = json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "required": ["entity"],
                    "properties": {
                        "entity": {
                            "type": "object",
                            "required": ["name", "fields"],
                            "properties": {
                                "name": { "type": "string" },
                                "table": { "type": "string" },
                                "fields": {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "required": ["name", "type"],
                                        "properties": {
                                            "name": { "type": "string" },
                                            "type": { "type": "string" },
                                            "primary_key": { "type": "boolean" },
                                            "nullable": { "type": "boolean" },
                                            "unique": { "type": "boolean" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
                Validator::new(&schema_json).expect("Invalid DATABASE schema")
            }),
            SpecCategory::Ui => UI_SCHEMA.get_or_init(|| {
                let schema_json = json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "required": ["component"],
                    "properties": {
                        "component": {
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": { "type": "string" },
                                "type": { "type": "string" },
                                "api_dependencies": {
                                    "type": "array",
                                    "items": { "type": "string" }
                                }
                            }
                        }
                    }
                });
                Validator::new(&schema_json).expect("Invalid UI schema")
            }),
            SpecCategory::BusinessLogic => LOGIC_SCHEMA.get_or_init(|| {
                let schema_json = json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "required": ["workflow"],
                    "properties": {
                        "workflow": {
                            "type": "object",
                            "required": ["name", "steps"],
                            "properties": {
                                "name": { "type": "string" },
                                "steps": { "type": "array" }
                            }
                        }
                    }
                });
                Validator::new(&schema_json).expect("Invalid LOGIC schema")
            }),
            SpecCategory::Security => SECURITY_SCHEMA.get_or_init(|| {
                let schema_json = json!({
                    "$schema": "http://json-schema.org/draft-07/schema#",
                    "type": "object",
                    "required": ["permissions"],
                    "properties": {
                        "permissions": { "type": "array" }
                    }
                });
                Validator::new(&schema_json).expect("Invalid SECURITY schema")
            }),
        }
    }

    pub fn validate(category: SpecCategory, value: &serde_json::Value, file_name: &str) -> IdeResult<()> {
        let validator = Self::get_schema_for_category(category);
        let error_msgs: Vec<String> = validator
            .iter_errors(value)
            .map(|e| format!("{}: {}", e.instance_path, e))
            .collect();


        if !error_msgs.is_empty() {
            return Err(IdeError::SchemaValidation {
                file: file_name.to_string(),
                errors: error_msgs,
            });
        }
        Ok(())
    }

}
