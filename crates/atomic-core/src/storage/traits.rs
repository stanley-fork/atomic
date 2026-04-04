//! Storage trait definitions for atomic-core.
//!
//! These traits define the storage abstraction layer. All database operations
//! go through these traits, allowing different backends (SQLite, Postgres, etc.)
//! to be plugged in.
//!
//! All trait methods are async to support both sync backends (SQLite via
//! spawn_blocking) and natively async backends (Postgres via sqlx).

use async_trait::async_trait;

use crate::models::AtomCluster;
use crate::compaction::{CompactionResult, TagMerge};
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::{CreateAtomRequest, ListAtomsParams, UpdateAtomRequest};

/// Result type alias for storage operations.
pub type StorageResult<T> = Result<T, AtomicCoreError>;

// ==================== Atom Storage ====================

/// Storage operations for atoms (the fundamental unit of the knowledge base).
#[async_trait]
pub trait AtomStore: Send + Sync {
    /// Get all atoms with their tags.
    async fn get_all_atoms(&self) -> StorageResult<Vec<AtomWithTags>>;

    /// Count total atoms in this database.
    async fn count_atoms(&self) -> StorageResult<i32>;

    /// Get a single atom by ID with its tags.
    async fn get_atom(&self, id: &str) -> StorageResult<Option<AtomWithTags>>;

    /// Insert a new atom into the database. Returns the created atom with tags.
    /// Does NOT trigger embedding — that's handled by AtomicCore.
    async fn insert_atom(
        &self,
        id: &str,
        request: &CreateAtomRequest,
        created_at: &str,
    ) -> StorageResult<AtomWithTags>;

    /// Insert multiple atoms in a single transaction. Returns the created atoms.
    async fn insert_atoms_bulk(
        &self,
        atoms: &[(String, CreateAtomRequest, String)], // (id, request, created_at)
    ) -> StorageResult<Vec<AtomWithTags>>;

    /// Update an existing atom. Returns the updated atom with tags.
    async fn update_atom(
        &self,
        id: &str,
        request: &UpdateAtomRequest,
        updated_at: &str,
    ) -> StorageResult<AtomWithTags>;

    /// Delete an atom and all associated data (tags, chunks, embeddings, edges).
    async fn delete_atom(&self, id: &str) -> StorageResult<()>;

    /// Get all atoms with a specific tag (including descendants of that tag).
    async fn get_atoms_by_tag(&self, tag_id: &str) -> StorageResult<Vec<AtomWithTags>>;

    /// List atoms with pagination, filtering, and sorting.
    async fn list_atoms(&self, params: &ListAtomsParams) -> StorageResult<PaginatedAtoms>;

    /// Get all unique sources with atom counts.
    async fn get_source_list(&self) -> StorageResult<Vec<SourceInfo>>;

    /// Get embedding status for a specific atom.
    async fn get_embedding_status(&self, atom_id: &str) -> StorageResult<String>;

    /// Get all atom canvas positions.
    async fn get_atom_positions(&self) -> StorageResult<Vec<AtomPosition>>;

    /// Save atom canvas positions (replaces all).
    async fn save_atom_positions(&self, positions: &[AtomPosition]) -> StorageResult<()>;

    /// Get all atoms with their average embedding vectors.
    async fn get_atoms_with_embeddings(&self) -> StorageResult<Vec<AtomWithEmbedding>>;

    /// Get just the tag IDs for an atom (lightweight, no full atom fetch).
    async fn get_atom_tag_ids(&self, atom_id: &str) -> StorageResult<Vec<String>>;

    /// Get just the content for an atom (lightweight, for embedding pipeline).
    async fn get_atom_content(&self, atom_id: &str) -> StorageResult<Option<String>>;

    /// Check which source URLs already exist in the database.
    /// Returns the set of URLs that are already present.
    async fn check_existing_source_urls(&self, urls: &[String]) -> StorageResult<std::collections::HashSet<String>>;

    /// Check if a specific source URL already exists.
    async fn source_url_exists(&self, url: &str) -> StorageResult<bool>;

    /// Get an atom by its source URL. Returns None if not found.
    async fn get_atom_by_source_url(&self, url: &str) -> StorageResult<Option<AtomWithTags>>;

    /// Count atoms with pending embedding status.
    async fn count_pending_embeddings(&self) -> StorageResult<i32>;

