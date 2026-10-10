use crate::ai::anthropic::AnthropicAiProvider;
use crate::ai::gemini::GeminiAiProvider;
use crate::ai::mock::MockAiProvider;
use crate::ai::ollama::OllamaAiProvider;
use crate::ai::provider::AiProvider;
use crate::error::{IdeError, IdeResult};
use crate::workspace::WorkspaceManager;
use serde::Serialize;
use std::sync::Arc;

pub const PROVIDER_IDS: [&str; 4] = ["anthropic", "gemini", "ollama", "mock"];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProviderInfo {
    pub id: String,
    pub label: String,
    /// True when the key or endpoint is configured, so the provider can be selected.
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProvidersResponse {
    pub active: String,
    pub providers: Vec<ProviderInfo>,
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

fn label_for(id: &str) -> &'static str {
    match id {
        "anthropic" => "Claude (Anthropic)",
        "gemini" => "Gemini",
        "ollama" => "Ollama (local)",
        _ => "Mock (offline)",
    }
}

pub fn is_known(id: &str) -> bool {
    PROVIDER_IDS.contains(&id)
}

/// Provider used when the user has not chosen one: the first cloud provider with a key.
pub fn default_provider_id() -> &'static str {
    if env_value("ANTHROPIC_API_KEY").is_some() {
        "anthropic"
    } else if env_value("GEMINI_API_KEY").is_some() {
        "gemini"
    } else {
        "mock"
    }
}

async fn is_available(id: &str, workspace: &Arc<WorkspaceManager>) -> bool {
    match id {
        "anthropic" => env_value("ANTHROPIC_API_KEY").is_some(),
        "gemini" => env_value("GEMINI_API_KEY").is_some(),
        "ollama" => OllamaAiProvider::from_env(workspace.clone()).is_available().await,
        _ => true,
    }
}

pub async fn list_providers(active: &str, workspace: &Arc<WorkspaceManager>) -> ProvidersResponse {
    let mut providers = Vec::with_capacity(PROVIDER_IDS.len());
    for id in PROVIDER_IDS {
        providers.push(ProviderInfo {
            id: id.into(),
            label: label_for(id).into(),
            available: is_available(id, workspace).await,
        });
    }
    ProvidersResponse {
        active: active.into(),
        providers,
    }
}

/// Builds the provider for `id`, or fails if it is unknown or not configured.
pub async fn resolve_provider(id: &str, workspace: &Arc<WorkspaceManager>) -> IdeResult<Box<dyn AiProvider>> {
    if !is_known(id) {
        return Err(IdeError::AiProvider {
            provider: id.into(),
            message: "unknown provider".into(),
        });
    }
    if !is_available(id, workspace).await {
        return Err(IdeError::AiProvider {
            provider: id.into(),
            message: "not configured (missing key or endpoint)".into(),
        });
    }

    let provider: Box<dyn AiProvider> = match id {
        "anthropic" => Box::new(
            AnthropicAiProvider::from_env(workspace.clone()).ok_or_else(|| IdeError::AiProvider {
                provider: id.into(),
                message: "ANTHROPIC_API_KEY is not set".into(),
            })?,
        ),
        "gemini" => Box::new(GeminiAiProvider::new(
            env_value("GEMINI_API_KEY").unwrap_or_default(),
            env_value("GEMINI_MODEL"),
            workspace.clone(),
        )),
        "ollama" => Box::new(OllamaAiProvider::from_env(workspace.clone())),
        _ => Box::new(MockAiProvider),
    };
    Ok(provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_ids_match_the_frontend_list() {
        assert!(is_known("anthropic"));
        assert!(is_known("mock"));
        assert!(!is_known("openai"));
    }
}
