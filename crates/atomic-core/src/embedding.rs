//! Embedding generation pipeline with callback-based events
//!
//! This module handles:
//! - Embedding generation via provider abstraction
//! - Tag extraction via LLM
//! - Semantic edge computation
//! - Callback-based event notification

use crate::chunking::chunk_content;
use crate::extraction::extract_tags_from_content;
use crate::providers::traits::EmbeddingConfig;
use crate::providers::{get_embedding_provider, get_model_capabilities, ProviderConfig, ProviderType};
use crate::storage::StorageBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// Events emitted during the embedding/tagging pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbeddingEvent {
    /// Embedding generation started for an atom
    Started { atom_id: String },
    /// Embedding generation completed successfully
    EmbeddingComplete { atom_id: String },
    /// Embedding generation failed
    EmbeddingFailed { atom_id: String, error: String },
    /// Tag extraction completed
    TaggingComplete {
        atom_id: String,
        tags_extracted: Vec<String>,
        new_tags_created: Vec<String>,
    },
    /// Tag extraction failed
    TaggingFailed { atom_id: String, error: String },
    /// Tag extraction was skipped (disabled or no API key)
    TaggingSkipped { atom_id: String },
    /// Progress update for batch embedding pipeline
    BatchProgress {
        batch_id: String,
        phase: String,
        completed: usize,
        total: usize,
    },
}

/// Generate embeddings via provider abstraction (batch support)
/// Uses ProviderConfig to determine which provider to use.
/// Includes retry with exponential backoff for transient failures.
pub async fn generate_embeddings_with_config(
    config: &ProviderConfig,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, EmbedError> {
    let _permit = crate::executor::EMBEDDING_SEMAPHORE
        .acquire()
        .await
        .expect("Embedding semaphore closed unexpectedly");

    let provider = get_embedding_provider(config).map_err(|e| EmbedError {
        message: e.to_string(),
        retryable: false,
        batch_reducible: false,
    })?;
    let embed_config = EmbeddingConfig::new(config.embedding_model());
    let model = config.embedding_model();
    let provider_type = format!("{:?}", config.provider_type);

    let mut last_error = String::new();
    let mut last_retryable = true;
    let mut last_batch_reducible = false;
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
        }

        match provider.embed_batch(texts, &embed_config).await {
            Ok(embeddings) => return Ok(embeddings),
            Err(e) => {
                last_error = e.to_string();
                last_retryable = e.is_retryable();
                last_batch_reducible = e.is_batch_reducible();
                if last_retryable {
                    tracing::warn!(
                        attempt = attempt + 1,
                        model = %model,
                        provider = %provider_type,
                        batch_size = texts.len(),
                        error = %last_error,
                        "Embedding attempt failed (retryable)"
                    );
                    continue;
                } else {
                    tracing::error!(
                        model = %model,
                        provider = %provider_type,
                        batch_size = texts.len(),
                        error = %last_error,
                        "Embedding failed (non-retryable)"
                    );
                    break;
                }
            }
        }
    }

    Err(EmbedError {
        message: last_error,
        retryable: last_retryable,
        batch_reducible: last_batch_reducible,
    })
}

/// Error from embedding generation with retryability info
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct EmbedError {
    pub message: String,
    pub retryable: bool,
    /// True when reducing batch size might resolve the error (e.g. 400 from
    /// providers that enforce smaller batch limits than our default).
    pub batch_reducible: bool,
}

/// Maximum texts per embedding API call for cross-atom batching.
/// At ~800 tokens/chunk this yields ~24k tokens per API call, which fits
/// within most providers' limits. The adaptive retry will split further
/// if a provider rejects the batch size.
const EMBEDDING_BATCH_SIZE: usize = 30;

/// Number of atoms to process per group in the embedding pipeline.
/// Bounds peak memory by limiting how much content, chunks, and embedding
/// results are held simultaneously.
const ATOM_GROUP_SIZE: usize = 200;

/// Metadata for a chunk awaiting embedding
#[derive(Clone)]
struct PendingChunk {
    atom_id: String,
    chunk_index: usize,
    content: String,
}

/// Input source for the embedding batch pipeline.
pub enum AtomInput {
    /// Content already loaded (e.g. from import or bulk create)
    Preloaded(Vec<(String, String)>),
    /// Only atom IDs — content will be loaded per-group from storage
    IdsOnly(Vec<String>),
}

/// Embed a list of chunks in adaptive batches.
/// Splits into batches of EMBEDDING_BATCH_SIZE, calls the API, and on failure
/// retries at half batch size recursively. Returns (embedded, failed: Vec<(atom_id, error)>).
async fn embed_chunks_batched(
    config: &ProviderConfig,
    chunks: Vec<PendingChunk>,
) -> (Vec<(PendingChunk, Vec<f32>)>, Vec<(String, String)>) {
    if chunks.is_empty() {
        return (vec![], vec![]);
    }

    let mut results: Vec<(PendingChunk, Vec<f32>)> = Vec::with_capacity(chunks.len());
    let mut failed_atoms: Vec<(String, String)> = Vec::new();

    // Split chunks into batches
    let batches: Vec<Vec<PendingChunk>> = chunks
        .into_iter()
        .collect::<Vec<_>>()
        .chunks(EMBEDDING_BATCH_SIZE)
        .map(|c| c.to_vec())
        .collect();

    let total_batches = batches.len();
    for (batch_idx, batch) in batches.into_iter().enumerate() {
        tracing::info!(
            batch = batch_idx + 1,
            total_batches,
            chunks = batch.len(),
            "Embedding batch"
        );
        let (mut successes, mut failures) = embed_batch_adaptive(config, batch).await;
        results.append(&mut successes);
        failed_atoms.append(&mut failures);
    }

    failed_atoms.sort_by(|a, b| a.0.cmp(&b.0));
    failed_atoms.dedup_by(|a, b| a.0 == b.0);
    (results, failed_atoms)
}

/// Try to embed a batch. On failure, split in half and retry each half.
/// Base case: single chunk failure returns the (atom_id, error) as failed.
fn embed_batch_adaptive(
    config: &ProviderConfig,
    batch: Vec<PendingChunk>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (Vec<(PendingChunk, Vec<f32>)>, Vec<(String, String)>)> + Send + '_>> {
    Box::pin(async move {
    if batch.is_empty() {
        return (vec![], vec![]);
    }

    let texts: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();

    match generate_embeddings_with_config(config, &texts).await {
        Ok(embeddings) => {
            let results: Vec<_> = batch
                .into_iter()
                .zip(embeddings.into_iter())
                .collect();
            (results, vec![])
        }
        Err(e) => {
            let can_split = batch.len() > 1 && (e.retryable || e.batch_reducible);
            if !can_split {
                // Unsplittable: single chunk, or non-retryable non-batch error
                if batch.len() > 1 {
                    tracing::error!(
                        batch_size = batch.len(),
                        error = %e.message,
                        "Non-retryable embedding error, failing entire batch"
                    );
                } else {
                    tracing::error!(
                        atom_id = %batch[0].atom_id,
                        chunk_index = batch[0].chunk_index,
                        error = %e.message,
                        "Single chunk embedding failed after retries"
                    );
                }
                let failed: Vec<_> = batch.iter()
                    .map(|c| (c.atom_id.clone(), e.message.clone()))
                    .collect();
                // dedup by atom_id
                let mut seen = std::collections::HashSet::new();
                let failed = failed.into_iter().filter(|(id, _)| seen.insert(id.clone())).collect();
                (vec![], failed)
            } else {
                // Split in half and retry each half
                let mid = batch.len() / 2;
                let (first_half, second_half): (Vec<_>, Vec<_>) = batch
                    .into_iter()
                    .enumerate()
                    .partition(|(i, _)| *i < mid);
                let first: Vec<PendingChunk> = first_half.into_iter().map(|(_, c)| c).collect();
                let second: Vec<PendingChunk> = second_half.into_iter().map(|(_, c)| c).collect();

                tracing::warn!(
                    original_size = mid * 2,
                    first_half = first.len(),
                    second_half = second.len(),
                    "Batch failed, retrying as 2 smaller batches"
                );

                let (mut r1, mut f1) = embed_batch_adaptive(config, first).await;
                let (mut r2, mut f2) = embed_batch_adaptive(config, second).await;
                r1.append(&mut r2);
                f1.append(&mut f2);
                (r1, f1)
            }
        }
    }
    })
}