    /// Get all average embeddings as (atom_id, embedding) pairs for PCA projection.
    async fn get_all_embedding_pairs(&self) -> StorageResult<Vec<(String, Vec<f32>)>>;

    /// Get semantic edges for canvas visualization, keeping at least top-K per atom.
    /// An edge is kept if either endpoint has fewer than top_k edges so far,
    /// which guarantees every atom gets its strongest connections but allows
    /// hubs to exceed top_k.
    async fn get_top_k_canvas_edges(&self, top_k: usize) -> StorageResult<Vec<CanvasEdgeData>>;

    /// Get all atom-to-tag-id mappings in batch.
    async fn get_all_atom_tag_ids(&self) -> StorageResult<std::collections::HashMap<String, Vec<String>>>;

    /// Get atom metadata for canvas display (title, primary tag, tag count) by position.
    async fn get_canvas_atom_metadata(&self) -> StorageResult<Vec<CanvasAtomPosition>>;
}

// ==================== Tag Storage ====================

/// Storage operations for tags (hierarchical organizational units).
#[async_trait]
pub trait TagStore: Send + Sync {
    /// Get all tags with atom counts, organized hierarchically.
    async fn get_all_tags(&self) -> StorageResult<Vec<TagWithCount>>;

    /// Get all tags filtered by minimum atom count.
    async fn get_all_tags_filtered(&self, min_count: i32) -> StorageResult<Vec<TagWithCount>>;

    /// Get children of a tag with pagination.
    async fn get_tag_children(
        &self,
        parent_id: &str,
        min_count: i32,
        limit: i32,
        offset: i32,
    ) -> StorageResult<PaginatedTagChildren>;

