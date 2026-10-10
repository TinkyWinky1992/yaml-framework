use crate::ai::plan::{build_plan, build_prompt, parse_plan_text, provider_error, SYSTEM_PROMPT};
use crate::ai::provider::AiProvider;
use crate::error::IdeResult;
use crate::models::{ImplementationPlan, TaskContext};
use crate::workspace::WorkspaceManager;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_OLLAMA_HOST: &str = "http://127.0.0.1:11434";
pub const DEFAULT_OLLAMA_MODEL: &str = "llama3.1";

/// A local model served by Ollama, using its `/api/chat` endpoint in JSON mode.
pub struct OllamaAiProvider {
    client: Client,
    host: String,
    model: String,
    workspace: Arc<WorkspaceManager>,
}

impl OllamaAiProvider {
    pub fn new(host: String, model: String, workspace: Arc<WorkspaceManager>) -> Self {
        Self {
            client: Client::builder().timeout(Duration::from_secs(300)).build().unwrap_or_default(),
            host: host.trim_end_matches('/').to_string(),
            model,
            workspace,
        }
    }

    /// Reads `OLLAMA_HOST` (default `http://127.0.0.1:11434`) and `OLLAMA_MODEL`.
    pub fn from_env(workspace: Arc<WorkspaceManager>) -> Self {
        let host = std::env::var("OLLAMA_HOST")
            .ok()
            .filter(|h| !h.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_HOST.into());
        let model = std::env::var("OLLAMA_MODEL")
            .ok()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.into());
        Self::new(host, model, workspace)
    }

    /// Available only when the server answers `GET /api/tags`.
    pub async fn is_available(&self) -> bool {
        self.client
            .get(format!("{}/api/tags", self.host))
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    async fn complete(&self, prompt: &str) -> IdeResult<String> {
        let payload = json!({
            "model": self.model,
            "stream": false,
            "format": "json",
            "messages": [
                { "role": "system", "content": SYSTEM_PROMPT },
                { "role": "user", "content": prompt },
            ],
        });

        let response = self
            .client
            .post(format!("{}/api/chat", self.host))
            .json(&payload)
            .send()
            .await
            .map_err(|e| provider_error(self.name(), format!("HTTP request error: {}", e)))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| provider_error(self.name(), format!("Failed to read response: {}", e)))?;
        if !status.is_success() {
            return Err(provider_error(self.name(), format!("API returned {}: {}", status, text)));
        }

        let body: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| provider_error(self.name(), format!("Failed to parse response JSON: {}", e)))?;
        extract_content(&body, self.name())
    }
}

/// Reads `message.content` from an `/api/chat` reply.
pub fn extract_content(body: &serde_json::Value, provider: &str) -> IdeResult<String> {
    body["message"]["content"]
        .as_str()
        .filter(|c| !c.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| provider_error(provider, "response has no message content"))
}

#[async_trait]
impl AiProvider for OllamaAiProvider {
    fn name(&self) -> &str {
        "Ollama"
    }

    async fn generate_plan(&self, context: &TaskContext) -> IdeResult<ImplementationPlan> {
        let text = self.complete(&build_prompt(context)).await?;
        let wire = parse_plan_text(&text, self.name())?;
        build_plan(wire, &self.workspace, self.name())
    }

    async fn repair_patch(&self, context: &TaskContext, errors: &[String]) -> IdeResult<ImplementationPlan> {
        let mut repair_context = context.clone();
        repair_context.failing_checks = errors.to_vec();
        self.generate_plan(&repair_context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_content_reads_chat_message() {
        let body = json!({
            "model": "llama3.1",
            "message": { "role": "assistant", "content": "{\"summary\": \"ok\", \"patches\": []}" },
            "done": true
        });
        let text = extract_content(&body, "Ollama").unwrap();
        let wire = parse_plan_text(&text, "Ollama").unwrap();
        assert_eq!(wire.summary, "ok");
        assert!(wire.patches.is_empty());
    }

    #[test]
    fn extract_content_rejects_missing_message() {
        assert!(extract_content(&json!({ "done": true }), "Ollama").is_err());
        assert!(extract_content(&json!({ "message": { "content": "  " } }), "Ollama").is_err());
    }
}