/// Generate embeddings via OpenRouter API (batch support)
/// DEPRECATED: Use generate_embeddings_with_config instead
/// Kept for backward compatibility with existing code
pub async fn generate_openrouter_embeddings_public(
    _client: &reqwest::Client,
    api_key: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    use crate::providers::openrouter::OpenRouterProvider;
    use crate::providers::traits::EmbeddingProvider;

    let provider = OpenRouterProvider::new(api_key.to_string());
    let config = EmbeddingConfig::new("openai/text-embedding-3-small");

    provider
        .embed_batch(texts, &config)
        .await
        .map_err(|e| e.to_string())
}

/// Convert f32 vector to binary blob for sqlite-vec
pub fn f32_vec_to_blob_public(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Process ONLY embedding generation for an atom (no tag extraction)
/// This is the fast phase - just embedding API calls
///
/// Steps:
/// 1. Set embedding_status to 'processing'
/// 2. Delete existing chunks
/// 3. Chunk content
/// 4. Generate embeddings via provider
/// 5. Store chunks and embeddings
/// 6. Compute semantic edges
/// 7. Set embedding_status to 'complete'
pub async fn process_embedding_only(
    storage: &StorageBackend,
    atom_id: &str,
    content: &str,
) -> Result<(), String> {
    process_embedding_only_inner(storage, atom_id, content, false, None).await
}

/// Process embedding with externally-provided settings (from registry).
pub async fn process_embedding_only_with_settings(
    storage: &StorageBackend,
    atom_id: &str,
    content: &str,
    settings_map: HashMap<String, String>,
) -> Result<(), String> {
    process_embedding_only_inner(storage, atom_id, content, false, Some(settings_map)).await
}

/// Inner implementation with edge deferral control.
/// When `skip_edges` is true, semantic edge computation is deferred (for batch processing).
/// When `external_settings` is Some, uses those settings instead of reading from the data db.
async fn process_embedding_only_inner(
    storage: &StorageBackend,
    atom_id: &str,
    content: &str,
    skip_edges: bool,
    external_settings: Option<HashMap<String, String>>,
) -> Result<(), String> {
    // Set embedding status to processing
    storage.set_embedding_status_sync(atom_id, "processing", None)
        .map_err(|e| e.to_string())?;

    // Get settings for embeddings (from registry if provided, otherwise from data db)
    let settings_map = match external_settings {
        Some(ref s) => s.clone(),
        None => storage.get_all_settings_sync().map_err(|e| e.to_string())?,
    };
    let provider_config = ProviderConfig::from_settings(&settings_map);

    // Validate provider configuration
    if provider_config.provider_type == ProviderType::OpenRouter
        && provider_config.openrouter_api_key.is_none()
    {
        return Err(
            "OpenRouter API key not configured. Please set it in Settings.".to_string(),
        );
    }

    // Delete existing chunks for this atom (handles FTS, vec_chunks, and atom_chunks)
    storage.delete_chunks_batch_sync(&[atom_id.to_string()])
        .map_err(|e| e.to_string())?;

    // Chunk content
    let chunks = chunk_content(content);

    if chunks.is_empty() {
        // No chunks to process, mark embedding as complete, tagging as skipped
        storage.set_embedding_status_sync(atom_id, "complete", None)
            .map_err(|e| e.to_string())?;
        storage.set_tagging_status_sync(atom_id, "skipped", None)
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Use adaptive batching so provider batch-size limits (e.g. DashScope's
    // max 10) are handled by splitting, same as the bulk embedding path.
    let pending: Vec<PendingChunk> = chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| PendingChunk {
            atom_id: atom_id.to_string(),
            chunk_index: index,
            content: chunk,
        })
        .collect();

    let (embedded, failed) = embed_chunks_batched(&provider_config, pending).await;

    if !failed.is_empty() {
        let error = failed.into_iter().next().map(|(_, e)| e).unwrap_or_default();
        return Err(format!("Failed to generate embeddings: {}", error));
    }

    // Store chunks and embeddings
    let chunks_with_embeddings: Vec<(String, Vec<f32>)> = embedded
        .into_iter()
        .map(|(chunk, emb)| (chunk.content, emb))
        .collect();
    storage.save_chunks_and_embeddings_sync(atom_id, &chunks_with_embeddings)
        .map_err(|e| format!("Failed to store chunks: {}", e))?;

    // Compute semantic edges for this atom (unless deferred for batch)
    if !skip_edges {
        match storage.compute_semantic_edges_for_atom_sync(atom_id, 0.5, 15) {
            Ok(edge_count) => {
                if edge_count > 0 {
                    tracing::debug!(
                        edge_count,
                        atom_id,
                        "Created semantic edges for atom"
                    );
                }
                // Mark edges complete so this atom isn't reprocessed on startup
                storage.set_edges_status_batch_sync(&[atom_id.to_string()], "complete").ok();
            }
            Err(e) => {
                tracing::warn!(
                    atom_id,
                    error = %e,
                    "Failed to compute semantic edges for atom"
                );
            }
        }
    }

    // Recompute tag centroid embeddings for this atom's tags
    let tag_ids = storage.get_atom_tag_ids_impl(atom_id).unwrap_or_default();
    if !tag_ids.is_empty() {
        if let Err(e) = storage.compute_tag_centroids_batch_impl(&tag_ids) {
            tracing::warn!(atom_id, error = %e, "Failed to recompute tag embeddings for atom");
        }
    }

    // Set embedding status to complete
    storage.set_embedding_status_sync(atom_id, "complete", None)
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Process tag extraction for an atom using a single LLM call on raw content.
/// Reads directly from `atoms.content` — does NOT depend on embedding being complete.
///
/// Steps:
/// 1. Set tagging_status to 'processing'
/// 2. Check auto_tagging_enabled (skip if disabled)
/// 3. Read raw content from atoms table
/// 4. Extract tags via single LLM call on full content
/// 5. Link extracted tags to the atom
/// 6. Set tagging_status to 'complete'
pub async fn process_tagging_only(
    storage: &StorageBackend,
    atom_id: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    process_tagging_only_inner(storage, atom_id, None).await
}

/// Process tagging with externally-provided settings (from registry).
pub async fn process_tagging_only_with_settings(
    storage: &StorageBackend,
    atom_id: &str,
    settings_map: HashMap<String, String>,
) -> Result<(Vec<String>, Vec<String>), String> {
    process_tagging_only_inner(storage, atom_id, Some(settings_map)).await
}

async fn process_tagging_only_inner(
    storage: &StorageBackend,
    atom_id: &str,
    external_settings: Option<HashMap<String, String>>,
) -> Result<(Vec<String>, Vec<String>), String> {
    // Respect atoms that were intentionally marked 'skipped' (e.g. by a
    // dimension-change reset that preserves existing tags) or already
    // 'complete'. Only 'pending'/'failed' atoms should actually run tagging.
    let current_status = storage
        .get_tagging_status_impl(atom_id)
        .map_err(|e| e.to_string())?;
    if current_status == "skipped" || current_status == "complete" {
        return Ok((Vec::new(), Vec::new()));
    }

    // Set tagging status to processing
    storage.set_tagging_status_sync(atom_id, "processing", None)
        .map_err(|e| e.to_string())?;

    // Get settings (from registry if provided, otherwise from data db)
    let settings_map = match external_settings {
        Some(ref s) => s.clone(),
        None => storage.get_all_settings_sync().map_err(|e| e.to_string())?,
    };
    let auto_tagging_enabled = settings_map
        .get("auto_tagging_enabled")
        .map(|v| v == "true")
        .unwrap_or(true);

    if !auto_tagging_enabled {
        storage.set_tagging_status_sync(atom_id, "skipped", None)
            .map_err(|e| e.to_string())?;
        return Ok((Vec::new(), Vec::new()));
    }

    let provider_config = ProviderConfig::from_settings(&settings_map);

    // Validate provider for LLM
    if provider_config.provider_type == ProviderType::OpenRouter
        && provider_config.openrouter_api_key.is_none()
    {
        storage.set_tagging_status_sync(atom_id, "skipped", None)
            .map_err(|e| e.to_string())?;
        return Ok((Vec::new(), Vec::new()));
    }

    let tagging_model = provider_config.llm_model().to_string();

    // Read raw content directly from atoms table — no dependency on embedding
    let content = storage.get_atom_content_impl(atom_id)
        .map_err(|e| format!("Failed to get atom content: {}", e))?
        .ok_or_else(|| format!("Atom not found: {}", atom_id))?;

    if content.trim().is_empty() {
        storage.set_tagging_status_sync(atom_id, "skipped", None)
            .map_err(|e| e.to_string())?;
        return Ok((Vec::new(), Vec::new()));
    }

    // Load model capabilities (uses in-memory + DB cache to avoid redundant fetches)
    let supported_params: Option<Vec<String>> =
        if provider_config.provider_type == ProviderType::OpenRouter {
            // Try to load capabilities from the settings cache
            let cached_json = storage.get_setting_sync("model_capabilities_cache").ok().flatten();
            let capabilities = if let Some(json) = cached_json {
                serde_json::from_str::<crate::providers::models::ModelCapabilitiesCache>(&json).ok()
            } else {
                None
            };

            capabilities.and_then(|caps| caps.get_supported_params(&tagging_model).cloned())
        } else {
            None
        };

    // Get tag tree for LLM context
    let tag_tree_json = storage.get_tag_tree_for_llm_impl()
        .map_err(|e| e.to_string())?;

    // Single LLM call on full content — no per-chunk loop, no consolidation
    let tags = extract_tags_from_content(
        &provider_config,
        &content,
        &tag_tree_json,
        &tagging_model,
        supported_params,
    )
    .await?;

    let mut all_tag_ids = Vec::new();

    for tag_application in tags {
        let trimmed_name = tag_application.name.trim();
        if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
            continue;
        }

        match storage.get_or_create_tag_impl(&tag_application.name, tag_application.parent_name.as_deref()) {
            Ok(tag_id) => all_tag_ids.push(tag_id),
            Err(e) => tracing::error!(tag_name = %tag_application.name, error = %e, "Failed to get/create tag"),
        }
    }

    if !all_tag_ids.is_empty() {
        storage.link_tags_to_atom_impl(atom_id, &all_tag_ids)
            .map_err(|e| e.to_string())?;
    }

    // Set tagging status to complete
    storage.set_tagging_status_sync(atom_id, "complete", None)
        .map_err(|e| e.to_string())?;

    all_tag_ids.sort();
    all_tag_ids.dedup();
    let all_new_tag_ids = all_tag_ids.clone();

    Ok((all_tag_ids, all_new_tag_ids))
}

/// Process tagging for multiple atoms concurrently with semaphore-based limiting
/// Used by process_pending_tagging for bulk operations
pub async fn process_tagging_batch<F>(storage: StorageBackend, atom_ids: Vec<String>, on_event: F)
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    process_tagging_batch_inner(storage, atom_ids, on_event, None).await
}

/// Process tagging batch with externally-provided settings (from registry).
pub async fn process_tagging_batch_with_settings<F>(
    storage: StorageBackend,
    atom_ids: Vec<String>,
    on_event: F,
    settings_map: HashMap<String, String>,
)
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    process_tagging_batch_inner(storage, atom_ids, on_event, Some(settings_map)).await
}

