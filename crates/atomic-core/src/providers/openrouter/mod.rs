//! OpenRouter provider implementation

mod embedding;
mod llm;
pub mod models;

use crate::providers::error::ProviderError;
use crate::providers::traits::{
    EmbeddingConfig, EmbeddingProvider, LlmConfig, LlmProvider, StreamCallback,
    StreamingLlmProvider,
};
use crate::providers::types::{CompletionResponse, Message, ToolDefinition};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

/// OpenRouter provider implementation
/// Supports embeddings, chat completions, streaming, tool calling, and structured outputs
pub struct OpenRouterProvider {
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenRouterProvider {
    pub fn new(api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client,
            api_key,
            base_url: "https://openrouter.ai/api/v1".to_string(),
        }
    }

    /// Get the HTTP client
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the API key
    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait]
impl EmbeddingProvider for OpenRouterProvider {
    async fn embed_batch(
        &self,
        texts: &[String],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<f32>>, ProviderError> {
        embedding::embed_batch(self, texts, config).await
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    async fn complete(
        &self,
        messages: &[Message],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError> {
        llm::complete(self, messages, config).await
    }

    async fn complete_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError> {
        llm::complete_with_tools(self, messages, tools, config).await
    }
}

#[async_trait]
impl StreamingLlmProvider for OpenRouterProvider {
    async fn complete_streaming_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError> {
        llm::complete_streaming_with_tools(self, messages, tools, config, on_delta).await
    }
}
