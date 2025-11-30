# Pluggable Provider Pattern for Atomic

## Overview

This plan introduces a modular, trait-based provider abstraction for the Atomic note-taking app. Currently, OpenRouter is tightly coupled throughout the codebase. This refactoring will:

1. Create a clean provider interface supporting embeddings and LLM capabilities
2. Use a **unified provider** model (single provider for all AI tasks)
3. Abstract OpenRouter first, then add Ollama support in a follow-up phase
4. Remove unused sqlite-lembed infrastructure (~70MB savings)

---

## Phase 1: Foundation & Cleanup

### 1.1 Remove sqlite-lembed Infrastructure

**Files to modify:**
- `src-tauri/src/db.rs`: Remove `load_lembed_extension()`, `register_embedding_model()`, `get_lembed_extension_filename()`, and simplify `Database::new()` and `new_connection()`
- `src-tauri/tauri.conf.json`: Remove resource entries for lembed dylibs and GGUF model

**Files to delete:**
- `src-tauri/resources/lembed0.so`
- `src-tauri/resources/lembed0-aarch64.dylib`
- `src-tauri/resources/lembed0-x86_64.dylib`
- `src-tauri/resources/all-MiniLM-L6-v2.q8_0.gguf`

### 1.2 Create Provider Module Structure

```
src-tauri/src/providers/
├── mod.rs              # Module exports and re-exports
├── types.rs            # Shared types (Message, ToolCall, CompletionRequest, etc.)
├── error.rs            # ProviderError enum with retry support
├── traits.rs           # EmbeddingProvider, LlmProvider, StreamingLlmProvider traits
├── registry.rs         # Provider factory and active provider management
└── openrouter/
    ├── mod.rs          # OpenRouter provider entry point
    ├── embedding.rs    # OpenRouterEmbeddingProvider implementation
    └── llm.rs          # OpenRouterLlmProvider implementation
```

### 1.3 Add Dependencies

Update `src-tauri/Cargo.toml`:
```toml
async-trait = "0.1"
thiserror = "1.0"
```

---

## Phase 2: Core Trait Definitions

