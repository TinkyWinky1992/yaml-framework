use crate::error::IdeResult;
use crate::models::{ImplementationPlan, TaskContext};
use async_trait::async_trait;

#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn generate_plan(&self, context: &TaskContext) -> IdeResult<ImplementationPlan>;
    async fn repair_patch(&self, context: &TaskContext, errors: &[String]) -> IdeResult<ImplementationPlan>;
}
