// Provider abstraction layer for AI services (embeddings, LLM completion)
// Enables pluggable providers (OpenRouter, Ollama, etc.)

pub mod error;
pub mod models;
pub mod openrouter;
pub mod registry;
pub mod traits;
pub mod types;

pub use error::ProviderError;
pub use models::{fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache, AvailableModel, ModelCapabilitiesCache};
pub use registry::ProviderRegistry;
pub use traits::{Capability, EmbeddingConfig, EmbeddingProvider, LlmConfig, LlmProvider, ProviderInfo, StreamingLlmProvider};
pub use types::{
    CompletionResponse, GenerationParams, Message, MessageRole, StreamDelta,
    StructuredOutputSchema, ToolCall, ToolDefinition,
};
