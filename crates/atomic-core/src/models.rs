//! Data models for atomic-core
//!
//! This module contains all the core data structures used throughout the library.

use serde::{Deserialize, Serialize};

// ==================== Core KB Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Atom {
    pub id: String,
    pub content: String,
    pub title: String,
    pub snippet: String,
    pub source_url: Option<String>,
    pub source: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub embedding_status: String, // 'pending', 'processing', 'complete', 'failed'
    pub tagging_status: String,   // 'pending', 'processing', 'complete', 'failed', 'skipped'
    pub embedding_error: Option<String>,
    pub tagging_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomWithTags {
    #[serde(flatten)]
    pub atom: Atom,
    pub tags: Vec<Tag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(no_recursion))]
pub struct TagWithCount {
    #[serde(flatten)]
    pub tag: Tag,
    pub atom_count: i32,
    pub children_total: i32,
    pub children: Vec<TagWithCount>,
}

/// Paginated response for tag children
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PaginatedTagChildren {
    pub children: Vec<TagWithCount>,
    pub total: i32,
}

/// Lightweight atom summary for paginated list views (no full content)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomSummary {
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub source_url: Option<String>,
    pub source: Option<String>,
    pub published_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub embedding_status: String,
    pub tagging_status: String,
    pub embedding_error: Option<String>,
    pub tagging_error: Option<String>,
    pub tags: Vec<Tag>,
}

/// Paginated response for atom list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PaginatedAtoms {
    pub atoms: Vec<AtomSummary>,
    pub total_count: i32,
    pub limit: i32,
    pub offset: i32,
    /// Cursor for keyset pagination: updated_at of the last item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Cursor tiebreaker: id of the last item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor_id: Option<String>,
}

/// Result struct for bulk atom creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BulkCreateResult {
    pub atoms: Vec<AtomWithTags>,
    pub count: usize,
    pub skipped: usize,
}

/// Result struct for similar atom search
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SimilarAtomResult {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub similarity_score: f32,
    pub matching_chunk_content: String,
    pub matching_chunk_index: i32,
}

/// Result struct for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SemanticSearchResult {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub similarity_score: f32,
    pub matching_chunk_content: String,
    pub matching_chunk_index: i32,
}

/// Payload for embedding-complete event (embedding only, no tags)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingCompletePayload {
    pub atom_id: String,
    pub status: String, // "complete" or "failed"
    pub error: Option<String>,
}

/// Payload for tagging-complete event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggingCompletePayload {
    pub atom_id: String,
    pub status: String, // "complete", "failed", or "skipped"
    pub error: Option<String>,
    pub tags_extracted: Vec<String>,   // IDs of all tags applied
    pub new_tags_created: Vec<String>, // IDs of newly created tags
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiCitation {
    pub id: String,
    pub citation_index: i32,
    pub atom_id: String,
    pub chunk_index: Option<i32>,
    pub excerpt: String,
}

/// Wiki article with all its citations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiArticleWithCitations {
    pub article: WikiArticle,
    pub citations: Vec<WikiCitation>,
}

/// Status of a wiki article for quick checks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiArticleStatus {
    pub has_article: bool,
    pub article_atom_count: i32,
    pub current_atom_count: i32,
    pub new_atoms_available: i32,
    pub updated_at: Option<String>,
}

/// Summary of a wiki article for list view (includes tag name)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiArticleSummary {
    pub id: String,
    pub tag_id: String,
    pub tag_name: String,
    pub updated_at: String,
    pub atom_count: i32,
    pub inbound_links: i32,
}

/// Inter-article wiki link (cross-reference between wiki articles)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiLink {
    pub id: String,
    pub source_article_id: String,
    pub target_tag_name: String,
    pub target_tag_id: Option<String>,
    pub has_article: bool,
}

/// Tag related to another tag by semantic connectivity
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelatedTag {
    pub tag_id: String,
    pub tag_name: String,
    pub score: f64,
    pub shared_atoms: i32,
    pub semantic_edges: i32,
    pub has_article: bool,
}

/// Suggested wiki article for tags that don't have articles yet
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SuggestedArticle {
    pub tag_id: String,
    pub tag_name: String,
    pub atom_count: i32,
    pub mention_count: i32,
    pub score: f64,
}

