//! Storage abstraction layer for atomic-core.
//!
//! This module defines the trait hierarchy for database backends and provides
//! the default SQLite implementation. Alternative backends (e.g., Postgres)
//! can be added by implementing the `Storage` supertrait.

pub mod traits;
pub mod sqlite;
pub mod postgres;

pub use traits::*;
pub use sqlite::SqliteStorage;

#[cfg(feature = "postgres")]
pub use postgres::PostgresStorage;

use crate::error::AtomicCoreError;

/// Runtime-dispatched storage backend.
///
/// AtomicCore holds this enum to support both SQLite and Postgres at runtime.
/// For SQLite, sync helper methods are called directly.
/// For Postgres, async trait methods are bridged to sync via `block_on`.
#[derive(Clone)]
pub enum StorageBackend {
    Sqlite(SqliteStorage),
    #[cfg(feature = "postgres")]
    Postgres(PostgresStorage),
}

impl StorageBackend {
    /// Get the underlying SqliteStorage, if this is a SQLite backend.
    /// Used for operations not yet abstracted behind the storage trait
    /// (e.g., embedding pipeline internals that directly use `Arc<Database>`).
    pub(crate) fn as_sqlite(&self) -> Option<&SqliteStorage> {
        match self {
            StorageBackend::Sqlite(s) => Some(s),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres(_) => None,
        }
    }

    /// Get the underlying PostgresStorage, if this is a Postgres backend.
    #[cfg(feature = "postgres")]
    pub(crate) fn as_postgres(&self) -> Option<&PostgresStorage> {
        match self {
            StorageBackend::Sqlite(_) => None,
            StorageBackend::Postgres(s) => Some(s),
        }
    }

    /// Run storage-specific optimization (SQLite PRAGMA optimize, Postgres no-op).
    pub(crate) fn optimize(&self) {
        match self {
            StorageBackend::Sqlite(s) => s.db.optimize(),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres(_) => {} // Postgres handles this automatically
        }
    }

    /// Get the underlying Database for operations not yet migrated to storage traits.
    /// Returns None for Postgres backend.
    pub(crate) fn sqlite_db(&self) -> Option<&std::sync::Arc<crate::db::Database>> {
        match self {
            StorageBackend::Sqlite(s) => Some(&s.db),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres(_) => None,
        }
    }

    /// Get pipeline status (embedding counts + failed atoms).
    pub(crate) fn get_pipeline_status(&self) -> Result<crate::models::PipelineStatus, AtomicCoreError> {
        match self {
            StorageBackend::Sqlite(s) => s.get_pipeline_status_sync(),
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres(s) => s.get_pipeline_status_sync(),
        }
    }

    /// Get the database path (for display).
    pub(crate) fn storage_path(&self) -> &std::path::Path {
        match self {
            StorageBackend::Sqlite(s) => &s.db.db_path,
            #[cfg(feature = "postgres")]
            StorageBackend::Postgres(_) => std::path::Path::new("postgres"),
        }
    }
}

/// Dedicated tokio runtime for Postgres async operations.
///
/// The sync→async bridge problem:
/// - actix-web workers use `current_thread` tokio → `block_in_place` panics
/// - `handle.block_on()` from a child thread deadlocks on `current_thread`
/// - A new runtime per call can't share the sqlx pool (pool is runtime-bound)
///
/// Solution: one shared multi-thread runtime created at process start.
/// The sqlx pool is created on THIS runtime (via `connect()` called from
/// `open_postgres()` which runs on the main runtime before actix starts).
/// All sync dispatch calls send work here and block on the result.
#[cfg(feature = "postgres")]
static PG_RUNTIME: std::sync::LazyLock<tokio::runtime::Runtime> = std::sync::LazyLock::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("Failed to create Postgres runtime")
});

/// Bridge an async Postgres call to sync on the dedicated PG_RUNTIME.
///
/// If called from within a tokio runtime (e.g., actix-web handler or `#[tokio::main]`),
/// moves to a background thread to avoid "cannot start a runtime from within a runtime".
/// If called from a non-async context, calls `block_on` directly.
#[cfg(feature = "postgres")]
fn block_on<F: std::future::Future + Send>(f: F) -> F::Output
where
    F::Output: Send,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        // We're inside a tokio runtime — can't call block_on directly.
        // Move to a plain thread where block_on is safe.
        std::thread::scope(|s| {
            s.spawn(|| PG_RUNTIME.block_on(f))
                .join()
                .expect("PG_RUNTIME bridge thread panicked")
        })
    } else {
        // No active runtime — call block_on directly.
        PG_RUNTIME.block_on(f)
    }
}

/// Public version for use by PostgresStorage::connect and initialize.
#[cfg(feature = "postgres")]
pub fn pg_runtime_block_on<F: std::future::Future + Send>(f: F) -> F::Output
where
    F::Output: Send,
{
    block_on(f)
}

// ==================== Sync dispatch methods ====================
//
// Each method dispatches to either the SqliteStorage sync helper
// or the PostgresStorage async trait method (via block_on).
// This keeps AtomicCore's public API synchronous while supporting
// both backends at runtime.