    /// Create a new tag.
    async fn create_tag(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag>;

    /// Update a tag's name and/or parent.
    async fn update_tag(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag>;

    /// Delete a tag. If recursive, also deletes child tags.
    async fn delete_tag(&self, id: &str, recursive: bool) -> StorageResult<()>;

    /// Get tags semantically related to a given tag (via centroid similarity).
    async fn get_related_tags(
        &self,
        tag_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<RelatedTag>>;

    /// Read all tags formatted for compaction LLM input.
    async fn get_tags_for_compaction(&self) -> StorageResult<String>;

    /// Apply tag merge operations (merge source tags into targets).
    async fn apply_tag_merges(
        &self,
        merges: &[TagMerge],
    ) -> StorageResult<CompactionResult>;

    /// Get or create a tag by name, optionally under a parent name.
    /// Returns the tag ID.
    async fn get_or_create_tag(
        &self,
        name: &str,
        parent_name: Option<&str>,
    ) -> StorageResult<String>;

    /// Link tags to an atom (ignores duplicates).
    async fn link_tags_to_atom(
        &self,
        atom_id: &str,
        tag_ids: &[String],
    ) -> StorageResult<()>;

    /// Get the tag tree formatted as JSON for LLM tag extraction.
    async fn get_tag_tree_for_llm(&self) -> StorageResult<String>;

    /// Compute tag centroid embeddings for a batch of tags from their atoms' embeddings.
    async fn compute_tag_centroids_batch(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<()>;

    /// Clean up orphaned parent tags (parents with no children and no atoms).
    async fn cleanup_orphaned_parents(
        &self,
        tag_id: &str,
    ) -> StorageResult<()>;

    /// Get all tag IDs in a hierarchy (the tag itself + all descendants).
    /// Uses a recursive traversal of the tag parent_id tree.
    async fn get_tag_hierarchy(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<String>>;

    /// Count distinct atoms that have any of the given tags.
    async fn count_atoms_with_tags(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<i32>;
}

// ==================== Chunk/Embedding Storage ====================

/// Storage operations for chunks, embeddings, and semantic edges.
#[async_trait]
pub trait ChunkStore: Send + Sync {
    /// Get atoms with pending embedding status (limit batch size).
    async fn get_pending_embeddings(&self, limit: i32) -> StorageResult<Vec<(String, String)>>; // (atom_id, content)

    /// Mark an atom's embedding status (pending, processing, complete, failed).
    async fn set_embedding_status(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()>;

    /// Mark an atom's tagging status.
    async fn set_tagging_status(
        &self,
        atom_id: &str,
        status: &str,
    ) -> StorageResult<()>;

    /// Save chunks and their embeddings for an atom (replaces existing).
    async fn save_chunks_and_embeddings(
        &self,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)], // (chunk_content, embedding)
    ) -> StorageResult<()>;

    /// Delete all chunks and embeddings for an atom.
    async fn delete_chunks(&self, atom_id: &str) -> StorageResult<()>;

    /// Reset atoms stuck in 'processing' status back to 'pending'.
    async fn reset_stuck_processing(&self) -> StorageResult<i32>;

    /// Rebuild semantic edges between all atoms with embeddings.
    async fn rebuild_semantic_edges(&self) -> StorageResult<i32>;

    /// Get semantic edges above a similarity threshold.
    async fn get_semantic_edges(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<SemanticEdge>>;

    /// Get the local neighborhood graph around an atom.
    async fn get_atom_neighborhood(
        &self,
        atom_id: &str,
        depth: i32,
        min_similarity: f32,
    ) -> StorageResult<NeighborhoodGraph>;

    /// Get connection counts for all atoms (tag connections + semantic edges).
    async fn get_connection_counts(
        &self,
        min_similarity: f32,
    ) -> StorageResult<std::collections::HashMap<String, i32>>;

    /// Save tag centroid embedding.
    async fn save_tag_centroid(
        &self,
        tag_id: &str,
        embedding: &[f32],
    ) -> StorageResult<()>;

    /// Recompute all tag centroid embeddings from their atoms' embeddings.
    async fn recompute_all_tag_embeddings(&self) -> StorageResult<i32>;

    /// Check sqlite-vec or equivalent vector extension version.
    async fn check_vector_extension(&self) -> StorageResult<String>;

    /// Atomically claim pending atoms for embedding: sets status to 'processing'
    /// and returns (atom_id, content) pairs. Ensures no double-processing.
    async fn claim_pending_embeddings(&self, limit: i32) -> StorageResult<Vec<(String, String)>>;

    /// Delete chunks for multiple atoms in batch.
    async fn delete_chunks_batch(&self, atom_ids: &[String]) -> StorageResult<()>;

    /// Compute semantic edges for a single atom against all other embedded atoms.
    async fn compute_semantic_edges_for_atom(
        &self,
        atom_id: &str,
        threshold: f32,
        max_edges: i32,
    ) -> StorageResult<i32>;

    /// Rebuild the full-text search index (SQLite: FTS5 rebuild, Postgres: no-op since tsvector is auto-maintained).
    async fn rebuild_fts_index(&self) -> StorageResult<()>;

    /// Atomically claim atoms that need tagging: sets tagging_status to 'processing'
    /// for atoms with embedding_status='complete' and tagging_status='pending'.
    /// Returns the atom IDs that were claimed.
    async fn claim_pending_tagging(&self) -> StorageResult<Vec<String>>;

    /// Get the current embedding dimension from the vector index.
    /// Returns None if the vector index doesn't exist or dimension can't be determined.
    async fn get_embedding_dimension(&self) -> StorageResult<Option<usize>>;

    /// Drop and recreate the vector index with a new dimension, resetting all embedding state.
    async fn recreate_vector_index(&self, dimension: usize) -> StorageResult<()>;

    /// Claim pending/processing atoms for re-embedding after dimension change.
    /// Sets status to 'processing' and returns (atom_id, content) pairs.
    async fn claim_pending_reembedding(&self) -> StorageResult<Vec<(String, String)>>;

    /// Claim ALL atoms for re-embedding regardless of current status.
    /// Sets status to 'processing' and returns (atom_id, content) pairs.
    async fn claim_all_for_reembedding(&self) -> StorageResult<Vec<(String, String)>>;
}

// ==================== Search Storage ====================

/// Storage operations for search (semantic, keyword, hybrid).
#[async_trait]
pub trait SearchStore: Send + Sync {
    /// Perform vector similarity search using embeddings.
    async fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>>;

    /// Perform keyword search using full-text search.
    async fn keyword_search(
        &self,
        query: &str,
        limit: i32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>>;

    /// Find atoms similar to a given atom.
    async fn find_similar(
        &self,
        atom_id: &str,
        limit: i32,
        threshold: f32,
    ) -> StorageResult<Vec<SimilarAtomResult>>;

    /// Search for chunks (not deduplicated by atom) using keyword search.
    /// Returns individual chunk results with scores. Used by wiki agentic research.
    async fn keyword_search_chunks(
        &self,
        query: &str,
        limit: i32,
        scope_tag_ids: &[String],
    ) -> StorageResult<Vec<ChunkSearchResult>>;

    /// Search for chunks using vector similarity.
    /// Returns individual chunk results with scores. Used by wiki agentic research.
    async fn vector_search_chunks(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        scope_tag_ids: &[String],
    ) -> StorageResult<Vec<ChunkSearchResult>>;
}

// ==================== Chat Storage ====================

/// Storage operations for chat conversations and messages.
#[async_trait]
pub trait ChatStore: Send + Sync {
    /// Create a new conversation with optional tag scope.
    async fn create_conversation(
        &self,
        tag_ids: &[String],
        title: Option<&str>,
    ) -> StorageResult<ConversationWithTags>;

    /// List conversations with optional tag filter and pagination.
    async fn get_conversations(
        &self,
        filter_tag_id: Option<&str>,
        limit: i32,
        offset: i32,
    ) -> StorageResult<Vec<ConversationWithTags>>;

    /// Get a conversation with its full message history.
    async fn get_conversation(
        &self,
        conversation_id: &str,
    ) -> StorageResult<Option<ConversationWithMessages>>;

    /// Update conversation metadata.
    async fn update_conversation(
        &self,
        id: &str,
        title: Option<&str>,
        is_archived: Option<bool>,
    ) -> StorageResult<Conversation>;

    /// Delete a conversation and all its messages.
    async fn delete_conversation(&self, id: &str) -> StorageResult<()>;

    /// Set the tag scope for a conversation (replaces existing scope).
    async fn set_conversation_scope(
        &self,
        conversation_id: &str,
        tag_ids: &[String],
    ) -> StorageResult<ConversationWithTags>;

    /// Add a tag to a conversation's scope.
    async fn add_tag_to_scope(
        &self,
        conversation_id: &str,
        tag_id: &str,
    ) -> StorageResult<ConversationWithTags>;

    /// Remove a tag from a conversation's scope.
    async fn remove_tag_from_scope(
        &self,
        conversation_id: &str,
        tag_id: &str,
    ) -> StorageResult<ConversationWithTags>;

    /// Save a chat message (user, assistant, system, or tool).
    async fn save_message(
        &self,
        conversation_id: &str,
        role: &str,
        content: &str,
    ) -> StorageResult<ChatMessage>;

    /// Save tool calls associated with a message.
    async fn save_tool_calls(
        &self,
        message_id: &str,
        tool_calls: &[ChatToolCall],
    ) -> StorageResult<()>;

    /// Save citations for a message.
    async fn save_citations(
        &self,
        message_id: &str,
        citations: &[ChatCitation],
    ) -> StorageResult<()>;

    /// Get the tag IDs that scope a conversation.
    async fn get_scope_tag_ids(
        &self,
        conversation_id: &str,
    ) -> StorageResult<Vec<String>>;

    /// Get a human-readable scope description for the system prompt.
    async fn get_scope_description(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<String>;
}

// ==================== Wiki Storage ====================

/// Storage operations for wiki articles and their metadata.
#[async_trait]
pub trait WikiStore: Send + Sync {
    /// Get a wiki article with its citations for a tag.
    async fn get_wiki(
        &self,
        tag_id: &str,
    ) -> StorageResult<Option<WikiArticleWithCitations>>;

    /// Get wiki article status (exists, atom count, etc.).
    async fn get_wiki_status(
        &self,
        tag_id: &str,
    ) -> StorageResult<WikiArticleStatus>;

    /// Save or update a wiki article with citations.
    async fn save_wiki(
        &self,
        tag_id: &str,
        content: &str,
        citations: &[WikiCitation],
        atom_count: i32,
    ) -> StorageResult<WikiArticleWithCitations>;

    /// Save or update a wiki article with citations and cross-reference links.
    /// This is the full-fidelity save used by wiki generation (includes links).
    async fn save_wiki_with_links(
        &self,
        article: &WikiArticle,
        citations: &[WikiCitation],
        links: &[WikiLink],
    ) -> StorageResult<()>;

    /// Delete a wiki article and its citations.
    async fn delete_wiki(&self, tag_id: &str) -> StorageResult<()>;

    /// Get cross-reference links from a wiki article to other wiki articles.
    async fn get_wiki_links(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<WikiLink>>;

    /// List all versions of a wiki article.
    async fn list_wiki_versions(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<WikiVersionSummary>>;

    /// Get a specific wiki article version.
    async fn get_wiki_version(
        &self,
        version_id: &str,
    ) -> StorageResult<Option<WikiArticleVersion>>;

    /// Get all wiki articles (summaries for list view).
    async fn get_all_wiki_articles(&self) -> StorageResult<Vec<WikiArticleSummary>>;

    /// Get tags that would benefit from having wiki articles.
    async fn get_suggested_wiki_articles(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<SuggestedArticle>>;

    /// Select chunks for wiki article generation, ranked by centroid similarity.
    ///
    /// Returns (chunks, atom_count) for the tag hierarchy. Uses centroid embedding
    /// for ranked retrieval if available, falls back to insertion order.
    async fn get_wiki_source_chunks(
        &self,
        tag_id: &str,
        max_source_tokens: usize,
    ) -> StorageResult<(Vec<ChunkWithContext>, i32)>;

    /// Select chunks for wiki article update (new atoms since last update).
    ///
    /// Returns None if no new atoms have been added since `last_update`.
    /// Otherwise returns (new_chunks, atom_count).
    async fn get_wiki_update_chunks(
        &self,
        tag_id: &str,
        last_update: &str,
        max_source_tokens: usize,
    ) -> StorageResult<Option<(Vec<ChunkWithContext>, i32)>>;
}

// ==================== Feed Storage ====================

/// Storage operations for RSS/Atom feed subscriptions.
#[async_trait]
pub trait FeedStore: Send + Sync {
    /// Create a new feed subscription.
    async fn create_feed(
        &self,
        url: &str,
        title: Option<&str>,
        site_url: Option<&str>,
        poll_interval: i32,
        tag_ids: &[String],
    ) -> StorageResult<Feed>;

    /// List all feed subscriptions.
    async fn list_feeds(&self) -> StorageResult<Vec<Feed>>;

    /// Get a single feed by ID.
    async fn get_feed(&self, id: &str) -> StorageResult<Feed>;

    /// Update a feed subscription.
    async fn update_feed(
        &self,
        id: &str,
        title: Option<&str>,
        poll_interval: Option<i32>,
        is_paused: Option<bool>,
        tag_ids: Option<&[String]>,
    ) -> StorageResult<Feed>;

    /// Delete a feed subscription.
    async fn delete_feed(&self, id: &str) -> StorageResult<()>;

    /// Get feeds that are due for polling.
    async fn get_due_feeds(&self) -> StorageResult<Vec<Feed>>;

    /// Record that a feed was polled (update timestamp and error).
    async fn mark_feed_polled(
        &self,
        id: &str,
        error: Option<&str>,
    ) -> StorageResult<()>;

    /// Atomically claim a feed item GUID. Returns true if this call claimed it.
    async fn claim_feed_item(
        &self,
        feed_id: &str,
        guid: &str,
    ) -> StorageResult<bool>;

    /// Mark a claimed feed item as successfully ingested with its atom_id.
    async fn complete_feed_item(
        &self,
        feed_id: &str,
        guid: &str,
        atom_id: &str,
    ) -> StorageResult<()>;

    /// Mark a claimed feed item as skipped with a reason.
    async fn mark_feed_item_skipped(
        &self,
        feed_id: &str,
        guid: &str,
        reason: &str,
    ) -> StorageResult<()>;

    /// Backfill feed metadata (title, site_url) using COALESCE to avoid overwriting existing values.
    async fn backfill_feed_metadata(
        &self,
        id: &str,
        title: Option<&str>,
        site_url: Option<&str>,
    ) -> StorageResult<()>;
}

// ==================== Clustering Storage ====================

/// Storage operations for atom clustering.
#[async_trait]
pub trait ClusterStore: Send + Sync {
    /// Compute clusters from atom embeddings.
    async fn compute_clusters(
        &self,
        min_similarity: f32,
        min_cluster_size: i32,
    ) -> StorageResult<Vec<AtomCluster>>;

    /// Save computed clusters (replaces existing).
    async fn save_clusters(&self, clusters: &[AtomCluster]) -> StorageResult<()>;

    /// Get cached clusters (recomputes if stale).
    async fn get_clusters(&self) -> StorageResult<Vec<AtomCluster>>;

    /// Get the hierarchical canvas level for a given parent.
    async fn get_canvas_level(
        &self,
        parent_id: Option<&str>,
        children_hint: Option<Vec<String>>,
    ) -> StorageResult<CanvasLevel>;
}

// ==================== Settings Storage ====================

/// Storage operations for key-value settings.
#[async_trait]
pub trait SettingsStore: Send + Sync {
    /// Get all settings as a key-value map.
    async fn get_all_settings(&self) -> StorageResult<std::collections::HashMap<String, String>>;

    /// Get a single setting by key.
    async fn get_setting(&self, key: &str) -> StorageResult<Option<String>>;

    /// Set a setting value (upsert).
    async fn set_setting(&self, key: &str, value: &str) -> StorageResult<()>;
}

// ==================== Token Storage ====================

/// Storage operations for API tokens.
#[async_trait]
pub trait TokenStore: Send + Sync {
    /// Create a new named API token. Returns (metadata, raw_token).
    async fn create_api_token(
        &self,
        name: &str,
    ) -> StorageResult<(crate::tokens::ApiTokenInfo, String)>;

    /// List all API tokens (metadata only).
    async fn list_api_tokens(&self) -> StorageResult<Vec<crate::tokens::ApiTokenInfo>>;

    /// Verify a raw API token. Returns token info if valid and not revoked.
    async fn verify_api_token(
        &self,
        raw_token: &str,
    ) -> StorageResult<Option<crate::tokens::ApiTokenInfo>>;

    /// Revoke an API token by ID.
    async fn revoke_api_token(&self, id: &str) -> StorageResult<()>;

    /// Update the last_used_at timestamp for a token.
    async fn update_token_last_used(&self, id: &str) -> StorageResult<()>;

    /// Migrate legacy server_auth_token to API tokens table.
    async fn migrate_legacy_token(&self) -> StorageResult<bool>;

    /// Ensure at least one token exists. Creates a "default" token if none exist.
    async fn ensure_default_token(&self) -> StorageResult<Option<(crate::tokens::ApiTokenInfo, String)>>;
}

// ==================== Database Management Storage ====================

/// Storage operations for managing logical databases.
#[async_trait]
pub trait DatabaseStore: Send + Sync {
    /// List all registered databases.
    async fn list_databases(&self) -> StorageResult<Vec<crate::registry::DatabaseInfo>>;

    /// Create a new database entry. Returns the new database info.
    async fn create_database(&self, name: &str) -> StorageResult<crate::registry::DatabaseInfo>;

    /// Rename a database.
    async fn rename_database(&self, id: &str, name: &str) -> StorageResult<()>;

    /// Delete a database entry (cannot delete default).
    async fn delete_database(&self, id: &str) -> StorageResult<()>;

    /// Get the ID of the default database.
    async fn get_default_database_id(&self) -> StorageResult<String>;

    /// Set a database as the new default.
    async fn set_default_database(&self, id: &str) -> StorageResult<()>;

    /// Purge all data for a logical database (delete all rows with the given db_id).
    /// Called after deleting the database entry to avoid orphaned data.
    async fn purge_database_data(&self, db_id: &str) -> StorageResult<()>;
}

// ==================== Supertrait ====================

/// Combined storage trait. Every storage backend must implement all sub-traits.
///
/// This is the main trait that `AtomicCore` holds as `Arc<dyn Storage>`.
#[async_trait]
pub trait Storage:
    AtomStore
    + TagStore
    + ChunkStore
    + SearchStore
    + ChatStore
    + WikiStore
    + FeedStore
    + ClusterStore
    + SettingsStore
    + TokenStore
    + DatabaseStore
    + Send
    + Sync
{
    /// Initialize the storage backend (run migrations, create tables, etc.).
    async fn initialize(&self) -> StorageResult<()>;

    /// Graceful shutdown (optimize, flush, etc.).
    async fn shutdown(&self) -> StorageResult<()>;

    /// Get the database/storage path (for display purposes).
    fn storage_path(&self) -> &std::path::Path;
}