async fn process_tagging_batch_inner<F>(
    storage: StorageBackend,
    atom_ids: Vec<String>,
    on_event: F,
    external_settings: Option<HashMap<String, String>>,
)
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    let total = atom_ids.len();
    let emit_progress = total > 1;
    let batch_id = Uuid::new_v4().to_string();
    let counter = Arc::new(AtomicUsize::new(0));

    if emit_progress {
        on_event(EmbeddingEvent::BatchProgress {
            batch_id: batch_id.clone(),
            phase: "tagging".to_string(),
            completed: 0,
            total,
        });
    }

    let mut tasks = Vec::with_capacity(total);

    for atom_id in atom_ids {
        let storage = storage.clone();
        let on_event = on_event.clone();
        let settings = external_settings.clone();
        let counter = counter.clone();
        let batch_id = batch_id.clone();

        let task = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = crate::executor::LLM_SEMAPHORE
                .acquire()
                .await
                .expect("Semaphore closed unexpectedly");

            let result = match settings {
                Some(s) => process_tagging_only_with_settings(&storage, &atom_id, s).await,
                None => process_tagging_only(&storage, &atom_id).await,
            };

            let event = match result {
                Ok((tags_extracted, new_tags_created)) => EmbeddingEvent::TaggingComplete {
                    atom_id: atom_id.clone(),
                    tags_extracted,
                    new_tags_created,
                },
                Err(e) => {
                    storage.set_tagging_status_sync(&atom_id, "failed", Some(&e)).ok();
                    EmbeddingEvent::TaggingFailed {
                        atom_id: atom_id.clone(),
                        error: e,
                    }
                }
            };

            on_event(event);

            if emit_progress {
                let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
                if done % 5 == 0 || done == total {
                    on_event(EmbeddingEvent::BatchProgress {
                        batch_id: batch_id.clone(),
                        phase: "tagging".to_string(),
                        completed: done,
                        total,
                    });
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    if emit_progress {
        on_event(EmbeddingEvent::BatchProgress {
            batch_id,
            phase: "complete".to_string(),
            completed: total,
            total,
        });
    }
}

/// Process embeddings and tagging for a SINGLE atom (used by create_atom/update_atom)
/// Spawns a background task that runs embedding and tagging concurrently.
/// Tagging reads raw content from the atoms table, so it does not depend on embedding.
pub fn spawn_embedding_task_single<F>(
    storage: StorageBackend,
    atom_id: String,
    content: String,
    on_event: F,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + 'static,
{
    spawn_embedding_task_single_with_settings(storage, atom_id, content, on_event, None);
}

/// Like `spawn_embedding_task_single` but with externally-provided settings (from registry).
pub fn spawn_embedding_task_single_with_settings<F>(
    storage: StorageBackend,
    atom_id: String,
    content: String,
    on_event: F,
    settings_map: Option<HashMap<String, String>>,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + 'static,
{
    let on_event = Arc::new(on_event);
    crate::executor::spawn(async move {
        // Emit started event
        on_event(EmbeddingEvent::Started {
            atom_id: atom_id.clone(),
        });

        // Run embedding and tagging concurrently — they're independent
        let storage_embed = storage.clone();
        let storage_tag = storage.clone();
        let atom_id_embed = atom_id.clone();
        let atom_id_tag = atom_id.clone();
        let content_embed = content.clone();
        let on_event_embed = Arc::clone(&on_event);
        let on_event_tag = Arc::clone(&on_event);
        let settings_embed = settings_map.clone();
        let settings_tag = settings_map;

        let embed_handle = tokio::spawn(async move {
            let result = match settings_embed {
                Some(s) => process_embedding_only_with_settings(&storage_embed, &atom_id_embed, &content_embed, s).await,
                None => process_embedding_only(&storage_embed, &atom_id_embed, &content_embed).await,
            };
            match &result {
                Ok(()) => {
                    on_event_embed(EmbeddingEvent::EmbeddingComplete {
                        atom_id: atom_id_embed.clone(),
                    });
                }
                Err(e) => {
                    storage_embed.set_embedding_status_sync(&atom_id_embed, "failed", Some(e)).ok();
                    on_event_embed(EmbeddingEvent::EmbeddingFailed {
                        atom_id: atom_id_embed.clone(),
                        error: e.clone(),
                    });
                }
            }
        });

        let tag_handle = tokio::spawn(async move {
            let result = match settings_tag {
                Some(s) => process_tagging_only_with_settings(&storage_tag, &atom_id_tag, s).await,
                None => process_tagging_only(&storage_tag, &atom_id_tag).await,
            };
            match result {
                Ok((tags_extracted, new_tags_created)) => {
                    on_event_tag(EmbeddingEvent::TaggingComplete {
                        atom_id: atom_id_tag.clone(),
                        tags_extracted,
                        new_tags_created,
                    });
                }
                Err(e) => {
                    storage_tag.set_tagging_status_sync(&atom_id_tag, "failed", Some(&e)).ok();
                    on_event_tag(EmbeddingEvent::TaggingFailed {
                        atom_id: atom_id_tag.clone(),
                        error: e,
                    });
                }
            }
        });

        let _ = tokio::join!(embed_handle, tag_handle);
    });
}

/// Process embeddings and tagging for multiple atoms concurrently.
/// Uses cross-atom batching for embedding API calls (reducing 10K calls to ~200).
/// Tagging runs per-atom concurrently via semaphores.
/// Set skip_tagging=true when re-embedding due to model/provider change (tags are preserved).
pub async fn process_embedding_batch<F>(
    storage: StorageBackend,
    input: AtomInput,
    skip_tagging: bool,
    on_event: F,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    process_embedding_batch_inner(storage, input, skip_tagging, on_event, None).await
}

/// Process embedding batch with externally-provided settings (from registry).
pub async fn process_embedding_batch_with_settings<F>(
    storage: StorageBackend,
    input: AtomInput,
    skip_tagging: bool,
    on_event: F,
    settings_map: HashMap<String, String>,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    process_embedding_batch_inner(storage, input, skip_tagging, on_event, Some(settings_map)).await
}

async fn process_embedding_batch_inner<F>(
    storage: StorageBackend,
    input: AtomInput,
    skip_tagging: bool,
    on_event: F,
    external_settings: Option<HashMap<String, String>>,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    // Extract all atom IDs and optional preloaded content
    let (all_atom_ids, preloaded_content): (Vec<String>, Option<Vec<(String, String)>>) = match input {
        AtomInput::Preloaded(atoms) => {
            let ids = atoms.iter().map(|(id, _)| id.clone()).collect();
            (ids, Some(atoms))
        }
        AtomInput::IdsOnly(ids) => (ids, None),
    };

    let total_count = all_atom_ids.len();
    if total_count == 0 {
        return;
    }

    let batch_id = Uuid::new_v4().to_string();

    // Only emit batch progress for bulk operations (>1 atom)
    let emit_progress = total_count > 1;
    if emit_progress {
        on_event(EmbeddingEvent::BatchProgress {
            batch_id: batch_id.clone(),
            phase: "chunking".to_string(),
            completed: 0,
            total: total_count,
        });
    }

    tracing::info!(
        total_count,
        "Starting pipeline for atoms (grouped processing, group_size={})",
        ATOM_GROUP_SIZE,
    );

    // === Get settings ===
    let provider_config = {
        let settings_map = match external_settings {
            Some(ref s) => s.clone(),
            None => {
                match storage.get_all_settings_sync() {
                    Ok(s) => s,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to get settings");
                        return;
                    }
                }
            }
        };
        let provider_config = ProviderConfig::from_settings(&settings_map);

        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            tracing::warn!("OpenRouter API key not configured, skipping embedding");
            return;
        }

        provider_config
    };

    // === Clean up old chunks for all atoms ===
    if let Err(e) = storage.delete_chunks_batch_sync(&all_atom_ids) {
        tracing::error!(error = %e, "Failed to clean up old chunks");
        return;
    }
    tracing::info!(total_count, "DB cleanup complete for atoms");

    // === Shared tagging state (fire-and-forget with atomic counter) ===
    let tagging_counter = Arc::new(AtomicUsize::new(0));
    let tagging_remaining = Arc::new(AtomicUsize::new(0));
    let tagging_done_notify = Arc::new(tokio::sync::Notify::new());

    // === Process atoms in bounded groups ===
    let mut completed_atom_ids: Vec<String> = Vec::new();
    let mut atoms_processed = 0usize;

    // Build an iterator of (id, content) pairs per group.
    // For Preloaded: chunk the Vec directly.
    // For IdsOnly: load content from DB per group.
    let num_groups = (total_count + ATOM_GROUP_SIZE - 1) / ATOM_GROUP_SIZE;

    // Consume preloaded content into an owned iterator we can drain per-group
    let mut preloaded_iter = preloaded_content.map(|v| v.into_iter());

    for group_idx in 0..num_groups {
        let group_start = group_idx * ATOM_GROUP_SIZE;
        let group_end = (group_start + ATOM_GROUP_SIZE).min(total_count);
        let group_size = group_end - group_start;

        // Get (id, content) pairs for this group
        let group_atoms: Vec<(String, String)> = if let Some(ref mut iter) = preloaded_iter {
            // Preloaded: take the next group_size items (drains from the Vec)
            iter.by_ref().take(group_size).collect()
        } else {
            // IdsOnly: load content from DB for this group
            let group_ids = &all_atom_ids[group_start..group_end];
            match storage.get_atom_contents_batch_impl(group_ids) {
                Ok(pairs) => pairs,
                Err(e) => {
                    tracing::error!(error = %e, group = group_idx + 1, "Failed to load atom content for group");
                    // Mark all atoms in this group as failed
                    for id in group_ids {
                        storage.set_embedding_status_sync(id, "failed", Some(&e.to_string())).ok();
                        on_event(EmbeddingEvent::EmbeddingFailed {
                            atom_id: id.clone(),
                            error: format!("Failed to load content: {}", e),
                        });
                    }
                    atoms_processed += group_size;
                    continue;
                }
            }
        };

        tracing::info!(
            group = group_idx + 1,
            num_groups,
            group_atoms = group_atoms.len(),
            "Processing atom group"
        );

        // --- Chunk this group ---
        let mut group_chunks: Vec<PendingChunk> = Vec::new();
        for (atom_id, content) in &group_atoms {
            let chunks = chunk_content(content);
            if chunks.is_empty() {
                storage.set_embedding_status_sync(atom_id, "complete", None).ok();
                storage.set_tagging_status_sync(atom_id, "skipped", None).ok();
                on_event(EmbeddingEvent::EmbeddingComplete {
                    atom_id: atom_id.clone(),
                });
            } else {
                for (index, chunk) in chunks.into_iter().enumerate() {
                    group_chunks.push(PendingChunk {
                        atom_id: atom_id.clone(),
                        chunk_index: index,
                        content: chunk,
                    });
                }
            }
        }

        if emit_progress {
            atoms_processed += group_size;
            on_event(EmbeddingEvent::BatchProgress {
                batch_id: batch_id.clone(),
                phase: "embedding".to_string(),
                completed: atoms_processed,
                total: total_count,
            });
        }

        // --- Embed this group's chunks ---
        if !group_chunks.is_empty() {
            let (embedded_chunks, failed_atoms) =
                embed_chunks_batched(&provider_config, group_chunks).await;

            // --- Store results in a single transaction ---
            // Group by atom_id, consuming the embedded results
            let mut by_atom: HashMap<String, Vec<(String, Vec<f32>)>> = HashMap::new();
            for (chunk, embedding) in embedded_chunks {
                by_atom
                    .entry(chunk.atom_id)
                    .or_default()
                    .push((chunk.content, embedding));
            }

            // Batch save: one lock acquire, one transaction, one fsync
            let atoms_vec: Vec<(String, Vec<(String, Vec<f32>)>)> = by_atom.into_iter().collect();
            match storage.save_chunks_and_embeddings_batch_sync(&atoms_vec) {
                Ok(succeeded) => {
                    // Batch-set status for all succeeded atoms
                    storage.set_embedding_status_batch_sync(&succeeded, "complete", None).ok();
                    for atom_id in &succeeded {
                        on_event(EmbeddingEvent::EmbeddingComplete {
                            atom_id: atom_id.clone(),
                        });
                    }
                    // Track atoms that saved OK but whose chunks failed
                    let succeeded_set: std::collections::HashSet<&String> = succeeded.iter().collect();
                    let mut db_failed: Vec<String> = Vec::new();
                    for (atom_id, _) in &atoms_vec {
                        if !succeeded_set.contains(atom_id) {
                            db_failed.push(atom_id.clone());
                        }
                    }
                    if !db_failed.is_empty() {
                        storage.set_embedding_status_batch_sync(&db_failed, "failed", Some("Failed to store embeddings in DB")).ok();
                        for atom_id in &db_failed {
                            on_event(EmbeddingEvent::EmbeddingFailed {
                                atom_id: atom_id.clone(),
                                error: "Failed to store embeddings in DB".to_string(),
                            });
                        }
                    }
                    completed_atom_ids.extend(succeeded);
                }
                Err(e) => {
                    // Entire batch transaction failed — fall back to per-atom
                    tracing::warn!(error = %e, "Batch save failed, falling back to per-atom");
                    for (atom_id, chunks_with_embeddings) in &atoms_vec {
                        match storage.save_chunks_and_embeddings_sync(atom_id, chunks_with_embeddings) {
                            Ok(()) => {
                                storage.set_embedding_status_sync(atom_id, "complete", None).ok();
                                completed_atom_ids.push(atom_id.clone());
                                on_event(EmbeddingEvent::EmbeddingComplete {
                                    atom_id: atom_id.clone(),
                                });
                            }
                            Err(_) => {
                                storage.set_embedding_status_sync(atom_id, "failed", Some("Failed to store embeddings in DB")).ok();
                                on_event(EmbeddingEvent::EmbeddingFailed {
                                    atom_id: atom_id.clone(),
                                    error: "Failed to store embeddings in DB".to_string(),
                                });
                            }
                        }
                    }
                }
            }

            // Mark atoms that failed embedding API calls
            if !failed_atoms.is_empty() {
                let failed_ids: Vec<String> = failed_atoms.iter().map(|(id, _)| id.clone()).collect();
                // Each atom may have a different error, but for batch status we use a generic message
                // and emit per-atom events with the specific error
                storage.set_embedding_status_batch_sync(&failed_ids, "failed", Some("Embedding API error")).ok();
                for (atom_id, error) in &failed_atoms {
                    on_event(EmbeddingEvent::EmbeddingFailed {
                        atom_id: atom_id.clone(),
                        error: error.clone(),
                    });
                }
            }

            if emit_progress {
                on_event(EmbeddingEvent::BatchProgress {
                    batch_id: batch_id.clone(),
                    phase: "storing".to_string(),
                    completed: completed_atom_ids.len(),
                    total: total_count,
                });
            }
        }

        // --- Spawn tagging tasks for this group (fire-and-forget) ---
        if !skip_tagging {
            tagging_remaining.fetch_add(group_atoms.len(), Ordering::Relaxed);
            for (atom_id, _) in group_atoms {
                let storage = storage.clone();
                let on_event = on_event.clone();
                let settings = external_settings.clone();
                let counter = tagging_counter.clone();
                let remaining = tagging_remaining.clone();
                let notify = tagging_done_notify.clone();
                let batch_id = batch_id.clone();
                let should_emit = emit_progress;
                let tagging_total = total_count;

                tokio::spawn(async move {
                    let _permit = crate::executor::LLM_SEMAPHORE
                        .acquire()
                        .await
                        .expect("Semaphore closed unexpectedly");

                    let result = match settings {
                        Some(s) => process_tagging_only_with_settings(&storage, &atom_id, s).await,
                        None => process_tagging_only(&storage, &atom_id).await,
                    };

                    let event = match result {
                        Ok((tags_extracted, new_tags_created)) => EmbeddingEvent::TaggingComplete {
                            atom_id: atom_id.clone(),
                            tags_extracted,
                            new_tags_created,
                        },
                        Err(e) => {
                            storage.set_tagging_status_sync(&atom_id, "failed", Some(&e)).ok();
                            EmbeddingEvent::TaggingFailed {
                                atom_id: atom_id.clone(),
                                error: e,
                            }
                        }
                    };

                    on_event(event);

                    // Emit tagging progress every 5 atoms
                    if should_emit {
                        let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
                        if done % 5 == 0 || done == tagging_total {
                            on_event(EmbeddingEvent::BatchProgress {
                                batch_id: batch_id.clone(),
                                phase: "tagging".to_string(),
                                completed: done,
                                total: tagging_total,
                            });
                        }
                    }

                    // Signal completion
                    if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
                        notify.notify_one();
                    }
                });
            }
        }

        // group_atoms, group_chunks, embedded_chunks, by_atom are all dropped here
    }

    tracing::info!(
        succeeded = completed_atom_ids.len(),
        total = total_count,
        "All groups processed, embeddings stored"
    );

    // Rebuild FTS index once after all groups
    storage.rebuild_fts_index_sync().ok();

    // === Mark atoms for edge computation ===
    // Edge computation now runs as a separate batched pipeline (process_pending_edges)
    // that checkpoints progress, so it can survive restarts.
    if !completed_atom_ids.is_empty() {
        if let Err(e) = storage.set_edges_status_batch_sync(&completed_atom_ids, "pending") {
            tracing::warn!(error = %e, "Failed to mark atoms for edge computation");
        }
    }

    // === Recompute tag centroid embeddings for affected tags ===
    if !completed_atom_ids.is_empty() {
        let affected_tag_ids = storage.get_tag_ids_for_atoms_batch_impl(&completed_atom_ids)
            .unwrap_or_default();

        if !affected_tag_ids.is_empty() {
            tracing::info!(count = affected_tag_ids.len(), "Recomputing centroid embeddings for tags");
            if let Err(e) = storage.compute_tag_centroids_batch_impl(&affected_tag_ids) {
                tracing::warn!(error = %e, "Failed to recompute tag embeddings");
            }
        }
    }

    if emit_progress {
        on_event(EmbeddingEvent::BatchProgress {
            batch_id: batch_id.clone(),
            phase: "finalizing".to_string(),
            completed: total_count,
            total: total_count,
        });
    }

    // === Wait for tagging to complete ===
    while !skip_tagging && tagging_remaining.load(Ordering::Acquire) > 0 {
        tagging_done_notify.notified().await;
    }

    if emit_progress {
        on_event(EmbeddingEvent::BatchProgress {
            batch_id: batch_id.clone(),
            phase: "complete".to_string(),
            completed: total_count,
            total: total_count,
        });
    }

    // === Kick off edge computation in the background ===
    if !completed_atom_ids.is_empty() {
        match process_pending_edges(storage) {
            Ok(count) if count > 0 => tracing::info!(count, "Started background edge computation"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "Failed to start edge computation"),
        }
    }

    if skip_tagging {
        tracing::info!("Pipeline complete. Tagging was skipped (re-embedding only).");
    } else {
        tracing::info!("Pipeline complete. All embedding and tagging tasks finished.");
    }
}

/// Convert L2 distance to cosine similarity for normalized vectors
/// Formula: cosine_similarity = 1 - (L2_distance² / 2)
/// This derives from: L2² = 2(1 - cos(θ)) for unit vectors
pub fn distance_to_similarity(distance: f32) -> f32 {
    (1.0 - (distance * distance / 2.0)).clamp(-1.0, 1.0)
}

/// Compute semantic edges for an atom after embedding generation
/// Finds similar atoms based on vector similarity and stores edges in semantic_edges table
pub fn compute_semantic_edges_for_atom(
    conn: &rusqlite::Connection,
    atom_id: &str,
    threshold: f32, // Default: 0.5 - lower than UI threshold to capture more relationships
    max_edges: i32, // Default: 15 per atom
) -> Result<i32, String> {
    use std::collections::HashMap;

    // First, delete existing edges for this atom (bidirectional)
    conn.execute(
        "DELETE FROM semantic_edges WHERE source_atom_id = ?1 OR target_atom_id = ?1",
        [atom_id],
    )
    .map_err(|e| format!("Failed to delete existing edges: {}", e))?;

    // Get all chunks for the given atom
    let mut stmt = conn
        .prepare("SELECT id, chunk_index, embedding FROM atom_chunks WHERE atom_id = ?1")
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;

    let source_chunks: Vec<(String, i32, Vec<u8>)> = stmt
        .query_map([atom_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| format!("Failed to query chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect chunks: {}", e))?;

    if source_chunks.is_empty() {
        return Ok(0);
    }

    // Map to store best similarity per target atom_id
    // Value: (similarity, source_chunk_index, target_chunk_index)
    let mut atom_similarities: HashMap<String, (f32, i32, i32)> = HashMap::new();

    // For each source chunk, find similar chunks
    for (_, source_chunk_index, embedding_blob) in &source_chunks {
        // Query vec_chunks for similar chunks
        let mut vec_stmt = conn
            .prepare(
                "SELECT chunk_id, distance
                 FROM vec_chunks
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare vec query: {}", e))?;

        let similar_chunks: Vec<(String, f32)> = vec_stmt
            .query_map(rusqlite::params![embedding_blob, max_edges * 5], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query similar chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

        // Filter by threshold, then batch-fetch chunk info
        let filtered: Vec<(String, f32)> = similar_chunks
            .into_iter()
            .filter(|(_, distance)| distance_to_similarity(*distance) >= threshold)
            .collect();

        if filtered.is_empty() {
            continue;
        }

        let chunk_ids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();
        let placeholders = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let info_query = format!(
            "SELECT id, atom_id, chunk_index FROM atom_chunks WHERE id IN ({})",
            placeholders
        );
        let mut info_stmt = conn.prepare(&info_query)
            .map_err(|e| format!("Failed to prepare chunk info query: {}", e))?;
        let chunk_info_map: HashMap<String, (String, i32)> = info_stmt
            .query_map(rusqlite::params_from_iter(chunk_ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i32>(2)?))
            })
            .map_err(|e| format!("Failed to query chunk info: {}", e))?
            .filter_map(|r| r.ok())
            .map(|(id, atom_id, idx)| (id, (atom_id, idx)))
            .collect();

        for (chunk_id, distance) in filtered {
            let similarity = distance_to_similarity(distance);

            if let Some((target_atom_id, target_chunk_index)) = chunk_info_map.get(&chunk_id) {
                if target_atom_id == atom_id {
                    continue;
                }

                let entry = atom_similarities.entry(target_atom_id.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, *source_chunk_index, *target_chunk_index));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((similarity, *source_chunk_index, *target_chunk_index));
                    }
                }
            }
        }
    }

    // Sort by similarity and take top N
    let mut edges: Vec<(String, f32, i32, i32)> = atom_similarities
        .into_iter()
        .map(|(target_id, (sim, src_idx, tgt_idx))| (target_id, sim, src_idx, tgt_idx))
        .collect();

    edges.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    edges.truncate(max_edges as usize);

    // Insert edges (store bidirectionally with consistent ordering)
    let now = chrono::Utc::now().to_rfc3339();
    let mut edges_created = 0;

    for (target_atom_id, similarity, source_chunk_index, target_chunk_index) in edges {
        // Use consistent ordering: smaller ID is source
        let (src_id, tgt_id, src_chunk, tgt_chunk) = if atom_id < target_atom_id.as_str() {
            (
                atom_id.to_string(),
                target_atom_id.clone(),
                source_chunk_index,
                target_chunk_index,
            )
        } else {
            (
                target_atom_id.clone(),
                atom_id.to_string(),
                target_chunk_index,
                source_chunk_index,
            )
        };

        let edge_id = Uuid::new_v4().to_string();

        // Insert or update (using INSERT OR REPLACE due to UNIQUE constraint)
        let result = conn.execute(
            "INSERT OR REPLACE INTO semantic_edges
             (id, source_atom_id, target_atom_id, similarity_score, source_chunk_index, target_chunk_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![&edge_id, &src_id, &tgt_id, similarity, src_chunk, tgt_chunk, &now,],
        );

        if result.is_ok() {
            edges_created += 1;
        }
    }

    Ok(edges_created)
}