/// Macro to generate dispatch methods. For each method:
/// - SQLite: calls `s.$sqlite_method($($arg),*)`
/// - Postgres: calls `block_on(<TraitName>::$trait_method(s, $($arg),*))`
macro_rules! dispatch {
    (
        $(
            fn $name:ident(&self $(, $arg:ident: $argty:ty)*) -> $ret:ty
                => sqlite: $sqlite_method:ident, pg_trait: $trait_name:path, pg_method: $pg_method:ident;
        )*
    ) => {
        impl StorageBackend {
            $(
                pub(crate) fn $name(&self $(, $arg: $argty)*) -> $ret {
                    match self {
                        StorageBackend::Sqlite(s) => s.$sqlite_method($($arg),*),
                        #[cfg(feature = "postgres")]
                        StorageBackend::Postgres(s) => {
                            block_on(<PostgresStorage as $trait_name>::$pg_method(s $(, $arg)*))
                        }
                    }
                }
            )*
        }
    };
}

use crate::compaction::{CompactionResult, TagMerge};
use crate::models::*;
use crate::{CreateAtomRequest, ListAtomsParams, UpdateAtomRequest};
use std::collections::{HashMap, HashSet};

dispatch! {
    // ---- AtomStore ----
    fn count_atoms_impl(&self) -> Result<i32, AtomicCoreError>
        => sqlite: count_atoms_impl, pg_trait: AtomStore, pg_method: count_atoms;
    fn get_all_atoms_impl(&self) -> Result<Vec<AtomWithTags>, AtomicCoreError>
        => sqlite: get_all_atoms_impl, pg_trait: AtomStore, pg_method: get_all_atoms;
    fn get_atom_impl(&self, id: &str) -> Result<Option<AtomWithTags>, AtomicCoreError>
        => sqlite: get_atom_impl, pg_trait: AtomStore, pg_method: get_atom;
    fn insert_atom_impl(&self, id: &str, request: &CreateAtomRequest, created_at: &str) -> Result<AtomWithTags, AtomicCoreError>
        => sqlite: insert_atom_impl, pg_trait: AtomStore, pg_method: insert_atom;
    fn insert_atoms_bulk_impl(&self, atoms: &[(String, CreateAtomRequest, String)]) -> Result<Vec<AtomWithTags>, AtomicCoreError>
        => sqlite: insert_atoms_bulk_impl, pg_trait: AtomStore, pg_method: insert_atoms_bulk;
    fn update_atom_impl(&self, id: &str, request: &UpdateAtomRequest, updated_at: &str) -> Result<AtomWithTags, AtomicCoreError>
        => sqlite: update_atom_impl, pg_trait: AtomStore, pg_method: update_atom;
    fn update_atom_content_only_impl(&self, id: &str, request: &UpdateAtomRequest, updated_at: &str) -> Result<AtomWithTags, AtomicCoreError>
        => sqlite: update_atom_content_only_impl, pg_trait: AtomStore, pg_method: update_atom_content_only;
    fn delete_atom_impl(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_atom_impl, pg_trait: AtomStore, pg_method: delete_atom;
    fn get_atoms_by_tag_impl(&self, tag_id: &str) -> Result<Vec<AtomWithTags>, AtomicCoreError>
        => sqlite: get_atoms_by_tag_impl, pg_trait: AtomStore, pg_method: get_atoms_by_tag;
    fn list_atoms_impl(&self, params: &ListAtomsParams) -> Result<PaginatedAtoms, AtomicCoreError>
        => sqlite: list_atoms_impl, pg_trait: AtomStore, pg_method: list_atoms;
    fn get_source_list_impl(&self) -> Result<Vec<SourceInfo>, AtomicCoreError>
        => sqlite: get_source_list_impl, pg_trait: AtomStore, pg_method: get_source_list;
    fn get_embedding_status_impl(&self, atom_id: &str) -> Result<String, AtomicCoreError>
        => sqlite: get_embedding_status_impl, pg_trait: AtomStore, pg_method: get_embedding_status;
    fn get_tagging_status_impl(&self, atom_id: &str) -> Result<String, AtomicCoreError>
        => sqlite: get_tagging_status_impl, pg_trait: AtomStore, pg_method: get_tagging_status;
    fn get_atom_positions_impl(&self) -> Result<Vec<AtomPosition>, AtomicCoreError>
        => sqlite: get_atom_positions_impl, pg_trait: AtomStore, pg_method: get_atom_positions;
    fn save_atom_positions_impl(&self, positions: &[AtomPosition]) -> Result<(), AtomicCoreError>
        => sqlite: save_atom_positions_impl, pg_trait: AtomStore, pg_method: save_atom_positions;
    fn get_atoms_with_embeddings_impl(&self) -> Result<Vec<AtomWithEmbedding>, AtomicCoreError>
        => sqlite: get_atoms_with_embeddings_impl, pg_trait: AtomStore, pg_method: get_atoms_with_embeddings;
    fn get_atom_tag_ids_impl(&self, atom_id: &str) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: get_atom_tag_ids_impl, pg_trait: AtomStore, pg_method: get_atom_tag_ids;
    fn get_tag_ids_for_atoms_batch_impl(&self, atom_ids: &[String]) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: get_tag_ids_for_atoms_batch_impl, pg_trait: AtomStore, pg_method: get_tag_ids_for_atoms_batch;
    fn get_atom_content_impl(&self, atom_id: &str) -> Result<Option<String>, AtomicCoreError>
        => sqlite: get_atom_content_impl, pg_trait: AtomStore, pg_method: get_atom_content;
    fn get_atom_contents_batch_impl(&self, atom_ids: &[String]) -> Result<Vec<(String, String)>, AtomicCoreError>
        => sqlite: get_atom_contents_batch_impl, pg_trait: AtomStore, pg_method: get_atom_contents_batch;
    fn check_existing_source_urls_sync(&self, urls: &[String]) -> Result<HashSet<String>, AtomicCoreError>
        => sqlite: check_existing_source_urls_sync, pg_trait: AtomStore, pg_method: check_existing_source_urls;
    fn source_url_exists_sync(&self, url: &str) -> Result<bool, AtomicCoreError>
        => sqlite: source_url_exists_sync, pg_trait: AtomStore, pg_method: source_url_exists;
    fn get_atom_by_source_url_sync(&self, url: &str) -> Result<Option<AtomWithTags>, AtomicCoreError>
        => sqlite: get_atom_by_source_url_sync, pg_trait: AtomStore, pg_method: get_atom_by_source_url;
    fn count_pending_embeddings_sync(&self) -> Result<i32, AtomicCoreError>
        => sqlite: count_pending_embeddings_sync, pg_trait: AtomStore, pg_method: count_pending_embeddings;
    fn get_all_embedding_pairs_sync(&self) -> Result<Vec<(String, Vec<f32>)>, AtomicCoreError>
        => sqlite: get_all_embedding_pairs_sync, pg_trait: AtomStore, pg_method: get_all_embedding_pairs;
    fn get_top_k_canvas_edges_sync(&self, top_k: usize) -> Result<Vec<CanvasEdgeData>, AtomicCoreError>
        => sqlite: get_top_k_canvas_edges_sync, pg_trait: AtomStore, pg_method: get_top_k_canvas_edges;
    fn get_all_atom_tag_ids_sync(&self) -> Result<std::collections::HashMap<String, Vec<String>>, AtomicCoreError>
        => sqlite: get_all_atom_tag_ids_sync, pg_trait: AtomStore, pg_method: get_all_atom_tag_ids;
    fn get_canvas_atom_metadata_sync(&self) -> Result<Vec<CanvasAtomPosition>, AtomicCoreError>
        => sqlite: get_canvas_atom_metadata_sync, pg_trait: AtomStore, pg_method: get_canvas_atom_metadata;
    fn get_canvas_atom_metadata_light_sync(&self) -> Result<Vec<(String, String, Option<String>, i32, Option<String>)>, AtomicCoreError>
        => sqlite: get_canvas_atom_metadata_light_sync, pg_trait: AtomStore, pg_method: get_canvas_atom_metadata_light;

    // ---- TagStore ----
    fn get_all_tags_impl(&self) -> Result<Vec<TagWithCount>, AtomicCoreError>
        => sqlite: get_all_tags_impl, pg_trait: TagStore, pg_method: get_all_tags;
    fn get_all_tags_filtered_impl(&self, min_count: i32) -> Result<Vec<TagWithCount>, AtomicCoreError>
        => sqlite: get_all_tags_filtered_impl, pg_trait: TagStore, pg_method: get_all_tags_filtered;
    fn get_tag_children_impl(&self, parent_id: &str, min_count: i32, limit: i32, offset: i32) -> Result<PaginatedTagChildren, AtomicCoreError>
        => sqlite: get_tag_children_impl, pg_trait: TagStore, pg_method: get_tag_children;
    fn create_tag_impl(&self, name: &str, parent_id: Option<&str>) -> Result<Tag, AtomicCoreError>
        => sqlite: create_tag_impl, pg_trait: TagStore, pg_method: create_tag;
    fn update_tag_impl(&self, id: &str, name: &str, parent_id: Option<&str>) -> Result<Tag, AtomicCoreError>
        => sqlite: update_tag_impl, pg_trait: TagStore, pg_method: update_tag;
    fn delete_tag_impl(&self, id: &str, recursive: bool) -> Result<(), AtomicCoreError>
        => sqlite: delete_tag_impl, pg_trait: TagStore, pg_method: delete_tag;
    fn set_tag_autotag_target_impl(&self, id: &str, value: bool) -> Result<(), AtomicCoreError>
        => sqlite: set_tag_autotag_target_impl, pg_trait: TagStore, pg_method: set_tag_autotag_target;
    fn configure_autotag_targets_impl(&self, keep_default_names: &[String], add_custom_names: &[String]) -> Result<Vec<Tag>, AtomicCoreError>
        => sqlite: configure_autotag_targets_impl, pg_trait: TagStore, pg_method: configure_autotag_targets;
    fn get_related_tags_impl(&self, tag_id: &str, limit: usize) -> Result<Vec<RelatedTag>, AtomicCoreError>
        => sqlite: get_related_tags_impl, pg_trait: TagStore, pg_method: get_related_tags;
    fn get_tags_for_compaction_impl(&self) -> Result<String, AtomicCoreError>
        => sqlite: get_tags_for_compaction_impl, pg_trait: TagStore, pg_method: get_tags_for_compaction;
    fn apply_tag_merges_impl(&self, merges: &[TagMerge]) -> Result<CompactionResult, AtomicCoreError>
        => sqlite: apply_tag_merges_impl, pg_trait: TagStore, pg_method: apply_tag_merges;
    fn get_or_create_tag_impl(&self, name: &str, parent_name: Option<&str>) -> Result<String, AtomicCoreError>
        => sqlite: get_or_create_tag_impl, pg_trait: TagStore, pg_method: get_or_create_tag;
    fn link_tags_to_atom_impl(&self, atom_id: &str, tag_ids: &[String]) -> Result<(), AtomicCoreError>
        => sqlite: link_tags_to_atom_impl, pg_trait: TagStore, pg_method: link_tags_to_atom;
    fn get_tag_tree_for_llm_impl(&self) -> Result<String, AtomicCoreError>
        => sqlite: get_tag_tree_for_llm_impl, pg_trait: TagStore, pg_method: get_tag_tree_for_llm;
    fn compute_tag_centroids_batch_impl(&self, tag_ids: &[String]) -> Result<(), AtomicCoreError>
        => sqlite: compute_tag_centroids_batch_impl, pg_trait: TagStore, pg_method: compute_tag_centroids_batch;
    fn cleanup_orphaned_parents_impl(&self, tag_id: &str) -> Result<(), AtomicCoreError>
        => sqlite: cleanup_orphaned_parents_impl, pg_trait: TagStore, pg_method: cleanup_orphaned_parents;
    fn get_tag_hierarchy_impl(&self, tag_id: &str) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: get_tag_hierarchy_impl, pg_trait: TagStore, pg_method: get_tag_hierarchy;
    fn count_atoms_with_tags_impl(&self, tag_ids: &[String]) -> Result<i32, AtomicCoreError>
        => sqlite: count_atoms_with_tags_impl, pg_trait: TagStore, pg_method: count_atoms_with_tags;

    // ---- ChunkStore ----
    fn set_embedding_status_sync(&self, atom_id: &str, status: &str, error: Option<&str>) -> Result<(), AtomicCoreError>
        => sqlite: set_embedding_status_sync, pg_trait: ChunkStore, pg_method: set_embedding_status;
    fn set_embedding_status_batch_sync(&self, atom_ids: &[String], status: &str, error: Option<&str>) -> Result<(), AtomicCoreError>
        => sqlite: set_embedding_status_batch_sync, pg_trait: ChunkStore, pg_method: set_embedding_status_batch;
    fn set_tagging_status_sync(&self, atom_id: &str, status: &str, error: Option<&str>) -> Result<(), AtomicCoreError>
        => sqlite: set_tagging_status_sync, pg_trait: ChunkStore, pg_method: set_tagging_status;
    fn save_chunks_and_embeddings_sync(&self, atom_id: &str, chunks: &[(String, Vec<f32>)]) -> Result<(), AtomicCoreError>
        => sqlite: save_chunks_and_embeddings_sync, pg_trait: ChunkStore, pg_method: save_chunks_and_embeddings;
    fn save_chunks_and_embeddings_batch_sync(&self, atoms: &[(String, Vec<(String, Vec<f32>)>)]) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: save_chunks_and_embeddings_batch_sync, pg_trait: ChunkStore, pg_method: save_chunks_and_embeddings_batch;
    fn reset_stuck_processing_sync(&self) -> Result<i32, AtomicCoreError>
        => sqlite: reset_stuck_processing_sync, pg_trait: ChunkStore, pg_method: reset_stuck_processing;
    fn reset_failed_embeddings_sync(&self) -> Result<i32, AtomicCoreError>
        => sqlite: reset_failed_embeddings_sync, pg_trait: ChunkStore, pg_method: reset_failed_embeddings;
    fn rebuild_semantic_edges_sync(&self) -> Result<i32, AtomicCoreError>
        => sqlite: rebuild_semantic_edges_sync, pg_trait: ChunkStore, pg_method: rebuild_semantic_edges;
    fn get_semantic_edges_sync(&self, min_similarity: f32) -> Result<Vec<SemanticEdge>, AtomicCoreError>
        => sqlite: get_semantic_edges_sync, pg_trait: ChunkStore, pg_method: get_semantic_edges;
    fn get_semantic_edges_raw_sync(&self, min_similarity: f32) -> Result<Vec<(String, String, f32)>, AtomicCoreError>
        => sqlite: get_semantic_edges_raw_sync, pg_trait: ChunkStore, pg_method: get_semantic_edges_raw;
    fn get_atom_neighborhood_sync(&self, atom_id: &str, depth: i32, min_similarity: f32) -> Result<NeighborhoodGraph, AtomicCoreError>
        => sqlite: get_atom_neighborhood_sync, pg_trait: ChunkStore, pg_method: get_atom_neighborhood;
    fn get_connection_counts_sync(&self, min_similarity: f32) -> Result<HashMap<String, i32>, AtomicCoreError>
        => sqlite: get_connection_counts_sync, pg_trait: ChunkStore, pg_method: get_connection_counts;
    fn recompute_all_tag_embeddings_sync(&self) -> Result<i32, AtomicCoreError>
        => sqlite: recompute_all_tag_embeddings_sync, pg_trait: ChunkStore, pg_method: recompute_all_tag_embeddings;
    fn check_vector_extension_sync(&self) -> Result<String, AtomicCoreError>
        => sqlite: check_vector_extension_sync, pg_trait: ChunkStore, pg_method: check_vector_extension;
    fn claim_pending_embeddings_sync(&self, limit: i32) -> Result<Vec<(String, String)>, AtomicCoreError>
        => sqlite: claim_pending_embeddings_sync, pg_trait: ChunkStore, pg_method: claim_pending_embeddings;
    fn delete_chunks_batch_sync(&self, atom_ids: &[String]) -> Result<(), AtomicCoreError>
        => sqlite: delete_chunks_batch_sync, pg_trait: ChunkStore, pg_method: delete_chunks_batch;
    fn compute_semantic_edges_for_atom_sync(&self, atom_id: &str, threshold: f32, max_edges: i32) -> Result<i32, AtomicCoreError>
        => sqlite: compute_semantic_edges_for_atom_sync, pg_trait: ChunkStore, pg_method: compute_semantic_edges_for_atom;
    fn compute_semantic_edges_batch_sync(&self, atom_ids: &[String], threshold: f32, max_edges: i32) -> Result<i32, AtomicCoreError>
        => sqlite: compute_semantic_edges_batch_sync, pg_trait: ChunkStore, pg_method: compute_semantic_edges_batch;
    fn rebuild_fts_index_sync(&self) -> Result<(), AtomicCoreError>
        => sqlite: rebuild_fts_index_sync, pg_trait: ChunkStore, pg_method: rebuild_fts_index;
    fn claim_pending_tagging_sync(&self) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: claim_pending_tagging_sync, pg_trait: ChunkStore, pg_method: claim_pending_tagging;
    fn get_embedding_dimension_sync(&self) -> Result<Option<usize>, AtomicCoreError>
        => sqlite: get_embedding_dimension_sync, pg_trait: ChunkStore, pg_method: get_embedding_dimension;
    fn recreate_vector_index_sync(&self, dimension: usize) -> Result<(), AtomicCoreError>
        => sqlite: recreate_vector_index_sync, pg_trait: ChunkStore, pg_method: recreate_vector_index;
    fn claim_pending_reembedding_sync(&self) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: claim_pending_reembedding_sync, pg_trait: ChunkStore, pg_method: claim_pending_reembedding;
    fn claim_all_for_reembedding_sync(&self) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: claim_all_for_reembedding_sync, pg_trait: ChunkStore, pg_method: claim_all_for_reembedding;
    fn claim_pending_edges_sync(&self, limit: i32) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: claim_pending_edges_sync, pg_trait: ChunkStore, pg_method: claim_pending_edges;
    fn set_edges_status_batch_sync(&self, atom_ids: &[String], status: &str) -> Result<(), AtomicCoreError>
        => sqlite: set_edges_status_batch_sync, pg_trait: ChunkStore, pg_method: set_edges_status_batch;
    fn count_pending_edges_sync(&self) -> Result<i32, AtomicCoreError>
        => sqlite: count_pending_edges_sync, pg_trait: ChunkStore, pg_method: count_pending_edges;

    // ---- SearchStore ----
    fn vector_search_sync(&self, query_embedding: &[f32], limit: i32, threshold: f32, tag_id: Option<&str>, created_after: Option<&str>) -> Result<Vec<SemanticSearchResult>, AtomicCoreError>
        => sqlite: vector_search_sync, pg_trait: SearchStore, pg_method: vector_search;
    fn keyword_search_sync(&self, query: &str, limit: i32, tag_id: Option<&str>, created_after: Option<&str>) -> Result<Vec<SemanticSearchResult>, AtomicCoreError>
        => sqlite: keyword_search_sync, pg_trait: SearchStore, pg_method: keyword_search;
    fn find_similar_sync(&self, atom_id: &str, limit: i32, threshold: f32) -> Result<Vec<SimilarAtomResult>, AtomicCoreError>
        => sqlite: find_similar_sync, pg_trait: SearchStore, pg_method: find_similar;
    fn keyword_search_chunks_sync(&self, query: &str, limit: i32, scope_tag_ids: &[String], created_after: Option<&str>) -> Result<Vec<ChunkSearchResult>, AtomicCoreError>
        => sqlite: keyword_search_chunks_sync, pg_trait: SearchStore, pg_method: keyword_search_chunks;
    fn vector_search_chunks_sync(&self, query_embedding: &[f32], limit: i32, threshold: f32, scope_tag_ids: &[String], created_after: Option<&str>) -> Result<Vec<ChunkSearchResult>, AtomicCoreError>
        => sqlite: vector_search_chunks_sync, pg_trait: SearchStore, pg_method: vector_search_chunks;

    // ---- ChatStore ----
    fn create_conversation_sync(&self, tag_ids: &[String], title: Option<&str>) -> Result<ConversationWithTags, AtomicCoreError>
        => sqlite: create_conversation_sync, pg_trait: ChatStore, pg_method: create_conversation;
    fn get_conversations_sync(&self, filter_tag_id: Option<&str>, limit: i32, offset: i32) -> Result<Vec<ConversationWithTags>, AtomicCoreError>
        => sqlite: get_conversations_sync, pg_trait: ChatStore, pg_method: get_conversations;
    fn get_conversation_sync(&self, conversation_id: &str) -> Result<Option<ConversationWithMessages>, AtomicCoreError>
        => sqlite: get_conversation_sync, pg_trait: ChatStore, pg_method: get_conversation;
    fn update_conversation_sync(&self, id: &str, title: Option<&str>, is_archived: Option<bool>) -> Result<Conversation, AtomicCoreError>
        => sqlite: update_conversation_sync, pg_trait: ChatStore, pg_method: update_conversation;
    fn delete_conversation_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_conversation_sync, pg_trait: ChatStore, pg_method: delete_conversation;
    fn set_conversation_scope_sync(&self, conversation_id: &str, tag_ids: &[String]) -> Result<ConversationWithTags, AtomicCoreError>
        => sqlite: set_conversation_scope_sync, pg_trait: ChatStore, pg_method: set_conversation_scope;
    fn add_tag_to_scope_sync(&self, conversation_id: &str, tag_id: &str) -> Result<ConversationWithTags, AtomicCoreError>
        => sqlite: add_tag_to_scope_sync, pg_trait: ChatStore, pg_method: add_tag_to_scope;
    fn remove_tag_from_scope_sync(&self, conversation_id: &str, tag_id: &str) -> Result<ConversationWithTags, AtomicCoreError>
        => sqlite: remove_tag_from_scope_sync, pg_trait: ChatStore, pg_method: remove_tag_from_scope;
    fn save_message_sync(&self, conversation_id: &str, role: &str, content: &str) -> Result<ChatMessage, AtomicCoreError>
        => sqlite: save_message_sync, pg_trait: ChatStore, pg_method: save_message;
    fn save_tool_calls_sync(&self, message_id: &str, tool_calls: &[ChatToolCall]) -> Result<(), AtomicCoreError>
        => sqlite: save_tool_calls_sync, pg_trait: ChatStore, pg_method: save_tool_calls;
    fn save_citations_sync(&self, message_id: &str, citations: &[ChatCitation]) -> Result<(), AtomicCoreError>
        => sqlite: save_citations_sync, pg_trait: ChatStore, pg_method: save_citations;
    fn get_scope_tag_ids_sync(&self, conversation_id: &str) -> Result<Vec<String>, AtomicCoreError>
        => sqlite: get_scope_tag_ids_sync, pg_trait: ChatStore, pg_method: get_scope_tag_ids;
    fn get_scope_description_sync(&self, tag_ids: &[String]) -> Result<String, AtomicCoreError>
        => sqlite: get_scope_description_sync, pg_trait: ChatStore, pg_method: get_scope_description;

    // ---- WikiStore ----
    fn get_wiki_sync(&self, tag_id: &str) -> Result<Option<WikiArticleWithCitations>, AtomicCoreError>
        => sqlite: get_wiki_sync, pg_trait: WikiStore, pg_method: get_wiki;
    fn get_wiki_status_sync(&self, tag_id: &str) -> Result<WikiArticleStatus, AtomicCoreError>
        => sqlite: get_wiki_status_sync, pg_trait: WikiStore, pg_method: get_wiki_status;
    fn save_wiki_with_links_sync(&self, article: &WikiArticle, citations: &[WikiCitation], links: &[WikiLink]) -> Result<(), AtomicCoreError>
        => sqlite: save_wiki_with_links_sync, pg_trait: WikiStore, pg_method: save_wiki_with_links;
    fn delete_wiki_sync(&self, tag_id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_wiki_sync, pg_trait: WikiStore, pg_method: delete_wiki;
    fn get_wiki_links_sync(&self, tag_id: &str) -> Result<Vec<WikiLink>, AtomicCoreError>
        => sqlite: get_wiki_links_sync, pg_trait: WikiStore, pg_method: get_wiki_links;
    fn list_wiki_versions_sync(&self, tag_id: &str) -> Result<Vec<WikiVersionSummary>, AtomicCoreError>
        => sqlite: list_wiki_versions_sync, pg_trait: WikiStore, pg_method: list_wiki_versions;
    fn get_wiki_version_sync(&self, version_id: &str) -> Result<Option<WikiArticleVersion>, AtomicCoreError>
        => sqlite: get_wiki_version_sync, pg_trait: WikiStore, pg_method: get_wiki_version;
    fn get_all_wiki_articles_sync(&self) -> Result<Vec<WikiArticleSummary>, AtomicCoreError>
        => sqlite: get_all_wiki_articles_sync, pg_trait: WikiStore, pg_method: get_all_wiki_articles;
    fn get_suggested_wiki_articles_sync(&self, limit: i32) -> Result<Vec<SuggestedArticle>, AtomicCoreError>
        => sqlite: get_suggested_wiki_articles_sync, pg_trait: WikiStore, pg_method: get_suggested_wiki_articles;
    fn get_wiki_source_chunks_sync(&self, tag_id: &str, max_source_tokens: usize) -> Result<(Vec<ChunkWithContext>, i32), AtomicCoreError>
        => sqlite: get_wiki_source_chunks_sync, pg_trait: WikiStore, pg_method: get_wiki_source_chunks;
    fn get_wiki_update_chunks_sync(&self, tag_id: &str, last_update: &str, max_source_tokens: usize) -> Result<Option<(Vec<ChunkWithContext>, i32)>, AtomicCoreError>
        => sqlite: get_wiki_update_chunks_sync, pg_trait: WikiStore, pg_method: get_wiki_update_chunks;
    fn save_wiki_proposal_sync(&self, proposal: &WikiProposal) -> Result<(), AtomicCoreError>
        => sqlite: save_wiki_proposal_sync, pg_trait: WikiStore, pg_method: save_wiki_proposal;
    fn get_wiki_proposal_sync(&self, tag_id: &str) -> Result<Option<WikiProposal>, AtomicCoreError>
        => sqlite: get_wiki_proposal_sync, pg_trait: WikiStore, pg_method: get_wiki_proposal;
    fn delete_wiki_proposal_sync(&self, tag_id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_wiki_proposal_sync, pg_trait: WikiStore, pg_method: delete_wiki_proposal;

    // ---- BriefingStore ----
    fn list_new_atoms_since_sync(&self, since: &str, limit: i32) -> Result<Vec<AtomWithTags>, AtomicCoreError>
        => sqlite: list_new_atoms_since_sync, pg_trait: BriefingStore, pg_method: list_new_atoms_since;
    fn count_new_atoms_since_sync(&self, since: &str) -> Result<i32, AtomicCoreError>
        => sqlite: count_new_atoms_since_sync, pg_trait: BriefingStore, pg_method: count_new_atoms_since;
    fn insert_briefing_sync(&self, briefing: &crate::briefing::Briefing, citations: &[crate::briefing::BriefingCitation]) -> Result<crate::briefing::BriefingWithCitations, AtomicCoreError>
        => sqlite: insert_briefing_sync, pg_trait: BriefingStore, pg_method: insert_briefing;
    fn get_latest_briefing_sync(&self) -> Result<Option<crate::briefing::BriefingWithCitations>, AtomicCoreError>
        => sqlite: get_latest_briefing_sync, pg_trait: BriefingStore, pg_method: get_latest_briefing;
    fn get_briefing_sync(&self, id: &str) -> Result<Option<crate::briefing::BriefingWithCitations>, AtomicCoreError>
        => sqlite: get_briefing_sync, pg_trait: BriefingStore, pg_method: get_briefing;
    fn list_briefings_sync(&self, limit: i32) -> Result<Vec<crate::briefing::Briefing>, AtomicCoreError>
        => sqlite: list_briefings_sync, pg_trait: BriefingStore, pg_method: list_briefings;
    fn delete_briefing_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_briefing_sync, pg_trait: BriefingStore, pg_method: delete_briefing;

    // ---- FeedStore ----
    fn list_feeds_sync(&self) -> Result<Vec<Feed>, AtomicCoreError>
        => sqlite: list_feeds_sync, pg_trait: FeedStore, pg_method: list_feeds;
    fn get_feed_sync(&self, id: &str) -> Result<Feed, AtomicCoreError>
        => sqlite: get_feed_sync, pg_trait: FeedStore, pg_method: get_feed;
    fn create_feed_sync(&self, url: &str, title: Option<&str>, site_url: Option<&str>, poll_interval: i32, tag_ids: &[String]) -> Result<Feed, AtomicCoreError>
        => sqlite: create_feed_sync, pg_trait: FeedStore, pg_method: create_feed;
    fn update_feed_sync(&self, id: &str, title: Option<&str>, poll_interval: Option<i32>, is_paused: Option<bool>, tag_ids: Option<&[String]>) -> Result<Feed, AtomicCoreError>
        => sqlite: update_feed_sync, pg_trait: FeedStore, pg_method: update_feed;
    fn delete_feed_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_feed_sync, pg_trait: FeedStore, pg_method: delete_feed;
    fn get_due_feeds_sync(&self) -> Result<Vec<Feed>, AtomicCoreError>
        => sqlite: get_due_feeds_sync, pg_trait: FeedStore, pg_method: get_due_feeds;
    fn mark_feed_polled_sync(&self, id: &str, error: Option<&str>) -> Result<(), AtomicCoreError>
        => sqlite: mark_feed_polled_sync, pg_trait: FeedStore, pg_method: mark_feed_polled;
    fn claim_feed_item_sync(&self, feed_id: &str, guid: &str) -> Result<bool, AtomicCoreError>
        => sqlite: claim_feed_item_sync, pg_trait: FeedStore, pg_method: claim_feed_item;
    fn complete_feed_item_sync(&self, feed_id: &str, guid: &str, atom_id: &str) -> Result<(), AtomicCoreError>
        => sqlite: complete_feed_item_sync, pg_trait: FeedStore, pg_method: complete_feed_item;
    fn mark_feed_item_skipped_sync(&self, feed_id: &str, guid: &str, reason: &str) -> Result<(), AtomicCoreError>
        => sqlite: mark_feed_item_skipped_sync, pg_trait: FeedStore, pg_method: mark_feed_item_skipped;
    fn backfill_feed_metadata_sync(&self, id: &str, title: Option<&str>, site_url: Option<&str>) -> Result<(), AtomicCoreError>
        => sqlite: backfill_feed_metadata_sync, pg_trait: FeedStore, pg_method: backfill_feed_metadata;

    // ---- ClusterStore ----
    fn compute_clusters_sync(&self, min_similarity: f32, min_cluster_size: i32) -> Result<Vec<AtomCluster>, AtomicCoreError>
        => sqlite: compute_clusters_sync, pg_trait: ClusterStore, pg_method: compute_clusters;
    fn save_clusters_sync(&self, clusters: &[AtomCluster]) -> Result<(), AtomicCoreError>
        => sqlite: save_clusters_sync, pg_trait: ClusterStore, pg_method: save_clusters;
    fn get_clusters_sync(&self) -> Result<Vec<AtomCluster>, AtomicCoreError>
        => sqlite: get_clusters_sync, pg_trait: ClusterStore, pg_method: get_clusters;
    fn get_canvas_level_sync(&self, parent_id: Option<&str>, children_hint: Option<Vec<String>>) -> Result<CanvasLevel, AtomicCoreError>
        => sqlite: get_canvas_level_sync, pg_trait: ClusterStore, pg_method: get_canvas_level;
    fn enrich_clusters_with_tags_sync(&self, clusters: Vec<AtomCluster>) -> Result<Vec<AtomCluster>, AtomicCoreError>
        => sqlite: enrich_clusters_with_tags_sync, pg_trait: ClusterStore, pg_method: enrich_clusters_with_tags;

    // ---- DatabaseStore ----
    fn list_databases_sync(&self) -> Result<Vec<crate::registry::DatabaseInfo>, AtomicCoreError>
        => sqlite: list_databases_sync, pg_trait: DatabaseStore, pg_method: list_databases;
    fn create_database_sync(&self, name: &str) -> Result<crate::registry::DatabaseInfo, AtomicCoreError>
        => sqlite: create_database_sync, pg_trait: DatabaseStore, pg_method: create_database;
    fn rename_database_sync(&self, id: &str, name: &str) -> Result<(), AtomicCoreError>
        => sqlite: rename_database_sync, pg_trait: DatabaseStore, pg_method: rename_database;
    fn delete_database_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: delete_database_sync, pg_trait: DatabaseStore, pg_method: delete_database;
    fn get_default_database_id_sync(&self) -> Result<String, AtomicCoreError>
        => sqlite: get_default_database_id_sync, pg_trait: DatabaseStore, pg_method: get_default_database_id;
    fn set_default_database_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: set_default_database_sync, pg_trait: DatabaseStore, pg_method: set_default_database;
    fn purge_database_data_sync(&self, db_id: &str) -> Result<(), AtomicCoreError>
        => sqlite: purge_database_data_sync, pg_trait: DatabaseStore, pg_method: purge_database_data;

    // ---- SettingsStore ----
    fn get_all_settings_sync(&self) -> Result<HashMap<String, String>, AtomicCoreError>
        => sqlite: get_all_settings_sync, pg_trait: SettingsStore, pg_method: get_all_settings;
    fn get_setting_sync(&self, key: &str) -> Result<Option<String>, AtomicCoreError>
        => sqlite: get_setting_sync, pg_trait: SettingsStore, pg_method: get_setting;
    fn set_setting_sync(&self, key: &str, value: &str) -> Result<(), AtomicCoreError>
        => sqlite: set_setting_sync, pg_trait: SettingsStore, pg_method: set_setting;

    // ---- TokenStore ----
    fn create_api_token_sync(&self, name: &str) -> Result<(crate::tokens::ApiTokenInfo, String), AtomicCoreError>
        => sqlite: create_api_token_sync, pg_trait: TokenStore, pg_method: create_api_token;
    fn list_api_tokens_sync(&self) -> Result<Vec<crate::tokens::ApiTokenInfo>, AtomicCoreError>
        => sqlite: list_api_tokens_sync, pg_trait: TokenStore, pg_method: list_api_tokens;
    fn verify_api_token_sync(&self, raw_token: &str) -> Result<Option<crate::tokens::ApiTokenInfo>, AtomicCoreError>
        => sqlite: verify_api_token_sync, pg_trait: TokenStore, pg_method: verify_api_token;
    fn revoke_api_token_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: revoke_api_token_sync, pg_trait: TokenStore, pg_method: revoke_api_token;
    fn update_token_last_used_sync(&self, id: &str) -> Result<(), AtomicCoreError>
        => sqlite: update_token_last_used_sync, pg_trait: TokenStore, pg_method: update_token_last_used;
    fn migrate_legacy_token_sync(&self) -> Result<bool, AtomicCoreError>
        => sqlite: migrate_legacy_token_sync, pg_trait: TokenStore, pg_method: migrate_legacy_token;
    fn ensure_default_token_sync(&self) -> Result<Option<(crate::tokens::ApiTokenInfo, String)>, AtomicCoreError>
        => sqlite: ensure_default_token_sync, pg_trait: TokenStore, pg_method: ensure_default_token;
}
