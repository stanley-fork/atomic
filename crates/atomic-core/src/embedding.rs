//! Embedding generation pipeline with callback-based events
//!
//! This module handles:
//! - Embedding generation via provider abstraction
//! - Tag extraction via LLM
//! - Semantic edge computation
//! - Callback-based event notification

use crate::chunking::chunk_content;
use crate::db::Database;
use crate::extraction::{
    extract_tags_from_content, get_or_create_tag, get_tag_tree_cached, link_tags_to_atom,
};
use crate::providers::traits::EmbeddingConfig;
use crate::providers::{get_embedding_provider, get_model_capabilities, ProviderConfig, ProviderType};
use crate::settings;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};
use tokio::sync::Semaphore;
use uuid::Uuid;

// Concurrency limits — these apply uniformly whether processing 1 or 10K atoms
const MAX_CONCURRENT_TAGGING: usize = 4;

static TAGGING_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(MAX_CONCURRENT_TAGGING));

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
}

/// Generate embeddings via provider abstraction (batch support)
/// Uses ProviderConfig to determine which provider to use.
/// Includes retry with exponential backoff for transient failures.
pub async fn generate_embeddings_with_config(
    config: &ProviderConfig,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let provider = get_embedding_provider(config).map_err(|e| e.to_string())?;
    let embed_config = EmbeddingConfig::new(config.embedding_model());

    let mut last_error = String::new();
    for attempt in 0..3u32 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_secs(1 << attempt)).await;
        }

        match provider.embed_batch(texts, &embed_config).await {
            Ok(embeddings) => return Ok(embeddings),
            Err(e) => {
                last_error = e.to_string();
                if e.is_retryable() {
                    eprintln!("Embedding attempt {} failed (retryable): {}", attempt + 1, last_error);
                    continue;
                } else {
                    break;
                }
            }
        }
    }

    Err(last_error)
}

/// Maximum texts per embedding API call for cross-atom batching.
/// OpenRouter/OpenAI handle batches well at this size.
const EMBEDDING_BATCH_SIZE: usize = 150;

/// Metadata for a chunk awaiting embedding
#[derive(Clone)]
struct PendingChunk {
    atom_id: String,
    chunk_index: usize,
    content: String,
}

/// Embed a list of chunks in adaptive batches.
/// Splits into batches of EMBEDDING_BATCH_SIZE, calls the API, and on failure
/// retries at half batch size recursively. Returns (embedded, failed_atom_ids).
async fn embed_chunks_batched(
    config: &ProviderConfig,
    chunks: Vec<PendingChunk>,
) -> (Vec<(PendingChunk, Vec<f32>)>, Vec<String>) {
    if chunks.is_empty() {
        return (vec![], vec![]);
    }

    let mut results: Vec<(PendingChunk, Vec<f32>)> = Vec::with_capacity(chunks.len());
    let mut failed_atom_ids: Vec<String> = Vec::new();

    // Split chunks into batches
    let batches: Vec<Vec<PendingChunk>> = chunks
        .into_iter()
        .collect::<Vec<_>>()
        .chunks(EMBEDDING_BATCH_SIZE)
        .map(|c| c.to_vec())
        .collect();

    let total_batches = batches.len();
    for (batch_idx, batch) in batches.into_iter().enumerate() {
        eprintln!(
            "Embedding batch {}/{} ({} chunks)...",
            batch_idx + 1,
            total_batches,
            batch.len()
        );
        let (mut successes, mut failures) = embed_batch_adaptive(config, batch).await;
        results.append(&mut successes);
        failed_atom_ids.append(&mut failures);
    }

    failed_atom_ids.sort();
    failed_atom_ids.dedup();
    (results, failed_atom_ids)
}