/// Process all atoms with 'pending' embedding status
///
/// Fetches all pending atoms, marks them as 'processing', and processes them in batch.
/// Returns the number of atoms queued for processing.
pub fn process_pending_embeddings<F>(
    storage: StorageBackend,
    on_event: F,
) -> Result<i32, String>
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    process_pending_embeddings_inner(storage, on_event, None)
}

/// Process pending embeddings with externally-provided settings (from registry).
pub fn process_pending_embeddings_with_settings<F>(
    storage: StorageBackend,
    on_event: F,
    settings_map: HashMap<String, String>,
) -> Result<i32, String>
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    process_pending_embeddings_inner(storage, on_event, Some(settings_map))
}

/// Max atoms to process per background embedding batch (limits memory usage).
const PENDING_BATCH_SIZE: i32 = 100;

fn process_pending_embeddings_inner<F>(
    storage: StorageBackend,
    on_event: F,
    external_settings: Option<HashMap<String, String>>,
) -> Result<i32, String>
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    // Claim atoms in capped batches to avoid loading thousands into memory at once.
    // Each batch is processed sequentially (gated by EMBEDDING_BATCH_SEMAPHORE).
    let mut total_count = 0i32;

    loop {
        let pending_atoms = storage.claim_pending_embeddings_sync(PENDING_BATCH_SIZE)
            .map_err(|e| e.to_string())?;

        if pending_atoms.is_empty() {
            break;
        }

        total_count += pending_atoms.len() as i32;

        let storage = storage.clone();
        let on_event = on_event.clone();
        let settings = external_settings.clone();

        crate::executor::spawn(async move {
            // Limit concurrent batch tasks to bound memory
            let _permit = crate::executor::EMBEDDING_BATCH_SEMAPHORE
                .acquire()
                .await
                .expect("Embedding batch semaphore closed unexpectedly");

            let input = AtomInput::Preloaded(pending_atoms);
            match settings {
                Some(s) => process_embedding_batch_with_settings(
                    storage,
                    input,
                    false,
                    on_event,
                    s,
                ).await,
                None => process_embedding_batch(
                    storage,
                    input,
                    false,
                    on_event,
                ).await,
            };
        });
    }

    Ok(total_count)
}

