use crate::providers::error::ProviderError;
use crate::providers::types::{
    CompletionResponse, GenerationParams, Message, StreamDelta, ToolDefinition,
};
use async_trait::async_trait;
use std::collections::HashSet;

/// Capabilities that a provider may support
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Can generate text embeddings
    Embeddings,
    /// Can generate chat completions
    Chat,
    /// Supports streaming responses
    Streaming,
    /// Supports tool/function calling
    ToolCalling,
    /// Supports structured JSON output with schema validation
    StructuredOutputs,
}

/// Information about a provider
#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub capabilities: HashSet<Capability>,
}

impl ProviderInfo {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            capabilities: HashSet::new(),
        }
    }

    pub fn with_capabilities(mut self, capabilities: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities = capabilities.into_iter().collect();
        self
    }

    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

/// Configuration for embedding requests
#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub model: String,
}

impl EmbeddingConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: "openai/text-embedding-3-small".to_string(),
        }
    }
}

/// Configuration for LLM requests
#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model: String,
    pub params: GenerationParams,
}

impl LlmConfig {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            params: GenerationParams::default(),
        }
    }

    pub fn with_params(mut self, params: GenerationParams) -> Self {
        self.params = params;
        self
    }
}

/// Provider that can generate text embeddings
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Get provider information
    fn info(&self) -> &ProviderInfo;

    /// Get the dimension of embeddings produced by this provider
    fn embedding_dimension(&self) -> usize;

    /// Generate embeddings for multiple texts (batch)
    async fn embed_batch(
        &self,
        texts: &[String],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<f32>>, ProviderError>;

    /// Generate embedding for a single text
    async fn embed(
        &self,
        text: &str,
        config: &EmbeddingConfig,
    ) -> Result<Vec<f32>, ProviderError> {
        let results = self.embed_batch(&[text.to_string()], config).await?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| ProviderError::ParseError("No embedding returned".to_string()))
    }
}

/// Provider that can generate text completions
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get provider information
    fn info(&self) -> &ProviderInfo;

    /// Generate a completion for the given messages
    async fn complete(
        &self,
        messages: &[Message],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Generate a completion with tool definitions
    async fn complete_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError>;
}

/// Callback type for streaming deltas
pub type StreamCallback = Box<dyn Fn(StreamDelta) + Send + Sync>;

/// Provider that supports streaming completions
#[async_trait]
pub trait StreamingLlmProvider: LlmProvider {
    /// Generate a streaming completion
    async fn complete_streaming(
        &self,
        messages: &[Message],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError>;

    /// Generate a streaming completion with tools
    async fn complete_streaming_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError>;
}