/// Try to embed a batch. On failure, split in half and retry each half.
/// Base case: single chunk failure returns the atom_id as failed.
fn embed_batch_adaptive(
    config: &ProviderConfig,
    batch: Vec<PendingChunk>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = (Vec<(PendingChunk, Vec<f32>)>, Vec<String>)> + Send + '_>> {
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
            if batch.len() == 1 {
                // Single chunk failed — mark atom as failed
                eprintln!(
                    "Embedding failed for atom {} chunk {}: {}",
                    batch[0].atom_id, batch[0].chunk_index, e
                );
                let failed_id = batch[0].atom_id.clone();
                (vec![], vec![failed_id])
            } else {
                // Split in half and retry each half
                let mid = batch.len() / 2;
                let (first_half, second_half): (Vec<_>, Vec<_>) = batch
                    .into_iter()
                    .enumerate()
                    .partition(|(i, _)| *i < mid);
                let first: Vec<PendingChunk> = first_half.into_iter().map(|(_, c)| c).collect();
                let second: Vec<PendingChunk> = second_half.into_iter().map(|(_, c)| c).collect();

                eprintln!(
                    "Batch of {} failed, retrying as 2 batches of {}/{}...",
                    mid * 2,
                    first.len(),
                    second.len()
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
    db: &Database,
    atom_id: &str,
    content: &str,
) -> Result<(), String> {
    process_embedding_only_inner(db, atom_id, content, false).await
}

/// Inner implementation with edge deferral control.
/// When `skip_edges` is true, semantic edge computation is deferred (for batch processing).
async fn process_embedding_only_inner(
    db: &Database,
    atom_id: &str,
    content: &str,
    skip_edges: bool,
) -> Result<(), String> {
    // Scope for initial DB operations
    let (provider_config, chunks) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Set embedding status to processing
        conn.execute(
            "UPDATE atoms SET embedding_status = 'processing' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;

        // Get settings for embeddings
        let settings_map = settings::get_all_settings(&conn).map_err(|e| e.to_string())?;
        let provider_config = ProviderConfig::from_settings(&settings_map);

        // Validate provider configuration
        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            return Err(
                "OpenRouter API key not configured. Please set it in Settings.".to_string(),
            );
        }

        // Delete existing chunks for this atom
        conn.execute(
            "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
            [atom_id],
        )
        .ok();
        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])
            .map_err(|e| e.to_string())?;

        // Chunk content
        let chunks = chunk_content(content);

        if chunks.is_empty() {
            // No chunks to process, mark embedding as complete, tagging as skipped
            conn.execute(
                "UPDATE atoms SET embedding_status = 'complete', tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }

        (provider_config, chunks)
    }; // Connection dropped here

    // Generate all embeddings in one batch (async, no lock)
    let chunk_texts: Vec<String> = chunks.iter().map(|s| s.to_string()).collect();
    let embeddings = generate_embeddings_with_config(&provider_config, &chunk_texts)
        .await
        .map_err(|e| format!("Failed to generate embeddings: {}", e))?;

    // Store chunks and embeddings
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        for (index, chunk_content) in chunks.iter().enumerate() {
            let chunk_id = Uuid::new_v4().to_string();
            let embedding_blob = f32_vec_to_blob_public(&embeddings[index]);

            // Insert into atom_chunks
            conn.execute(
                "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![&chunk_id, atom_id, index as i32, chunk_content, &embedding_blob],
            )
            .map_err(|e| format!("Failed to insert chunk: {}", e))?;

            // Insert into vec_chunks for similarity search
            conn.execute(
                "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                rusqlite::params![&chunk_id, &embedding_blob],
            )
            .map_err(|e| format!("Failed to insert vec_chunk: {}", e))?;

        }

        // Rebuild FTS index to include new chunks
        conn.execute("INSERT INTO atom_chunks_fts(atom_chunks_fts) VALUES('rebuild')", [])
            .ok();

        // Compute semantic edges for this atom (unless deferred for batch)
        if !skip_edges {
            match compute_semantic_edges_for_atom(&conn, atom_id, 0.5, 15) {
                Ok(edge_count) => {
                    if edge_count > 0 {
                        eprintln!(
                            "Created {} semantic edges for atom {}",
                            edge_count, atom_id
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to compute semantic edges for atom {}: {}",
                        atom_id, e
                    );
                }
            }
        }

        // Recompute tag centroid embeddings for this atom's tags
        let tag_ids = get_tag_ids_for_atom(&conn, atom_id);
        if !tag_ids.is_empty() {
            if let Err(e) = compute_tag_embeddings_batch(&conn, &tag_ids) {
                eprintln!("Warning: Failed to recompute tag embeddings for atom {}: {}", atom_id, e);
            }
        }

        // Set embedding status to complete
        conn.execute(
            "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;
    }

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
    db: &Database,
    atom_id: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    // Get settings, validate state, and read raw content
    let (provider_config, tagging_model, content) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Set tagging status to processing
        conn.execute(
            "UPDATE atoms SET tagging_status = 'processing' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;

        // Get settings
        let settings_map = settings::get_all_settings(&conn).map_err(|e| e.to_string())?;
        let auto_tagging_enabled = settings_map
            .get("auto_tagging_enabled")
            .map(|v| v == "true")
            .unwrap_or(true);

        if !auto_tagging_enabled {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        let provider_config = ProviderConfig::from_settings(&settings_map);

        // Validate provider for LLM
        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        let tagging_model = provider_config.llm_model().to_string();

        // Read raw content directly from atoms table — no dependency on embedding
        let content: String = conn
            .query_row(
                "SELECT content FROM atoms WHERE id = ?1",
                [atom_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("Atom not found: {}", e))?;

        if content.trim().is_empty() {
            conn.execute(
                "UPDATE atoms SET tagging_status = 'skipped' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        (provider_config, tagging_model, content)
    }; // Connection dropped

    // Load model capabilities (uses in-memory + DB cache to avoid redundant fetches)
    let supported_params: Option<Vec<String>> =
        if provider_config.provider_type == ProviderType::OpenRouter {
            let db_path = db.db_path.clone();
            let capabilities = get_model_capabilities(move || {
                crate::db::Database::open(&db_path)
                    .map(|db| db.conn.into_inner().unwrap())
                    .map_err(|e| e.to_string())
            })
            .await;

            capabilities.and_then(|caps| caps.get_supported_params(&tagging_model).cloned())
        } else {
            None
        };

    // Get tag tree (cached to avoid redundant DB queries during bulk processing)
    let tag_tree_json = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        get_tag_tree_cached(&conn)?
    };

    // Single LLM call on full content — no per-chunk loop, no consolidation
    let result = extract_tags_from_content(
        &provider_config,
        &content,
        &tag_tree_json,
        &tagging_model,
        supported_params,
    )
    .await?;

    let mut all_tag_ids = Vec::new();

    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        for tag_application in result.tags {
            let trimmed_name = tag_application.name.trim();
            if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
                continue;
            }

            match get_or_create_tag(&conn, &tag_application.name, &tag_application.parent_name) {
                Ok(tag_id) => all_tag_ids.push(tag_id),
                Err(e) => eprintln!("Failed to get/create tag '{}': {}", tag_application.name, e),
            }
        }

        if !all_tag_ids.is_empty() {
            link_tags_to_atom(&conn, atom_id, &all_tag_ids)?;
        }

        // Set tagging status to complete
        conn.execute(
            "UPDATE atoms SET tagging_status = 'complete' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;
    }

    all_tag_ids.sort();
    all_tag_ids.dedup();
    let all_new_tag_ids = all_tag_ids.clone();

    Ok((all_tag_ids, all_new_tag_ids))
}

/// Process tagging for multiple atoms concurrently with semaphore-based limiting
/// Used by process_pending_tagging for bulk operations
pub async fn process_tagging_batch<F>(db: Arc<Database>, atom_ids: Vec<String>, on_event: F)
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    let mut tasks = Vec::with_capacity(atom_ids.len());

    for atom_id in atom_ids {
        let db = Arc::clone(&db);
        let on_event = on_event.clone();

        let task = tokio::spawn(async move {
            // Acquire semaphore permit
            let _permit = TAGGING_SEMAPHORE
                .acquire()
                .await
                .expect("Semaphore closed unexpectedly");

            let result = process_tagging_only(&db, &atom_id).await;

            let event = match result {
                Ok((tags_extracted, new_tags_created)) => EmbeddingEvent::TaggingComplete {
                    atom_id: atom_id.clone(),
                    tags_extracted,
                    new_tags_created,
                },
                Err(e) => {
                    if let Ok(conn) = db.conn.lock() {
                        let _ = conn.execute(
                            "UPDATE atoms SET tagging_status = 'failed' WHERE id = ?1",
                            [&atom_id],
                        );
                    }
                    EmbeddingEvent::TaggingFailed {
                        atom_id: atom_id.clone(),
                        error: e,
                    }
                }
            };

            on_event(event);
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }
}

/// Process embeddings and tagging for a SINGLE atom (used by create_atom/update_atom)
/// Spawns a background task that runs embedding and tagging concurrently.
/// Tagging reads raw content from the atoms table, so it does not depend on embedding.
pub fn spawn_embedding_task_single<F>(
    db: Arc<Database>,
    atom_id: String,
    content: String,
    on_event: F,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + 'static,
{
    let on_event = Arc::new(on_event);
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");

        // Emit started event
        on_event(EmbeddingEvent::Started {
            atom_id: atom_id.clone(),
        });

        // Run embedding and tagging concurrently — they're independent now
        let db_embed = Arc::clone(&db);
        let db_tag = Arc::clone(&db);
        let atom_id_embed = atom_id.clone();
        let atom_id_tag = atom_id.clone();
        let content_embed = content.clone();
        let on_event_embed = Arc::clone(&on_event);
        let on_event_tag = Arc::clone(&on_event);

        rt.block_on(async move {
            let embed_handle = tokio::spawn(async move {
                let result = process_embedding_only(&db_embed, &atom_id_embed, &content_embed).await;
                match &result {
                    Ok(()) => {
                        on_event_embed(EmbeddingEvent::EmbeddingComplete {
                            atom_id: atom_id_embed.clone(),
                        });
                    }
                    Err(e) => {
                        if let Ok(conn) = db_embed.conn.lock() {
                            let _ = conn.execute(
                                "UPDATE atoms SET embedding_status = 'failed' WHERE id = ?1",
                                [&atom_id_embed],
                            );
                        }
                        on_event_embed(EmbeddingEvent::EmbeddingFailed {
                            atom_id: atom_id_embed.clone(),
                            error: e.clone(),
                        });
                    }
                }
            });

            let tag_handle = tokio::spawn(async move {
                let result = process_tagging_only(&db_tag, &atom_id_tag).await;
                match result {
                    Ok((tags_extracted, new_tags_created)) => {
                        on_event_tag(EmbeddingEvent::TaggingComplete {
                            atom_id: atom_id_tag.clone(),
                            tags_extracted,
                            new_tags_created,
                        });
                    }
                    Err(e) => {
                        if let Ok(conn) = db_tag.conn.lock() {
                            let _ = conn.execute(
                                "UPDATE atoms SET tagging_status = 'failed' WHERE id = ?1",
                                [&atom_id_tag],
                            );
                        }
                        on_event_tag(EmbeddingEvent::TaggingFailed {
                            atom_id: atom_id_tag.clone(),
                            error: e,
                        });
                    }
                }
            });

            let _ = tokio::join!(embed_handle, tag_handle);
        });
    });
}

/// Process embeddings and tagging for multiple atoms concurrently.
/// Uses cross-atom batching for embedding API calls (reducing 10K calls to ~200).
/// Tagging runs per-atom concurrently via semaphores.
/// Set skip_tagging=true when re-embedding due to model/provider change (tags are preserved).
pub async fn process_embedding_batch<F>(
    db: Arc<Database>,
    atoms: Vec<(String, String)>,
    skip_tagging: bool,
    on_event: F,
) where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    let total_count = atoms.len();
    if total_count == 0 {
        return;
    }

    eprintln!(
        "Starting pipeline for {} atoms (cross-atom batching + concurrent tagging)...",
        total_count
    );

    // === Phase 1a: Get settings (brief lock) ===
    let provider_config = {
        let conn = db.conn.lock().expect("DB lock failed");

        let settings_map = match settings::get_all_settings(&conn) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to get settings: {}", e);
                return;
            }
        };
        let provider_config = ProviderConfig::from_settings(&settings_map);

        if provider_config.provider_type == ProviderType::OpenRouter
            && provider_config.openrouter_api_key.is_none()
        {
            eprintln!("OpenRouter API key not configured. Skipping embedding.");
            return;
        }

        provider_config
    }; // Lock released

    // === Phase 1b: Clean up old chunks (lock, but no FTS5 scans or chunking) ===
    {
        let conn = db.conn.lock().expect("DB lock failed");

        for (i, (atom_id, _)) in atoms.iter().enumerate() {
            // Delete old vec_chunks via subquery (single statement instead of N+1)
            conn.execute(
                "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
                [atom_id.as_str()],
            )
            .ok();

            // Delete old atom_chunks
            conn.execute(
                "DELETE FROM atom_chunks WHERE atom_id = ?1",
                [atom_id.as_str()],
            )
            .ok();

            if (i + 1) % 1000 == 0 {
                eprintln!("DB cleanup: {}/{} atoms", i + 1, total_count);
            }
        }

        eprintln!("DB cleanup complete for {} atoms", total_count);
    } // Lock released

    // === Phase 1c: Chunk content (NO lock — CPU only) ===
    let mut all_chunks: Vec<PendingChunk> = Vec::new();
    let mut empty_atom_ids: Vec<String> = Vec::new();

    for (i, (atom_id, content)) in atoms.iter().enumerate() {
        let chunks = chunk_content(content);
        if chunks.is_empty() {
            empty_atom_ids.push(atom_id.clone());
        } else {
            for (index, chunk) in chunks.into_iter().enumerate() {
                all_chunks.push(PendingChunk {
                    atom_id: atom_id.clone(),
                    chunk_index: index,
                    content: chunk,
                });
            }
        }

        if (i + 1) % 500 == 0 {
            eprintln!("Chunking progress: {}/{} atoms", i + 1, total_count);
        }
    }

    // Mark empty atoms as complete (brief lock)
    if !empty_atom_ids.is_empty() {
        let conn = db.conn.lock().expect("DB lock failed");
        for atom_id in &empty_atom_ids {
            conn.execute(
                "UPDATE atoms SET embedding_status = 'complete', tagging_status = 'skipped' WHERE id = ?1",
                [atom_id.as_str()],
            )
            .ok();
            on_event(EmbeddingEvent::EmbeddingComplete {
                atom_id: atom_id.clone(),
            });
        }
    }

    eprintln!(
        "Pre-chunked {} atoms into {} total chunks",
        total_count,
        all_chunks.len()
    );

    // === Phase 2: Spawn tagging tasks concurrently (independent of embedding) ===
    let mut tagging_tasks = Vec::new();
    if !skip_tagging {
        for (atom_id, _) in &atoms {
            let db = Arc::clone(&db);
            let atom_id = atom_id.clone();
            let on_event = on_event.clone();

            let task = tokio::spawn(async move {
                let _permit = TAGGING_SEMAPHORE
                    .acquire()
                    .await
                    .expect("Semaphore closed unexpectedly");

                let result = process_tagging_only(&db, &atom_id).await;

                let event = match result {
                    Ok((tags_extracted, new_tags_created)) => EmbeddingEvent::TaggingComplete {
                        atom_id: atom_id.clone(),
                        tags_extracted,
                        new_tags_created,
                    },
                    Err(e) => {
                        if let Ok(conn) = db.conn.lock() {
                            let _ = conn.execute(
                                "UPDATE atoms SET tagging_status = 'failed' WHERE id = ?1",
                                [&atom_id],
                            );
                        }
                        EmbeddingEvent::TaggingFailed {
                            atom_id: atom_id.clone(),
                            error: e,
                        }
                    }
                };

                on_event(event);
            });
            tagging_tasks.push(task);
        }
    }

    // === Phase 3: Cross-atom batched embedding API calls ===
    let (embedded_chunks, failed_atom_ids) =
        embed_chunks_batched(&provider_config, all_chunks).await;

    // === Phase 4: Store results in DB per-atom ===
    let mut completed_atom_ids = Vec::new();
    {
        use std::collections::HashMap;

        // Group embedded chunks by atom_id
        let mut by_atom: HashMap<String, Vec<(usize, String, Vec<f32>)>> = HashMap::new();
        for (chunk, embedding) in embedded_chunks {
            by_atom
                .entry(chunk.atom_id.clone())
                .or_default()
                .push((chunk.chunk_index, chunk.content, embedding));
        }

        let conn = db.conn.lock().expect("DB lock failed");

        // Store successful embeddings
        for (atom_id, chunks) in &by_atom {
            let mut success = true;
            for (chunk_index, content, embedding) in chunks {
                let chunk_id = Uuid::new_v4().to_string();
                let embedding_blob = f32_vec_to_blob_public(embedding);

                if conn
                    .execute(
                        "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![&chunk_id, atom_id, *chunk_index as i32, content, &embedding_blob],
                    )
                    .is_err()
                {
                    success = false;
                    break;
                }
                conn.execute(
                    "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                    rusqlite::params![&chunk_id, &embedding_blob],
                )
                .ok();
            }

            if success {
                conn.execute(
                    "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
                    [atom_id.as_str()],
                )
                .ok();
                completed_atom_ids.push(atom_id.clone());
                on_event(EmbeddingEvent::EmbeddingComplete {
                    atom_id: atom_id.clone(),
                });
            } else {
                conn.execute(
                    "UPDATE atoms SET embedding_status = 'failed' WHERE id = ?1",
                    [atom_id.as_str()],
                )
                .ok();
                on_event(EmbeddingEvent::EmbeddingFailed {
                    atom_id: atom_id.clone(),
                    error: "Failed to store embeddings in DB".to_string(),
                });
            }
        }

        // Mark atoms that failed embedding API calls
        for atom_id in &failed_atom_ids {
            conn.execute(
                "UPDATE atoms SET embedding_status = 'failed' WHERE id = ?1",
                [atom_id.as_str()],
            )
            .ok();
            on_event(EmbeddingEvent::EmbeddingFailed {
                atom_id: atom_id.clone(),
                error: "Embedding API call failed after retries".to_string(),
            });
        }

        eprintln!(
            "Embeddings stored: {} succeeded, {} failed",
            completed_atom_ids.len(),
            failed_atom_ids.len()
        );

        // Rebuild FTS index to include all new chunks
        conn.execute("INSERT INTO atom_chunks_fts(atom_chunks_fts) VALUES('rebuild')", [])
            .ok();
    } // DB lock released

    // === Phase 4b: Compute semantic edges (separate lock per atom) ===
    if total_count > 1 && !completed_atom_ids.is_empty() {
        eprintln!(
            "Computing semantic edges for {} atoms...",
            completed_atom_ids.len()
        );
        let mut total_edges = 0;
        for (i, atom_id) in completed_atom_ids.iter().enumerate() {
            let conn = db.conn.lock().expect("DB lock failed");
            match compute_semantic_edges_for_atom(&conn, atom_id, 0.5, 15) {
                Ok(count) => total_edges += count,
                Err(e) => {
                    eprintln!("Warning: Failed to compute edges for {}: {}", atom_id, e)
                }
            }
            drop(conn); // Release lock between atoms so tagging can interleave

            if (i + 1) % 1000 == 0 {
                eprintln!("Edge progress: {}/{} atoms ({} edges)", i + 1, completed_atom_ids.len(), total_edges);
            }
        }
        eprintln!(
            "Created {} total semantic edges for {} atoms",
            total_edges,
            completed_atom_ids.len()
        );
    }

    // === Phase 4c: Recompute tag centroid embeddings for affected tags ===
    if !completed_atom_ids.is_empty() {
        let conn = db.conn.lock().expect("DB lock failed");

        // Collect all unique tag IDs affected by the embedded atoms
        let mut affected_tag_ids: Vec<String> = Vec::new();
        for atom_id in &completed_atom_ids {
            affected_tag_ids.extend(get_tag_ids_for_atom(&conn, atom_id));
        }
        affected_tag_ids.sort();
        affected_tag_ids.dedup();

        if !affected_tag_ids.is_empty() {
            eprintln!("Recomputing centroid embeddings for {} tags...", affected_tag_ids.len());
            if let Err(e) = compute_tag_embeddings_batch(&conn, &affected_tag_ids) {
                eprintln!("Warning: Failed to recompute tag embeddings: {}", e);
            }
        }
    }

    // === Phase 5: Wait for tagging to complete ===
    for task in tagging_tasks {
        let _ = task.await;
    }

    if skip_tagging {
        eprintln!("Pipeline complete. Tagging was skipped (re-embedding only).");
    } else {
        eprintln!("Pipeline complete. All embedding and tagging tasks finished.");
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
    db: Arc<Database>,
    on_event: F,
) -> Result<i32, String>
where
    F: Fn(EmbeddingEvent) + Send + Sync + Clone + 'static,
{
    // Atomically fetch and mark pending atoms as 'processing' in a single statement
    // This prevents race conditions from duplicate calls
    let pending_atoms: Vec<(String, String)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "UPDATE atoms SET embedding_status = 'processing'
                 WHERE embedding_status = 'pending'
                 RETURNING id, content",
            )
            .map_err(|e| format!("Failed to prepare query: {}", e))?;
        let results: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to query pending atoms: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect pending atoms: {}", e))?;
        results
    };

    let count = pending_atoms.len() as i32;

    if count > 0 {
        // Process batch asynchronously in a separate thread
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(process_embedding_batch(
                db,
                pending_atoms,
                false, // don't skip tagging - normal flow
                on_event,
            ));
        });
    }

    Ok(count)
}