/// Archived version of a wiki article
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiArticleVersion {
    pub id: String,
    pub tag_id: String,
    pub content: String,
    pub citations: Vec<WikiCitation>,
    pub atom_count: i32,
    pub version_number: i32,
    pub created_at: String,
}

/// Summary of a wiki article version for list views
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WikiVersionSummary {
    pub id: String,
    pub version_number: i32,
    pub atom_count: i32,
    pub created_at: String,
}

/// Chunk with context for wiki generation
#[derive(Debug, Clone)]
pub struct ChunkWithContext {
    pub atom_id: String,
    pub chunk_index: i32,
    pub content: String,
    pub similarity_score: f32,
}

/// Individual chunk search result (not deduplicated by atom).
/// Used by wiki agentic research and other chunk-level search needs.
#[derive(Debug, Clone)]
pub struct ChunkSearchResult {
    pub chunk_id: String,
    pub atom_id: String,
    pub content: String,
    pub chunk_index: i32,
    /// Normalized score (0.0-1.0), higher is better
    pub score: f32,
}

/// Position of an atom on the canvas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomPosition {
    pub atom_id: String,
    pub x: f64,
    pub y: f64,
}

/// Atom with 2D position and metadata for the global canvas view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CanvasAtomPosition {
    pub atom_id: String,
    pub x: f64,
    pub y: f64,
    pub title: String,
    pub primary_tag: Option<String>,
    pub tag_count: i32,
    pub tag_ids: Vec<String>,
}

/// Edge between two atoms for the global canvas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CanvasEdgeData {
    pub source: String,
    pub target: String,
    pub weight: f32,
}

/// Cluster centroid label for the global canvas
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CanvasClusterLabel {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub label: String,
    pub atom_count: i32,
    pub atom_ids: Vec<String>,
}

/// Full response for the global canvas view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct GlobalCanvasData {
    pub atoms: Vec<CanvasAtomPosition>,
    pub edges: Vec<CanvasEdgeData>,
    pub clusters: Vec<CanvasClusterLabel>,
}

/// Atom with its average embedding vector for similarity calculations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomWithEmbedding {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub embedding: Option<Vec<f32>>,  // Average of chunk embeddings, None if not yet embedded
}

// ==================== Semantic Graph Types ====================

/// Pre-computed semantic edge between two atoms
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SemanticEdge {
    pub id: String,
    pub source_atom_id: String,
    pub target_atom_id: String,
    pub similarity_score: f32,
    pub source_chunk_index: Option<i32>,
    pub target_chunk_index: Option<i32>,
    pub created_at: String,
}

/// Neighborhood graph for local graph view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NeighborhoodGraph {
    pub center_atom_id: String,
    pub atoms: Vec<NeighborhoodAtom>,
    pub edges: Vec<NeighborhoodEdge>,
}

/// Atom in a neighborhood graph with depth info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NeighborhoodAtom {
    #[serde(flatten)]
    pub atom: AtomWithTags,
    pub depth: i32, // 0 = center, 1 = direct connection, 2 = friend-of-friend
}

/// Edge in a neighborhood graph (combines tag and semantic connections)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct NeighborhoodEdge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String, // "tag", "semantic", "both"
    pub strength: f32,     // Combined strength (0-1)
    pub shared_tag_count: i32,
    pub similarity_score: Option<f32>,
}

/// Atom cluster assignment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AtomCluster {
    pub cluster_id: i32,
    pub atom_ids: Vec<String>,
    pub dominant_tags: Vec<String>,
}

// ==================== Canvas Hierarchy Types ====================

/// Type of node in the hierarchical canvas view
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CanvasNodeType {
    Category,
    Tag,
    SemanticCluster,
    Atom,
}

/// A node in the hierarchical canvas view
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CanvasNode {
    pub id: String,
    pub node_type: CanvasNodeType,
    pub label: String,
    pub atom_count: i32,
    pub children_ids: Vec<String>,
    pub dominant_tags: Vec<String>,
    pub centroid: Option<Vec<f32>>,
}

/// An edge between two nodes at the same level
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CanvasEdge {
    pub source_id: String,
    pub target_id: String,
    pub weight: f32,
}

/// Entry in the breadcrumb navigation trail
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BreadcrumbEntry {
    pub id: String,
    pub label: String,
}