/// Running accumulator for computing a centroid without holding all blobs in memory.
struct CentroidAccumulator {
    sum: Vec<f64>,
    count: usize,
}

impl CentroidAccumulator {
    fn new(dim: usize) -> Self {
        Self { sum: vec![0.0f64; dim], count: 0 }
    }

    /// Add an embedding blob to the running sum. Silently skips malformed blobs.
    fn add_blob(&mut self, blob: &[u8]) {
        let dim = self.sum.len();
        if blob.len() != dim * 4 {
            return;
        }
        for i in 0..dim {
            let bytes: [u8; 4] = [
                blob[i * 4],
                blob[i * 4 + 1],
                blob[i * 4 + 2],
                blob[i * 4 + 3],
            ];
            self.sum[i] += f32::from_le_bytes(bytes) as f64;
        }
        self.count += 1;
    }

    /// Finalize into a normalized unit-length f32 blob. Returns None if empty.
    fn finalize(&self) -> Option<Vec<u8>> {
        if self.count == 0 {
            return None;
        }
        let count = self.count as f64;
        let mut centroid: Vec<f64> = self.sum.iter().map(|v| v / count).collect();

        // Normalize to unit length
        let magnitude: f64 = centroid.iter().map(|v| v * v).sum::<f64>().sqrt();
        if magnitude > 0.0 {
            for val in &mut centroid {
                *val /= magnitude;
            }
        }

        let f32_vec: Vec<f32> = centroid.iter().map(|v| *v as f32).collect();
        Some(f32_vec_to_blob_public(&f32_vec))
    }
}

