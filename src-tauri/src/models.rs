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

