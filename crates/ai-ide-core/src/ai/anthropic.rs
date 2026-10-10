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

pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-5-5";

const MAX_OUTPUT_TOKENS: u32 = 8192;

/// Claude through the Anthropic Messages API.
///
/// The key is sent only in the `x-api-key` header and is never logged or put in an error message.
pub struct AnthropicAiProvider {
    client: Client,
    api_key: String,
    model: String,
    workspace: Arc<WorkspaceManager>,
}

impl AnthropicAiProvider {
    pub fn new(api_key: String, model: Option<String>, workspace: Arc<WorkspaceManager>) -> Self {
        Self {
            client: Client::builder().timeout(Duration::from_secs(180)).build().unwrap_or_default(),
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_ANTHROPIC_MODEL.into()),
            workspace,
        }
    }

    /// Reads `ANTHROPIC_API_KEY` and the optional `ANTHROPIC_MODEL`. Returns `None` without a key.
    pub fn from_env(workspace: Arc<WorkspaceManager>) -> Option<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.trim().is_empty())?;
        let model = std::env::var("ANTHROPIC_MODEL").ok().filter(|m| !m.trim().is_empty());
        Some(Self::new(api_key.trim().to_string(), model, workspace))
    }

    async fn complete(&self, prompt: &str) -> IdeResult<String> {
        let payload = json!({
            "model": self.model,
            "max_tokens": MAX_OUTPUT_TOKENS,
            "system": SYSTEM_PROMPT,
            "messages": [{ "role": "user", "content": prompt }],
        });

        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
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
        extract_text(&body, self.name())
    }
}

/// Joins the `text` blocks of a Messages API response.
pub fn extract_text(body: &serde_json::Value, provider: &str) -> IdeResult<String> {
    let text: String = body["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Err(provider_error(provider, "response contained no text block"));
    }
    Ok(text)
}

#[async_trait]
impl AiProvider for AnthropicAiProvider {
    fn name(&self) -> &str {
        "Anthropic"
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
    fn extract_text_joins_text_blocks_only() {
        let body = json!({
            "id": "msg_1",
            "type": "message",
            "content": [
                { "type": "text", "text": "{\"summary\": " },
                { "type": "tool_use", "id": "x", "name": "n", "input": {} },
                { "type": "text", "text": "\"ok\"}" }
            ],
            "stop_reason": "end_turn"
        });
        assert_eq!(extract_text(&body, "Anthropic").unwrap(), "{\"summary\": \"ok\"}");
    }

    #[test]
    fn extract_text_rejects_empty_content() {
        let body = json!({ "content": [] });
        assert!(extract_text(&body, "Anthropic").is_err());
    }

    #[test]
    fn canned_reply_parses_into_wire_plan() {
        let body = json!({
            "content": [{ "type": "text", "text": "{\"summary\": \"Add phone\", \"rationale\": \"spec\", \"patches\": [{\"path\": \"src/routes/users.ts\", \"patched_content\": \"export {};\\n\"}]}" }]
        });
        let text = extract_text(&body, "Anthropic").unwrap();
        let wire = parse_plan_text(&text, "Anthropic").unwrap();
        assert_eq!(wire.summary, "Add phone");
        assert_eq!(wire.patches[0].patched_content, "export {};\n");
    }
}