/// Write a computed centroid to tag_embeddings + vec_tags.
fn upsert_tag_centroid(
    conn: &rusqlite::Connection,
    tag_id: &str,
    embedding_blob: &[u8],
    chunk_count: i32,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO tag_embeddings (tag_id, embedding, atom_count, updated_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tag_id, embedding_blob, chunk_count, &now],
    )
    .map_err(|e| format!("Failed to upsert tag_embeddings: {}", e))?;

    // vec0 doesn't support REPLACE, so delete + insert
    conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id]).ok();
    conn.execute(
        "INSERT INTO vec_tags (tag_id, embedding) VALUES (?1, ?2)",
        rusqlite::params![tag_id, embedding_blob],
    )
    .map_err(|e| format!("Failed to upsert vec_tags: {}", e))?;

    Ok(())
}

/// Compute the centroid embedding for a single tag (streaming, constant memory).
///
/// Averages all chunk embeddings from atoms under this tag (including descendant tags),
/// normalizes to unit length, and upserts into `tag_embeddings` + `vec_tags`.
pub fn compute_tag_embedding(conn: &rusqlite::Connection, tag_id: &str) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE descendant_tags(id) AS (
                SELECT ?1
                UNION ALL
                SELECT t.id FROM tags t
                INNER JOIN descendant_tags dt ON t.parent_id = dt.id
            )
            SELECT ac.embedding
            FROM atom_chunks ac
            INNER JOIN atom_tags at ON ac.atom_id = at.atom_id
            WHERE at.tag_id IN (SELECT id FROM descendant_tags)
              AND ac.embedding IS NOT NULL",
        )
        .map_err(|e| format!("Failed to prepare tag embedding query: {}", e))?;

    // Determine dimension from vec_chunks schema
    let dim: usize = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|sql| {
            let start = sql.find("float[")?;
            let after = &sql[start + 6..];
            let end = after.find(']')?;
            after[..end].parse::<usize>().ok()
        })
        .unwrap_or(1536);

    let mut acc = CentroidAccumulator::new(dim);

    let mut rows = stmt.query([tag_id])
        .map_err(|e| format!("Failed to query tag embeddings: {}", e))?;

    while let Some(row) = rows.next().map_err(|e| format!("Failed to read row: {}", e))? {
        let blob: Vec<u8> = row.get(0).map_err(|e| format!("Failed to get blob: {}", e))?;
        acc.add_blob(&blob);
    }

    match acc.finalize() {
        Some(blob) => upsert_tag_centroid(conn, tag_id, &blob, acc.count as i32),
        None => {
            conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id]).ok();
            conn.execute("DELETE FROM tag_embeddings WHERE tag_id = ?1", [tag_id]).ok();
            Ok(())
        }
    }
}

