use crate::ai::plan::{build_plan, build_prompt, parse_plan_text, provider_error, SYSTEM_PROMPT};
use crate::ai::provider::AiProvider;
use crate::error::IdeResult;
use crate::models::{ImplementationPlan, TaskContext};
use crate::workspace::WorkspaceManager;
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;

pub struct GeminiAiProvider {
    client: Client,
    api_key: String,
    model: String,
    workspace: Arc<WorkspaceManager>,
}

impl GeminiAiProvider {
    pub fn new(api_key: String, model: Option<String>, workspace: Arc<WorkspaceManager>) -> Self {
        Self {
            client: Client::builder().build().unwrap_or_default(),
            api_key,
            model: model.unwrap_or_else(|| "gemini-2.5-flash".into()),
            workspace,
        }
    }

    async fn complete(&self, prompt: &str) -> IdeResult<String> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let payload = json!({
            "contents": [{
                "parts": [{ "text": format!("{}\n\n{}", SYSTEM_PROMPT, prompt) }]
            }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
        });

        let response = self
            .client
            .post(&url)
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

/// Reads the first candidate's text part from a `generateContent` reply.
pub fn extract_text(body: &serde_json::Value, provider: &str) -> IdeResult<String> {
    body["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| provider_error(provider, "Missing text part in Gemini response"))
}

#[async_trait]
impl AiProvider for GeminiAiProvider {
    fn name(&self) -> &str {
        "GoogleGeminiProvider"
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
    fn extract_text_reads_first_candidate() {
        let body = json!({
            "candidates": [{ "content": { "parts": [{ "text": "{\"summary\": \"gemini\"}" }] } }]
        });
        let text = extract_text(&body, "Gemini").unwrap();
        assert_eq!(parse_plan_text(&text, "Gemini").unwrap().summary, "gemini");
    }

    #[test]
    fn extract_text_rejects_blocked_reply() {
        let body = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
        assert!(extract_text(&body, "Gemini").is_err());
    }
}
