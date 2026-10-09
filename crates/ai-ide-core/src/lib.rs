pub mod ai;
pub mod changes;
pub mod context;
pub mod drift;
pub mod error;
pub mod git;
pub mod graph;
pub mod models;
pub mod patch;
pub mod specs;
pub mod storage;
pub mod validation;
pub mod workspace;

pub use error::{IdeError, IdeResult};

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_version() {
        assert_eq!(version(), "0.1.0");
    }
}