/// Compute centroid embeddings for multiple tags in a single pass.
///
/// Builds an inverted ancestry map so each chunk embedding is read from SQLite exactly
/// once and accumulated into every tag centroid that includes it (the tag itself + all
/// its ancestors in the affected set).
pub fn compute_tag_embeddings_batch(
    conn: &rusqlite::Connection,
    tag_ids: &[String],
) -> Result<(), String> {
    if tag_ids.is_empty() {
        return Ok(());
    }

    // Build the set of tags we're computing centroids for
    let target_set: std::collections::HashSet<&str> =
        tag_ids.iter().map(|s| s.as_str()).collect();

    // For each target tag, get its full descendant hierarchy. Build an inverted map:
    // descendant_tag_id → set of target tag_ids whose centroid it contributes to.
    let mut descendant_to_targets: std::collections::HashMap<String, Vec<&str>> =
        std::collections::HashMap::new();

    for tag_id in tag_ids {
        let mut stmt = conn
            .prepare(
                "WITH RECURSIVE descendant_tags(id) AS (
                    SELECT ?1
                    UNION ALL
                    SELECT t.id FROM tags t
                    INNER JOIN descendant_tags dt ON t.parent_id = dt.id
                )
                SELECT id FROM descendant_tags",
            )
            .map_err(|e| format!("Failed to prepare hierarchy query: {}", e))?;

        let descendants: Vec<String> = stmt
            .query_map([tag_id.as_str()], |row| row.get(0))
            .map_err(|e| format!("Failed to query hierarchy: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect hierarchy: {}", e))?;

        for desc_id in descendants {
            descendant_to_targets
                .entry(desc_id)
                .or_default()
                .push(tag_id.as_str());
        }
    }

    // Determine embedding dimension from vec_chunks schema
    let dim: usize = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_chunks'",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|sql| {
            let start = sql.find("float[")?;
            let after = &sql[start + 6..];
            let end = after.find(']')?;
            after[..end].parse::<usize>().ok()
        })
        .unwrap_or(1536);

    // Initialize accumulators for each target tag
    let mut accumulators: std::collections::HashMap<&str, CentroidAccumulator> = tag_ids
        .iter()
        .map(|id| (id.as_str(), CentroidAccumulator::new(dim)))
        .collect();

    // Collect all descendant tag IDs that map to at least one target
    let all_descendant_ids: Vec<&str> = descendant_to_targets.keys().map(|s| s.as_str()).collect();

    // Stream chunk embeddings for all atoms tagged under any descendant, in batches
    // to avoid SQLite parameter limits (max ~999)
    for batch in all_descendant_ids.chunks(500) {
        let placeholders = batch.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT at.tag_id, ac.embedding
             FROM atom_chunks ac
             INNER JOIN atom_tags at ON ac.atom_id = at.atom_id
             WHERE at.tag_id IN ({})
               AND ac.embedding IS NOT NULL",
            placeholders
        );

        let mut stmt = conn.prepare(&query)
            .map_err(|e| format!("Failed to prepare batch embedding query: {}", e))?;

        let mut rows = stmt.query(rusqlite::params_from_iter(batch.iter()))
            .map_err(|e| format!("Failed to query batch embeddings: {}", e))?;

        while let Some(row) = rows.next().map_err(|e| format!("Failed to read row: {}", e))? {
            let tag_id: String = row.get(0).map_err(|e| format!("Failed to get tag_id: {}", e))?;
            let blob: Vec<u8> = row.get(1).map_err(|e| format!("Failed to get blob: {}", e))?;

            // Look up which target centroids this row contributes to
            if let Some(targets) = descendant_to_targets.get(&tag_id) {
                for &target_id in targets {
                    if let Some(acc) = accumulators.get_mut(target_id) {
                        acc.add_blob(&blob);
                    }
                }
            }
        }
    }

    // Finalize and write all centroids
    for tag_id in tag_ids {
        if let Some(acc) = accumulators.get(tag_id.as_str()) {
            match acc.finalize() {
                Some(blob) => {
                    if let Err(e) = upsert_tag_centroid(conn, tag_id, &blob, acc.count as i32) {
                        tracing::warn!(tag_id, error = %e, "Failed to write centroid for tag");
                    }
                }
                None => {
                    conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id.as_str()]).ok();
                    conn.execute("DELETE FROM tag_embeddings WHERE tag_id = ?1", [tag_id.as_str()]).ok();
                }
            }
        }
    }

    Ok(())
}