/// A single level in the hierarchical canvas, returned by get_canvas_level()
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CanvasLevel {
    pub parent_id: Option<String>,
    pub parent_label: Option<String>,
    pub breadcrumb: Vec<BreadcrumbEntry>,
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
}

// ==================== Chat Types ====================
// These are included here for use by the Tauri app's chat functionality,
// even though chat is not part of atomic-core's scope.

/// Chat conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub is_archived: bool,
}

/// Conversation with its tag scope and summary info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConversationWithTags {
    #[serde(flatten)]
    pub conversation: Conversation,
    pub tags: Vec<Tag>,
    pub message_count: i32,
    pub last_message_preview: Option<String>,
}

/// Conversation with full message history
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ConversationWithMessages {
    #[serde(flatten)]
    pub conversation: Conversation,
    pub tags: Vec<Tag>,
    pub messages: Vec<ChatMessageWithContext>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChatMessage {
    pub id: String,
    pub conversation_id: String,
    pub role: String, // "user", "assistant", "system", "tool"
    pub content: String,
    pub created_at: String,
    pub message_index: i32,
}

/// Message with tool calls and citations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChatMessageWithContext {
    #[serde(flatten)]
    pub message: ChatMessage,
    pub tool_calls: Vec<ChatToolCall>,
    pub citations: Vec<ChatCitation>,
}

/// Tool call record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChatToolCall {
    pub id: String,
    pub message_id: String,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_output: Option<serde_json::Value>,
    pub status: String, // "pending", "running", "complete", "failed"
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// Citation in a chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChatCitation {
    pub id: String,
    pub message_id: String,
    pub citation_index: i32,
    pub atom_id: String,
    pub chunk_index: Option<i32>,
    pub excerpt: String,
    pub relevance_score: Option<f32>,
}

// ==================== Feed Types ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Feed {
    pub id: String,
    pub url: String,
    pub title: Option<String>,
    pub site_url: Option<String>,
    pub poll_interval: i32,
    pub last_polled_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub is_paused: bool,
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateFeedRequest {
    pub url: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: i32,
    #[serde(default)]
    pub tag_ids: Vec<String>,
}

fn default_poll_interval() -> i32 {
    60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateFeedRequest {
    pub poll_interval: Option<i32>,
    pub is_paused: Option<bool>,
    pub tag_ids: Option<Vec<String>>,
}

// ==================== Filtering & Sorting Types ====================

/// Source filter for atom list queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFilter {
    #[default]
    All,
    Manual,
    External,
}

/// Sort field for atom list queries
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    Updated,
    Created,
    Published,
    Title,
}

/// Sort direction
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Desc,
    Asc,
}

/// Parameters for list_atoms query
#[derive(Debug, Clone)]
pub struct ListAtomsParams {
    pub tag_id: Option<String>,
    pub limit: i32,
    pub offset: i32,
    pub cursor: Option<String>,
    pub cursor_id: Option<String>,
    pub source_filter: SourceFilter,
    pub source_value: Option<String>,
    pub sort_by: SortField,
    pub sort_order: SortOrder,
}

/// Source with atom count for filter dropdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SourceInfo {
    pub source: String,
    pub atom_count: i32,
}

// ==================== Pipeline Status ====================

/// Result of changing a provider-related setting
#[derive(Debug, Clone, Serialize)]
pub struct SettingChangeResult {
    pub dimension_changed: bool,
    pub old_dim: usize,
    pub new_dim: usize,
    pub total_atom_count: i32,
    pub retried_failed_count: i32,
}

/// Embedding/tagging pipeline status summary
#[derive(Debug, Clone, Serialize)]
pub struct PipelineStatus {
    pub pending: i32,
    pub processing: i32,
    pub complete: i32,
    pub failed_count: i32,
    pub failed: Vec<FailedAtom>,
    pub tagging_failed_count: i32,
    pub tagging_failed: Vec<FailedAtom>,
}

/// An atom that failed embedding or tagging
#[derive(Debug, Clone, Serialize)]
pub struct FailedAtom {
    pub atom_id: String,
    pub title: String,
    pub snippet: String,
    pub error: Option<String>,
    pub updated_at: String,
}
