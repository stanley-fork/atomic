use crate::providers::error::ProviderError;
use crate::providers::openrouter::OpenRouterProvider;
use crate::providers::traits::{EmbeddingProvider, LlmProvider, StreamingLlmProvider};
use std::collections::HashMap;
use std::sync::Arc;

/// Registry holding the active provider instances
pub struct ProviderRegistry {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    llm_provider: Arc<dyn LlmProvider>,
    streaming_provider: Arc<dyn StreamingLlmProvider>,
    provider_type: String,
}

impl ProviderRegistry {
    /// Create a new provider registry from settings
    pub fn from_settings(settings: &HashMap<String, String>) -> Result<Self, ProviderError> {
        let provider_type = settings
            .get("provider")
            .cloned()
            .unwrap_or_else(|| "openrouter".to_string());

        match provider_type.as_str() {
            "openrouter" => {
                let api_key = settings
                    .get("openrouter_api_key")
                    .cloned()
                    .ok_or_else(|| {
                        ProviderError::Configuration(
                            "OpenRouter API key not configured".to_string(),
                        )
                    })?;

                if api_key.is_empty() {
                    return Err(ProviderError::Configuration(
                        "OpenRouter API key is empty".to_string(),
                    ));
                }

                let provider = Arc::new(OpenRouterProvider::new(api_key));

                Ok(Self {
                    embedding_provider: provider.clone(),
                    llm_provider: provider.clone(),
                    streaming_provider: provider,
                    provider_type,
                })
            }
            // Future: "ollama" => { ... }
            _ => Err(ProviderError::Configuration(format!(
                "Unknown provider type: {}",
                provider_type
            ))),
        }
    }

    /// Get the embedding provider
    pub fn embedding(&self) -> &dyn EmbeddingProvider {
        &*self.embedding_provider
    }

    /// Get the LLM provider
    pub fn llm(&self) -> &dyn LlmProvider {
        &*self.llm_provider
    }

    /// Get the streaming LLM provider
    pub fn streaming(&self) -> &dyn StreamingLlmProvider {
        &*self.streaming_provider
    }

    /// Get the current provider type name
    pub fn provider_type(&self) -> &str {
        &self.provider_type
    }

    /// Check if the provider supports a capability
    pub fn supports_streaming(&self) -> bool {
        use crate::providers::traits::Capability;
        self.llm_provider.info().supports(Capability::Streaming)
    }

    /// Check if the provider supports tool calling
    pub fn supports_tool_calling(&self) -> bool {
        use crate::providers::traits::Capability;
        self.llm_provider.info().supports(Capability::ToolCalling)
    }

    /// Check if the provider supports structured outputs
    pub fn supports_structured_outputs(&self) -> bool {
        use crate::providers::traits::Capability;
        self.llm_provider
            .info()
            .supports(Capability::StructuredOutputs)
    }
}