### 2.1 Provider Types (`providers/types.rs`)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct StructuredOutputSchema {
    pub name: String,
    pub schema: serde_json::Value,
    pub strict: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GenerationParams {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub structured_output: Option<StructuredOutputSchema>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub enum StreamDelta {
    Content(String),
    ToolCallStart { id: String, name: String },
    ToolCallArguments { id: String, arguments: String },
    Done { finish_reason: Option<String> },
}
```

### 2.2 Error Types (`providers/error.rs`)

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("Network error: {0}")]
    Network(String),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("Rate limited, retry after {retry_after_secs:?} seconds")]
    RateLimited { retry_after_secs: Option<u64> },

    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),

    #[error("Parse error: {0}")]
    ParseError(String),
}

impl ProviderError {
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            ProviderError::RateLimited { .. } |
            ProviderError::Network(_)
        )
    }
}
```

### 2.3 Provider Traits (`providers/traits.rs`)

```rust
use async_trait::async_trait;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    Embeddings,
    Chat,
    Streaming,
    ToolCalling,
    StructuredOutputs,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub name: String,
    pub capabilities: HashSet<Capability>,
}

#[derive(Debug, Clone)]
pub struct EmbeddingConfig {
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub model: String,
    pub params: GenerationParams,
}

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn info(&self) -> &ProviderInfo;
    fn embedding_dimension(&self) -> usize;

    async fn embed_batch(
        &self,
        texts: &[String],
        config: &EmbeddingConfig,
    ) -> Result<Vec<Vec<f32>>, ProviderError>;
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn info(&self) -> &ProviderInfo;

    async fn complete(
        &self,
        messages: &[Message],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError>;

    async fn complete_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<CompletionResponse, ProviderError>;
}

pub type StreamCallback = Box<dyn Fn(StreamDelta) + Send + Sync>;

#[async_trait]
pub trait StreamingLlmProvider: LlmProvider {
    async fn complete_streaming(
        &self,
        messages: &[Message],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError>;

    async fn complete_streaming_with_tools(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        config: &LlmConfig,
        on_delta: StreamCallback,
    ) -> Result<CompletionResponse, ProviderError>;
}
```

---

## Phase 3: OpenRouter Provider Implementation

### 3.1 OpenRouter Embedding Provider

Extract logic from `embedding.rs:generate_openrouter_embeddings_public()` into `providers/openrouter/embedding.rs`:

```rust
pub struct OpenRouterEmbeddingProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl OpenRouterEmbeddingProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            base_url: "https://openrouter.ai/api/v1".to_string(),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenRouterEmbeddingProvider {
    fn info(&self) -> &ProviderInfo { /* capabilities: Embeddings */ }
    fn embedding_dimension(&self) -> usize { 1536 } // text-embedding-3-small

    async fn embed_batch(&self, texts: &[String], config: &EmbeddingConfig)
        -> Result<Vec<Vec<f32>>, ProviderError> {
        // Move existing OpenRouter embedding logic here
    }
}
```

### 3.2 OpenRouter LLM Provider

Extract and consolidate logic from `extraction.rs`, `wiki.rs`, and `agent.rs` into `providers/openrouter/llm.rs`:

```rust
pub struct OpenRouterLlmProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

#[async_trait]
impl LlmProvider for OpenRouterLlmProvider {
    fn info(&self) -> &ProviderInfo {
        // capabilities: Chat, Streaming, ToolCalling, StructuredOutputs
    }

    async fn complete(&self, messages: &[Message], config: &LlmConfig)
        -> Result<CompletionResponse, ProviderError> {
        // Consolidated OpenRouter chat completion logic
    }

    async fn complete_with_tools(&self, messages: &[Message], tools: &[ToolDefinition], config: &LlmConfig)
        -> Result<CompletionResponse, ProviderError> {
        // OpenRouter tool calling logic
    }
}

#[async_trait]
impl StreamingLlmProvider for OpenRouterLlmProvider {
    async fn complete_streaming(&self, messages: &[Message], config: &LlmConfig, on_delta: StreamCallback)
        -> Result<CompletionResponse, ProviderError> {
        // Move SSE parsing logic from agent.rs here
    }

    async fn complete_streaming_with_tools(&self, ...) { /* ... */ }
}
```

### 3.3 Provider Registry (`providers/registry.rs`)

```rust
use std::sync::Arc;

pub struct ProviderRegistry {
    embedding_provider: Arc<dyn EmbeddingProvider>,
    llm_provider: Arc<dyn LlmProvider>,
    streaming_provider: Arc<dyn StreamingLlmProvider>,
}

impl ProviderRegistry {
    pub fn from_settings(settings: &HashMap<String, String>) -> Result<Self, ProviderError> {
        let provider_type = settings.get("provider").unwrap_or(&"openrouter".to_string());

        match provider_type.as_str() {
            "openrouter" => {
                let api_key = settings.get("openrouter_api_key")
                    .ok_or(ProviderError::Configuration("Missing API key".into()))?;
                let provider = Arc::new(OpenRouterLlmProvider::new(api_key.clone()));
                let embedding = Arc::new(OpenRouterEmbeddingProvider::new(api_key.clone()));
                Ok(Self {
                    embedding_provider: embedding,
                    llm_provider: provider.clone(),
                    streaming_provider: provider,
                })
            }
            // Future: "ollama" => { ... }
            _ => Err(ProviderError::Configuration(format!("Unknown provider: {}", provider_type))),
        }
    }

    pub fn embedding(&self) -> &dyn EmbeddingProvider { &*self.embedding_provider }
    pub fn llm(&self) -> &dyn LlmProvider { &*self.llm_provider }
    pub fn streaming(&self) -> &dyn StreamingLlmProvider { &*self.streaming_provider }
}
```

---

## Phase 4: Integration - Update Existing Code

### 4.1 Update `lib.rs`

Add `ProviderRegistry` to Tauri app state:

```rust
mod providers;

use providers::ProviderRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AppState {
    pub db: Database,
    pub providers: RwLock<Option<Arc<ProviderRegistry>>>,
}

// In setup:
fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    // ... existing db setup ...

    // Initialize providers from settings
    let settings = settings::get_all_settings(&conn)?;
    let registry = ProviderRegistry::from_settings(&settings).ok();

    app.manage(AppState {
        db,
        providers: RwLock::new(registry.map(Arc::new)),
    });
}
```

### 4.2 Update `embedding.rs`

Replace direct OpenRouter calls with provider abstraction:

```rust
pub async fn generate_embeddings(
    providers: &ProviderRegistry,
    texts: &[String],
    model: &str,
) -> Result<Vec<Vec<f32>>, String> {
    let config = EmbeddingConfig { model: model.to_string() };
    providers.embedding()
        .embed_batch(texts, &config)
        .await
        .map_err(|e| e.to_string())
}
```

### 4.3 Update `extraction.rs`

Replace `call_openrouter_api()` with provider calls:

```rust
pub async fn extract_tags_from_chunk(
    providers: &ProviderRegistry,
    chunk_content: &str,
    tag_tree_json: &str,
    model: &str,
) -> Result<ExtractionResult, String> {
    let messages = vec![
        Message { role: MessageRole::System, content: Some(EXTRACTION_SYSTEM_PROMPT.into()), .. },
        Message { role: MessageRole::User, content: Some(format!("Tags: {}\n\nContent: {}", tag_tree_json, chunk_content)), .. },
    ];

    let config = LlmConfig {
        model: model.to_string(),
        params: GenerationParams {
            temperature: Some(0.1),
            max_tokens: Some(1000),
            structured_output: Some(StructuredOutputSchema { /* ... */ }),
        },
    };

    let response = providers.llm().complete(&messages, &config).await?;
    // Parse response into ExtractionResult
}
```

### 4.4 Update `wiki.rs`

Replace `call_openrouter_for_wiki()` with provider calls:

```rust
pub async fn generate_wiki_content(
    providers: &ProviderRegistry,
    tag_name: &str,
    chunks: &[ChunkWithMetadata],
    model: &str,
) -> Result<WikiGenerationResult, String> {
    let config = LlmConfig {
        model: model.to_string(),
        params: GenerationParams {
            temperature: Some(0.3),
            max_tokens: Some(4000),
            structured_output: Some(/* wiki schema */),
        },
    };

    let response = providers.llm().complete(&messages, &config).await?;
    // Parse and return
}
```

### 4.5 Update `agent.rs`

Replace streaming logic with provider abstraction:

```rust
pub async fn run_agent_loop(
    providers: &ProviderRegistry,
    app_handle: AppHandle,
    ctx: AgentContext,
) -> Result<ChatMessageWithContext, String> {
    let config = LlmConfig {
        model: ctx.model.clone(),
        params: GenerationParams {
            temperature: Some(0.7),
            max_tokens: Some(4000),
            structured_output: None,
        },
    };

    let on_delta = Box::new(move |delta: StreamDelta| {
        match delta {
            StreamDelta::Content(text) => {
                app_handle.emit("chat-stream-delta", /* ... */);
            }
            StreamDelta::ToolCallStart { id, name } => {
                app_handle.emit("chat-tool-start", /* ... */);
            }
            // ...
        }
    });

    let response = providers.streaming()
        .complete_streaming_with_tools(&messages, &tools, &config, on_delta)
        .await?;

    // Process response, execute tools, continue loop if needed
}
```

---

## Phase 5: Settings & Frontend Updates

### 5.1 New Settings Schema

| Key | Default | Description |
|-----|---------|-------------|
| `provider` | `"openrouter"` | Active provider (unified for all AI tasks) |
| `openrouter_api_key` | (required) | OpenRouter API key |
| `embedding_model` | `"openai/text-embedding-3-small"` | Model for embeddings |
| `tagging_model` | `"openai/gpt-4o-mini"` | Model for tag extraction |
| `wiki_model` | `"anthropic/claude-sonnet-4.5"` | Model for wiki generation |
| `chat_model` | `"anthropic/claude-sonnet-4"` | Model for chat |
| `auto_tagging_enabled` | `"true"` | Enable auto-tagging |

### 5.2 Settings Migration

Add to `settings.rs`:

```rust
pub fn migrate_settings(conn: &Connection) -> Result<(), String> {
    let defaults = [
        ("provider", "openrouter"),
        ("embedding_model", "openai/text-embedding-3-small"),
        ("wiki_model", "anthropic/claude-sonnet-4.5"),
    ];

    for (key, default) in defaults {
        if get_setting(conn, key).is_err() {
            set_setting(conn, key, default)?;
        }
    }
    Ok(())
}
```

### 5.3 Frontend Settings UI Updates

Update `src/components/settings/SettingsModal.tsx`:

```tsx
// Add provider selector (for future use - only OpenRouter initially)
<div className="space-y-2">
  <label>AI Provider</label>
  <select value={provider} disabled>
    <option value="openrouter">OpenRouter (Cloud)</option>
    {/* Future: <option value="ollama">Ollama (Local)</option> */}
  </select>
</div>

// Add model configuration for all capabilities
<Input label="Embedding Model" value={embeddingModel} />
<Input label="Tagging Model" value={taggingModel} />
<Input label="Wiki Model" value={wikiModel} />
<Input label="Chat Model" value={chatModel} />
```

### 5.4 New Tauri Commands

```rust
#[tauri::command]
pub async fn get_provider_info(state: State<'_, AppState>) -> Result<ProviderInfo, String>;

#[tauri::command]
pub async fn reinitialize_providers(state: State<'_, AppState>) -> Result<(), String>;

#[tauri::command]
pub async fn test_provider_connection(provider: String, settings: HashMap<String, String>) -> Result<bool, String>;
```

---

## Phase 6: Future - Ollama Support

### 6.1 Ollama Provider Implementation

Create `providers/ollama/`:

```rust
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String, // default: http://127.0.0.1:11434
}

// EmbeddingProvider: POST /api/embeddings
// LlmProvider: POST /api/chat
// StreamingLlmProvider: POST /api/chat with stream: true (NDJSON format)
```

### 6.2 Capability Differences

| Capability | OpenRouter | Ollama |
|------------|------------|--------|
| Embeddings | Yes (batch) | Yes (single) |
| Streaming | Yes (SSE) | Yes (NDJSON) |
| Tool Calling | Yes | Limited (model-dependent) |
| Structured Outputs | Yes | No (prompt engineering) |

### 6.3 Fallback Strategies

For Ollama without structured outputs:
- **Tag extraction**: Strong prompts with examples, JSON extraction from markdown
- **Wiki generation**: Prompt for JSON output, parse with fallback
- **Agent**: ReAct-style prompting if tool calling unsupported

### 6.4 Dimension Handling

When switching providers with different embedding dimensions:

```rust
fn ensure_vec_dimension(conn: &Connection, required_dim: usize) -> Result<bool, String> {
    let current = get_setting(conn, "embedding_dimension").ok().and_then(|s| s.parse().ok());

    if current != Some(required_dim) {
        // Dimension changed - need to recreate vec_chunks and re-embed
        conn.execute("DROP TABLE IF EXISTS vec_chunks", [])?;
        conn.execute(&format!(
            "CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id TEXT PRIMARY KEY, embedding float[{}])",
            required_dim
        ), [])?;
        conn.execute("UPDATE atoms SET embedding_status = 'pending'", [])?;
        set_setting(conn, "embedding_dimension", &required_dim.to_string())?;
        return Ok(true); // Needs re-embedding
    }
    Ok(false)
}
```

### 6.5 Ollama Settings

| Key | Default | Description |
|-----|---------|-------------|
| `ollama_host` | `"http://127.0.0.1:11434"` | Ollama server URL |
| `ollama_embedding_model` | `"nomic-embed-text"` | Ollama embedding model |
| `ollama_llm_model` | `"llama3.2:3b"` | Ollama LLM model |

---

## Critical Files Summary

### Must Read Before Implementation

1. **`src-tauri/src/embedding.rs`** - Current embedding logic, batch processing, blob conversion
2. **`src-tauri/src/extraction.rs`** - Structured outputs, retry logic, tag extraction
3. **`src-tauri/src/agent.rs`** - Streaming SSE parsing, tool calling, message accumulation
4. **`src-tauri/src/wiki.rs`** - Wiki generation with structured outputs
5. **`src-tauri/src/db.rs`** - sqlite-lembed code to remove, vec_chunks management
6. **`src-tauri/src/lib.rs`** - App state management pattern

### Files to Create

- `src-tauri/src/providers/mod.rs`
- `src-tauri/src/providers/types.rs`
- `src-tauri/src/providers/error.rs`
- `src-tauri/src/providers/traits.rs`
- `src-tauri/src/providers/registry.rs`
- `src-tauri/src/providers/openrouter/mod.rs`
- `src-tauri/src/providers/openrouter/embedding.rs`
- `src-tauri/src/providers/openrouter/llm.rs`

### Files to Modify

- `src-tauri/src/lib.rs` - Add providers module, app state
- `src-tauri/src/db.rs` - Remove sqlite-lembed, add dimension handling
- `src-tauri/src/embedding.rs` - Use provider abstraction
- `src-tauri/src/extraction.rs` - Use provider abstraction
- `src-tauri/src/wiki.rs` - Use provider abstraction
- `src-tauri/src/agent.rs` - Use provider abstraction
- `src-tauri/src/commands.rs` - Add new provider commands
- `src-tauri/src/settings.rs` - Add migration function
- `src-tauri/Cargo.toml` - Add dependencies
- `src-tauri/tauri.conf.json` - Remove resource entries
- `src/components/settings/SettingsModal.tsx` - Add model configuration
- `src/stores/settings.ts` - Handle new settings keys

### Files to Delete

- `src-tauri/resources/lembed0.so`
- `src-tauri/resources/lembed0-aarch64.dylib`
- `src-tauri/resources/lembed0-x86_64.dylib`
- `src-tauri/resources/all-MiniLM-L6-v2.q8_0.gguf`

---

## Implementation Order

1. **Phase 1**: Create provider module structure and trait definitions (no behavior change)
2. **Phase 2**: Implement OpenRouterEmbeddingProvider, update embedding.rs
3. **Phase 3**: Implement OpenRouterLlmProvider, update extraction.rs
4. **Phase 4**: Update wiki.rs to use provider
5. **Phase 5**: Add streaming support, update agent.rs
6. **Phase 6**: Remove sqlite-lembed infrastructure
7. **Phase 7**: Update settings UI with model configuration
8. **Future**: Add Ollama provider implementation
