use thiserror::Error;

#[derive(Error, Debug)]
pub enum IdeError {
    #[error("Workspace error: {0}")]
    Workspace(String),

    #[error("Path traversal or security violation: {0}")]
    Security(String),

    #[error("Specification parsing error in '{file}:{line}:{column}': {message}")]
    SpecParse {
        file: String,
        line: usize,
        column: usize,
        message: String,
    },

    #[error("Schema validation failed for '{file}': {errors:?}")]
    SchemaValidation {
        file: String,
        errors: Vec<String>,
    },

    #[error("Specification reference not found: {0}")]
    MissingReference(String),

    #[error("Semantic diff calculation error: {0}")]
    SemanticDiff(String),

    #[error("Dependency graph error: {0}")]
    DependencyGraph(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("AI provider error ({provider}): {message}")]
    AiProvider {
        provider: String,
        message: String,
    },

    #[error("Patch conflict error for '{file}': {message}")]
    PatchConflict {
        file: String,
        message: String,
    },

    #[error("Validation command failed with exit code {code}: {output}")]
    ValidationFailed {
        code: i32,
        output: String,
    },

    #[error("Git error: {0}")]
    Git(String),

    #[error("Database persistence error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type IdeResult<T> = Result<T, IdeError>;