/// Compute the centroid embedding for a single tag.
///
/// Averages all chunk embeddings from atoms under this tag (including descendant tags),
/// normalizes to unit length, and upserts into `tag_embeddings` + `vec_tags`.
pub fn compute_tag_embedding(conn: &rusqlite::Connection, tag_id: &str) -> Result<(), String> {
    // Get all chunk embeddings for atoms under this tag hierarchy
    let embeddings: Vec<Vec<u8>> = {
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

        let results = stmt.query_map([tag_id], |row| row.get(0))
            .map_err(|e| format!("Failed to query tag embeddings: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect tag embeddings: {}", e))?;
        results
    };

    if embeddings.is_empty() {
        // No embeddings — remove any existing tag embedding
        conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id]).ok();
        conn.execute("DELETE FROM tag_embeddings WHERE tag_id = ?1", [tag_id]).ok();
        return Ok(());
    }

    // Determine dimension from first embedding blob
    let dim = embeddings[0].len() / 4;
    if dim == 0 {
        return Ok(());
    }

    // Average all vectors component-wise
    let mut centroid = vec![0.0f64; dim];
    let count = embeddings.len() as f64;

    for blob in &embeddings {
        if blob.len() != dim * 4 {
            continue;
        }
        for i in 0..dim {
            let bytes: [u8; 4] = [
                blob[i * 4],
                blob[i * 4 + 1],
                blob[i * 4 + 2],
                blob[i * 4 + 3],
            ];
            centroid[i] += f32::from_le_bytes(bytes) as f64;
        }
    }

    for val in &mut centroid {
        *val /= count;
    }

    // Normalize to unit length
    let magnitude: f64 = centroid.iter().map(|v| v * v).sum::<f64>().sqrt();
    if magnitude > 0.0 {
        for val in &mut centroid {
            *val /= magnitude;
        }
    }

    // Convert to f32 and then to blob
    let centroid_f32: Vec<f32> = centroid.iter().map(|v| *v as f32).collect();
    let embedding_blob = f32_vec_to_blob_public(&centroid_f32);

    let now = chrono::Utc::now().to_rfc3339();
    let atom_count = embeddings.len() as i32;

    // Upsert into tag_embeddings
    conn.execute(
        "INSERT OR REPLACE INTO tag_embeddings (tag_id, embedding, atom_count, updated_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![tag_id, &embedding_blob, atom_count, &now],
    )
    .map_err(|e| format!("Failed to upsert tag_embeddings: {}", e))?;

    // Upsert into vec_tags (delete + insert since vec0 doesn't support REPLACE)
    conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id]).ok();
    conn.execute(
        "INSERT INTO vec_tags (tag_id, embedding) VALUES (?1, ?2)",
        rusqlite::params![tag_id, &embedding_blob],
    )
    .map_err(|e| format!("Failed to upsert vec_tags: {}", e))?;

    Ok(())
}

/// Compute centroid embeddings for multiple tags.
pub fn compute_tag_embeddings_batch(
    conn: &rusqlite::Connection,
    tag_ids: &[String],
) -> Result<(), String> {
    for tag_id in tag_ids {
        if let Err(e) = compute_tag_embedding(conn, tag_id) {
            eprintln!("Warning: Failed to compute tag embedding for {}: {}", tag_id, e);
        }
    }
    Ok(())
}

/// Get tag IDs for an atom from atom_tags table.
fn get_tag_ids_for_atom(conn: &rusqlite::Connection, atom_id: &str) -> Vec<String> {
    let mut stmt = match conn.prepare("SELECT tag_id FROM atom_tags WHERE atom_id = ?1") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([atom_id], |row| row.get(0))
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
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
