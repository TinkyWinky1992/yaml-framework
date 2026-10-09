use crate::ai::provider::AiProvider;
use crate::error::{IdeError, IdeResult};
use crate::models::{ImplementationPlan, TaskContext};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct GeminiAiProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl GeminiAiProvider {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            client: Client::builder().build().unwrap_or_default(),
            api_key,
            model: model.unwrap_or_else(|| "gemini-2.5-flash".into()),
        }
    }
}

#[async_trait]
impl AiProvider for GeminiAiProvider {
    fn name(&self) -> &str {
        "GoogleGeminiProvider"
    }

    async fn generate_plan(&self, context: &TaskContext) -> IdeResult<ImplementationPlan> {
        let prompt = format!(
            "Task: {}\nSpec changes: {:?}\nImpacted files: {:?}\nOutput JSON matching ImplementationPlan schema.",
            context.task_description,
            context.spec_changes,
            context.impacted_files.keys().collect::<Vec<_>>()
        );

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let payload = json!({
            "contents": [{
                "parts": [{ "text": prompt }]
            }],
            "generationConfig": {
                "responseMimeType": "application/json"
            }
        });

        let response = self.client.post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| IdeError::AiProvider {
                provider: self.name().into(),
                message: format!("HTTP request error: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(IdeError::AiProvider {
                provider: self.name().into(),
                message: format!("API returned {}: {}", status, text),
            });
        }

        let body: serde_json::Value = response.json().await.map_err(|e| IdeError::AiProvider {
            provider: self.name().into(),
            message: format!("Failed to parse response JSON: {}", e),
        })?;

        let text_content = body["candidates"][0]["content"]["parts"][0]["text"].as_str()
            .ok_or_else(|| IdeError::AiProvider {
                provider: self.name().into(),
                message: "Missing text part in Gemini response".into(),
            })?;

        let plan: ImplementationPlan = serde_json::from_str(text_content).map_err(|e| IdeError::AiProvider {
            provider: self.name().into(),
            message: format!("Failed to parse structured plan: {}", e),
        })?;

        Ok(plan)
    }

    async fn repair_patch(&self, context: &TaskContext, errors: &[String]) -> IdeResult<ImplementationPlan> {
        let mut repair_context = context.clone();
        repair_context.failing_checks = errors.to_vec();
        self.generate_plan(&repair_context).await
    }
}
