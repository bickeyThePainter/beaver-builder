use std::sync::Arc;

use super::openai::OpenAiProvider;
use super::provider::LlmProvider;

/// Factory for constructing LLM providers from environment configuration.
pub struct LlmProviderFactory;

impl LlmProviderFactory {
    /// Build a provider based on the LLM_PROVIDER env var (default: "openai").
    ///
    /// Falls back to OpenAI if an unknown provider is specified, logging a warning.
    pub fn from_env() -> Arc<dyn LlmProvider> {
        let provider = std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".into());
        match provider.as_str() {
            "openai" => Arc::new(OpenAiProvider::from_env()),
            other => {
                tracing::warn!(
                    provider = other,
                    "unknown LLM provider, falling back to openai"
                );
                Arc::new(OpenAiProvider::from_env())
            }
        }
    }
}
