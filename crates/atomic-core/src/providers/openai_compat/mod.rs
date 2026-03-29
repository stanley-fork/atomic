//! OpenAI-compatible provider implementation
//! Works with any server that implements the OpenAI API (vLLM, LiteLLM, llama.cpp, LocalAI, etc.)

mod embedding;
mod llm;

use crate::providers::error::ProviderError;
use crate::providers::traits::{
    EmbeddingConfig, EmbeddingProvider, LlmConfig, LlmProvider, StreamCallback,
    StreamingLlmProvider,
};
use crate::providers::types::{CompletionResponse, Message, ToolDefinition};
use async_trait::async_trait;
use reqwest::Client;
use std::time::Duration;

/// Generic OpenAI-compatible provider
/// Supports any server implementing the OpenAI API format
pub struct OpenAICompatProvider {
    client: Client,
    api_key: Option<String>,
    base_url: String,
}

impl OpenAICompatProvider {
    pub fn new(base_url: String, api_key: Option<String>, timeout_secs: Option<u64>) -> Self {
        let timeout = Duration::from_secs(timeout_secs.unwrap_or(300));
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|_| Client::new());

        // Normalize the base URL to always end with /v1 so callers can provide
        // either "http://host:port" or "http://host:port/v1" and both work.
        let trimmed = base_url.trim_end_matches('/').to_string();
        let base_url = if trimmed.ends_with("/v1") {
            trimmed
        } else {
            format!("{}/v1", trimmed)
        };

        Self {
            client,
            api_key,
            base_url,
        }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAICompatProvider {
    async fn embed_batch(
        &self,
        texts: &[String],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<f32>>, ProviderError> {
        embedding::embed_batch(self, texts, config).await
    }
}

#[async_trait]
impl LlmProvider for OpenAICompatProvider {
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
impl StreamingLlmProvider for OpenAICompatProvider {
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
