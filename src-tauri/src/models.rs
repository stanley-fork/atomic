use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atom {
    pub id: String,
    pub content: String,
    pub source_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub embedding_status: String, // 'pending', 'processing', 'complete', 'failed'
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomWithTags {
    #[serde(flatten)]
    pub atom: Atom,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagWithCount {
    #[serde(flatten)]
    pub tag: Tag,
    pub atom_count: i32,
    pub children: Vec<TagWithCount>,
}

/// Result struct for similar atom search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarAtomResult {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub similarity_score: f32,
    pub matching_chunk_content: String,
    pub matching_chunk_index: i32,
}

/// Result struct for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticSearchResult {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub similarity_score: f32,
    pub matching_chunk_content: String,
    pub matching_chunk_index: i32,
}

/// Payload for embedding-complete event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingCompletePayload {
    pub atom_id: String,
    pub status: String, // "complete" or "failed"
    pub error: Option<String>,
    pub tags_extracted: Vec<String>,      // IDs of all tags applied
    pub new_tags_created: Vec<String>,    // IDs of newly created tags
}

/// Chunk data for internal use
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ChunkData {
    pub id: String,
    pub atom_id: String,
    pub chunk_index: i32,
    pub content: String,
}

/// Wiki article for a tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiArticle {
    pub id: String,
    pub tag_id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub atom_count: i32,
}

/// Citation linking article content to source atom/chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiCitation {
    pub id: String,
    pub citation_index: i32,
    pub atom_id: String,
    pub chunk_index: Option<i32>,
    pub excerpt: String,
}

/// Wiki article with all its citations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiArticleWithCitations {
    pub article: WikiArticle,
    pub citations: Vec<WikiCitation>,
}

/// Status of a wiki article for quick checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiArticleStatus {
    pub has_article: bool,
    pub article_atom_count: i32,
    pub current_atom_count: i32,
    pub new_atoms_available: i32,
    pub updated_at: Option<String>,
}

/// Chunk with context for wiki generation
#[derive(Debug, Clone)]
pub struct ChunkWithContext {
    pub atom_id: String,
    pub chunk_index: i32,
    pub content: String,
    pub similarity_score: f32,
}

/// Position of an atom on the canvas
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomPosition {
    pub atom_id: String,
    pub x: f64,
    pub y: f64,
}

/// Atom with its average embedding vector for similarity calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomWithEmbedding {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub embedding: Option<Vec<f32>>,  // Average of chunk embeddings, None if not yet embedded
}

