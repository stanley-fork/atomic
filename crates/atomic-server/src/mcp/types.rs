use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== Tool Input Types ====================

/// Input parameters for semantic_search tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SemanticSearchParams {
    /// The search query to find relevant atoms using vector similarity
    pub query: String,

    /// Maximum number of results to return (default: 10, max: 50)
    #[serde(default)]
    pub limit: Option<i32>,

    /// Minimum similarity threshold between 0.0 and 1.0 (default: 0.3)
    #[serde(default)]
    pub threshold: Option<f32>,
}

/// Input parameters for read_atom tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadAtomParams {
    /// The UUID of the atom to retrieve
    pub atom_id: String,

    /// Maximum number of lines to return (default: 100, max: 500)
    #[serde(default)]
    pub limit: Option<i32>,

    /// Line offset for pagination, 0-indexed (default: 0)
    #[serde(default)]
    pub offset: Option<i32>,
}

/// Input parameters for create_atom tool
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateAtomParams {
    /// The markdown content of the atom
    pub content: String,

    /// Optional source URL where this content originated
    #[serde(default)]
    pub source_url: Option<String>,

    /// Optional list of existing tag UUIDs to associate with this atom
    #[serde(default)]
    pub tag_ids: Option<Vec<String>>,
}

// ==================== Tool Output Types ====================

/// A search result with atom content and similarity score
#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub atom_id: String,
    pub content_preview: String,
    pub similarity_score: f32,
    pub matching_chunk: String,
}

/// Paginated atom content response
#[derive(Debug, Serialize)]
pub struct AtomContent {
    pub atom_id: String,
    pub content: String,
    pub total_lines: i32,
    pub returned_lines: i32,
    pub offset: i32,
    pub has_more: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Created atom response
#[derive(Debug, Serialize)]
pub struct CreatedAtom {
    pub atom_id: String,
    pub content_preview: String,
    pub tags: Vec<String>,
    pub embedding_status: String,
}
