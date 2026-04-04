//! Curated registry of OpenRouter text embedding models.
//!
//! OpenRouter exposes a `/api/v1/embeddings/models` endpoint that lists available
//! embedding models, but the schema does not include vector dimensions — those only
//! appear in free-text descriptions. Since Atomic's SQLite-vec index is fixed at
//! creation time, we need authoritative dimensions per model, so we hand-curate
//! the list here.
//!
//! Adding a new model is a one-line change: append to `EMBEDDING_MODELS`.

use serde::{Deserialize, Serialize};

/// Metadata for an OpenRouter text embedding model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterEmbeddingModel {
    /// OpenRouter model ID (e.g. "openai/text-embedding-3-small")
    pub id: &'static str,
    /// Human-readable display name
    pub name: &'static str,
    /// Native output vector dimension (what the API returns by default)
    pub dimension: usize,
    /// Maximum input context length in tokens
    pub context_length: usize,
}

/// Curated list of OpenRouter text embedding models with verified dimensions.
///
/// Dimensions are the size the API actually returns when we don't pass a
/// `dimensions` / `output_dimension` parameter — which is how `embed_batch`
/// currently calls the endpoint. For MRL-capable models this is the provider's
/// *default*, which may be smaller than the native maximum (e.g. Codestral Embed
/// is native 3072 but defaults to 1536).
pub const EMBEDDING_MODELS: &[OpenRouterEmbeddingModel] = &[
    // OpenAI
    OpenRouterEmbeddingModel {
        id: "openai/text-embedding-3-small",
        name: "OpenAI: text-embedding-3-small",
        dimension: 1536,
        context_length: 8192,
    },
    OpenRouterEmbeddingModel {
        id: "openai/text-embedding-3-large",
        name: "OpenAI: text-embedding-3-large",
        dimension: 3072,
        context_length: 8192,
    },
    OpenRouterEmbeddingModel {
        id: "openai/text-embedding-ada-002",
        name: "OpenAI: text-embedding-ada-002",
        dimension: 1536,
        context_length: 8192,
    },
    // Google
    OpenRouterEmbeddingModel {
        id: "google/gemini-embedding-001",
        name: "Google: Gemini Embedding 001",
        dimension: 3072,
        context_length: 20000,
    },
    // Mistral
    OpenRouterEmbeddingModel {
        id: "mistralai/mistral-embed-2312",
        name: "Mistral: Mistral Embed 2312",
        dimension: 1024,
        context_length: 8192,
    },
    OpenRouterEmbeddingModel {
        id: "mistralai/codestral-embed-2505",
        name: "Mistral: Codestral Embed 2505 (code)",
        dimension: 1536,
        context_length: 8192,
    },
    // Qwen
    OpenRouterEmbeddingModel {
        id: "qwen/qwen3-embedding-8b",
        name: "Qwen: Qwen3 Embedding 8B",
        dimension: 4096,
        context_length: 32000,
    },
    OpenRouterEmbeddingModel {
        id: "qwen/qwen3-embedding-4b",
        name: "Qwen: Qwen3 Embedding 4B",
        dimension: 2560,
        context_length: 32768,
    },
    // BAAI
    OpenRouterEmbeddingModel {
        id: "baai/bge-m3",
        name: "BAAI: bge-m3 (multilingual)",
        dimension: 1024,
        context_length: 8192,
    },
    OpenRouterEmbeddingModel {
        id: "baai/bge-large-en-v1.5",
        name: "BAAI: bge-large-en-v1.5",
        dimension: 1024,
        context_length: 512,
    },
    OpenRouterEmbeddingModel {
        id: "baai/bge-base-en-v1.5",
        name: "BAAI: bge-base-en-v1.5",
        dimension: 768,
        context_length: 512,
    },
    // Intfloat E5
    OpenRouterEmbeddingModel {
        id: "intfloat/multilingual-e5-large",
        name: "Intfloat: Multilingual E5 Large",
        dimension: 1024,
        context_length: 512,
    },
    OpenRouterEmbeddingModel {
        id: "intfloat/e5-large-v2",
        name: "Intfloat: E5 Large v2",
        dimension: 1024,
        context_length: 512,
    },
    OpenRouterEmbeddingModel {
        id: "intfloat/e5-base-v2",
        name: "Intfloat: E5 Base v2",
        dimension: 768,
        context_length: 512,
    },
    // Thenlper GTE
    OpenRouterEmbeddingModel {
        id: "thenlper/gte-large",
        name: "Thenlper: GTE Large",
        dimension: 1024,
        context_length: 512,
    },
    OpenRouterEmbeddingModel {
        id: "thenlper/gte-base",
        name: "Thenlper: GTE Base",
        dimension: 768,
        context_length: 512,
    },
];

/// Get the full list of curated embedding models.
pub fn get_embedding_models() -> &'static [OpenRouterEmbeddingModel] {
    EMBEDDING_MODELS
}

/// Look up the native vector dimension for a given OpenRouter embedding model ID.
/// Returns `None` if the model is not in the curated registry.
pub fn get_embedding_dimension(model_id: &str) -> Option<usize> {
    EMBEDDING_MODELS
        .iter()
        .find(|m| m.id == model_id)
        .map(|m| m.dimension)
}