/// Max atoms to process per edge computation batch.
const EDGE_BATCH_SIZE: i32 = 500;

/// Process all atoms with pending edge computation in batches.
///
/// Claims atoms in batches, computes edges in a single transaction, marks them
/// complete, and repeats. Each batch is checkpointed so progress survives restarts.
/// Returns the total number of atoms processed.
pub fn process_pending_edges(storage: StorageBackend) -> Result<i32, String> {
    let pending_count = storage.count_pending_edges_sync()
        .map_err(|e| e.to_string())?;

    if pending_count == 0 {
        return Ok(0);
    }

    tracing::info!(count = pending_count, "Starting batched edge computation");

    let storage_clone = storage.clone();
    crate::executor::spawn(async move {
        let mut total_processed = 0;
        loop {
            let batch = match storage_clone.claim_pending_edges_sync(EDGE_BATCH_SIZE) {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to claim atoms for edge computation");
                    break;
                }
            };

            if batch.is_empty() {
                break;
            }

            let batch_size = batch.len();

            // Compute all edges in a single transaction (one lock acquire, one fsync)
            let batch_edges = match storage_clone.compute_semantic_edges_batch_sync(&batch, 0.5, 15) {
                Ok(count) => count,
                Err(e) => {
                    tracing::error!(error = %e, "Failed to compute edges for batch");
                    // Mark as complete anyway to avoid infinite retry
                    0
                }
            };

            // Checkpoint: mark this batch complete before claiming the next
            if let Err(e) = storage_clone.set_edges_status_batch_sync(&batch, "complete") {
                tracing::error!(error = %e, "Failed to mark edges as complete");
                break;
            }

            total_processed += batch_size;
            tracing::info!(
                batch_edges,
                progress = total_processed,
                "Edge computation batch complete"
            );

            // Yield to other tasks between batches
            tokio::task::yield_now().await;
        }

        tracing::info!(total = total_processed, "Edge computation pipeline complete");
    });

    Ok(pending_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_to_similarity() {
        // Formula: 1.0 - (distance² / 2.0), clamped to [-1.0, 1.0]
        assert!((distance_to_similarity(0.0) - 1.0).abs() < 0.001); // 1 - 0 = 1.0
        assert!((distance_to_similarity(1.0) - 0.5).abs() < 0.001); // 1 - 0.5 = 0.5
        // distance = √2 gives 1.0 - 1.0 = 0.0
        assert!((distance_to_similarity(std::f32::consts::SQRT_2) - 0.0).abs() < 0.001);
        // distance = 2.0 gives 1.0 - 2.0 = -1.0 (clamped)
        assert!((distance_to_similarity(2.0) - (-1.0)).abs() < 0.001);
    }
}
