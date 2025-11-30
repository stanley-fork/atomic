use crate::chunking::chunk_content;
use crate::db::Database;
use crate::extraction::{
    build_tag_info_for_consolidation, cleanup_orphaned_parents, consolidate_atom_tags,
    extract_tags_from_chunk, get_or_create_tag, get_tag_tree_for_llm, link_tags_to_atom,
    tag_names_to_ids,
};
use crate::models::EmbeddingCompletePayload;
use crate::providers::models::{fetch_and_return_capabilities, get_cached_capabilities_sync, save_capabilities_cache};
use crate::providers::openrouter::OpenRouterProvider;
use crate::providers::traits::{EmbeddingConfig, EmbeddingProvider};
use crate::settings;
use reqwest::Client;
use std::sync::{Arc, LazyLock};
use tauri::AppHandle;
use tauri::Emitter;
use tokio::sync::Semaphore;
use uuid::Uuid;

// Limit concurrent embedding tasks to prevent thread exhaustion
const MAX_CONCURRENT_EMBEDDINGS: usize = 1;

static EMBEDDING_SEMAPHORE: LazyLock<Semaphore> = LazyLock::new(|| {
    Semaphore::new(MAX_CONCURRENT_EMBEDDINGS)
});

/// Generate embeddings via provider abstraction (batch support)
/// Uses the configured embedding model from settings or defaults to text-embedding-3-small
pub async fn generate_embeddings_with_provider(
    api_key: &str,
    texts: &[String],
    model: Option<&str>,
) -> Result<Vec<Vec<f32>>, String> {
    let provider = OpenRouterProvider::new(api_key.to_string());
    let config = EmbeddingConfig::new(
        model.unwrap_or("openai/text-embedding-3-small")
    );

    provider
        .embed_batch(texts, &config)
        .await
        .map_err(|e| e.to_string())
}

/// Generate embeddings via OpenRouter API (batch support)
/// DEPRECATED: Use generate_embeddings_with_provider instead
/// Kept for backward compatibility with existing code
pub async fn generate_openrouter_embeddings_public(
    _client: &Client,
    api_key: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    generate_embeddings_with_provider(api_key, texts, None).await
}

/// Convert f32 vector to binary blob for sqlite-vec
pub fn f32_vec_to_blob_public(vec: &[f32]) -> Vec<u8> {
    vec.iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// Process embeddings for a SINGLE atom (used by create_atom/update_atom)
/// This maintains the old behavior: spawns dedicated thread + runtime
/// Fine for 1-2 atoms, but use process_embedding_batch for bulk operations
///
/// This spawns a background task that:
/// 1. Sets embedding_status to 'processing'
/// 2. Chunks the content
/// 3. Generates real embeddings for each chunk using sqlite-lembed
/// 4. Extracts tags using OpenRouter LLM (if enabled)
/// 5. Stores chunks and embeddings in database
/// 6. Links extracted tags to the atom
/// 7. Sets embedding_status to 'complete' or 'failed'
/// 8. Emits 'embedding-complete' event with tag info
pub fn spawn_embedding_task_single(
    app_handle: AppHandle,
    db: Arc<Database>,
    atom_id: String,
    content: String,
) {
    std::thread::spawn(move || {
        // Create a tokio runtime for async operations
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        
        let result = rt.block_on(process_embeddings(&db, &atom_id, &content));

        let payload = match result {
            Ok((tags_extracted, new_tags_created)) => EmbeddingCompletePayload {
                atom_id: atom_id.clone(),
                status: "complete".to_string(),
                error: None,
                tags_extracted,
                new_tags_created,
            },
            Err(e) => {
                // Update status to failed
                if let Ok(conn) = db.conn.lock() {
                    let _ = conn.execute(
                        "UPDATE atoms SET embedding_status = 'failed' WHERE id = ?1",
                        [&atom_id],
                    );
                }
                EmbeddingCompletePayload {
                    atom_id: atom_id.clone(),
                    status: "failed".to_string(),
                    error: Some(e),
                    tags_extracted: Vec::new(),
                    new_tags_created: Vec::new(),
                }
            }
        };

        // Emit event to frontend
        let _ = app_handle.emit("embedding-complete", payload);
    });
}

/// Process embeddings for multiple atoms concurrently with semaphore-based limiting
/// Used by process_pending_embeddings for bulk operations
pub async fn process_embedding_batch(
    app_handle: AppHandle,
    db: Arc<Database>,
    atoms: Vec<(String, String)>,
) {
    let mut tasks = Vec::with_capacity(atoms.len());

    for (atom_id, content) in atoms {
        let app_handle = app_handle.clone();
        let db = Arc::clone(&db);

        let task = tokio::spawn(async move {
            // Acquire semaphore permit - blocks if 15 tasks already running
            let _permit = EMBEDDING_SEMAPHORE.acquire().await
                .expect("Semaphore closed unexpectedly");

            // Process with permit held (auto-released on drop)
            let result = process_embeddings(&db, &atom_id, &content).await;

            // Build payload and emit event (same as current behavior)
            let payload = match result {
                Ok((tags_extracted, new_tags_created)) => EmbeddingCompletePayload {
                    atom_id: atom_id.clone(),
                    status: "complete".to_string(),
                    error: None,
                    tags_extracted,
                    new_tags_created,
                },
                Err(e) => {
                    // Update status to failed
                    if let Ok(conn) = db.conn.lock() {
                        let _ = conn.execute(
                            "UPDATE atoms SET embedding_status = 'failed' WHERE id = ?1",
                            [&atom_id],
                        );
                    }
                    EmbeddingCompletePayload {
                        atom_id: atom_id.clone(),
                        status: "failed".to_string(),
                        error: Some(e),
                        tags_extracted: Vec::new(),
                        new_tags_created: Vec::new(),
                    }
                }
            };

            let _ = app_handle.emit("embedding-complete", payload);
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let _ = task.await;
    }
}

async fn process_embeddings(
    db: &Database,
    atom_id: &str,
    content: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    // Scope to ensure connection is dropped before any async operations
    let (auto_tagging_enabled, api_key, tagging_model, embedding_model, chunks) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Set status to processing
        conn.execute(
            "UPDATE atoms SET embedding_status = 'processing' WHERE id = ?1",
            [atom_id],
        )
        .map_err(|e| e.to_string())?;

        // Get settings for auto-tagging and embeddings
        let settings_map = settings::get_all_settings(&conn)?;
        let auto_tagging_enabled = settings_map
            .get("auto_tagging_enabled")
            .map(|v| v == "true")
            .unwrap_or(true); // Default to true
        let api_key = settings_map
            .get("openrouter_api_key")
            .cloned()
            .ok_or("OpenRouter API key not configured. Embeddings require API key.")?;
        let tagging_model = settings_map
            .get("tagging_model")
            .cloned()
            .unwrap_or_else(|| "openai/gpt-4o-mini".to_string());
        let embedding_model = settings_map
            .get("embedding_model")
            .cloned()
            .unwrap_or_else(|| "openai/text-embedding-3-small".to_string());

        // First, get existing chunk IDs for this atom to delete from vec_chunks
        let existing_chunk_ids: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT id FROM atom_chunks WHERE atom_id = ?1")
                .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;
            let ids = stmt
                .query_map([atom_id], |row| row.get(0))
                .map_err(|e| format!("Failed to query chunks: {}", e))?
                .collect::<Result<Vec<String>, _>>()
                .map_err(|e| format!("Failed to collect chunk IDs: {}", e))?;
            ids
        };

        // Delete existing vec_chunks entries for this atom's chunks
        for chunk_id in &existing_chunk_ids {
            conn.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [chunk_id])
                .ok(); // Ignore errors if chunk doesn't exist in vec_chunks
        }

        // Delete existing chunks for this atom
        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])
            .map_err(|e| e.to_string())?;

        // Chunk the content
        let chunks = chunk_content(content);

        if chunks.is_empty() {
            // No chunks to process, mark as complete
            conn.execute(
                "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
                [atom_id],
            )
            .map_err(|e| e.to_string())?;
            return Ok((Vec::new(), Vec::new()));
        }

        (auto_tagging_enabled, api_key, tagging_model, embedding_model, chunks)
    }; // Connection dropped here

    // Create HTTP client for OpenRouter API (still used for tag extraction)
    let client = Client::new();

    // Load model capabilities for parameter filtering
    let supported_params: Option<Vec<String>> = if auto_tagging_enabled {
        // Step 1: Check cache (sync, with lock)
        let (cached, is_stale) = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            match get_cached_capabilities_sync(&conn) {
                Ok(Some(cache)) => {
                    let stale = cache.is_stale();
                    (Some(cache), stale)
                }
                Ok(None) => (None, true),
                Err(_) => (None, true),
            }
        }; // Lock dropped here

        // Step 2: Fetch fresh if needed (async, no lock)
        let capabilities = if is_stale {
            match fetch_and_return_capabilities(&client).await {
                Ok(fresh_cache) => {
                    // Step 3: Save to DB (sync, with new connection)
                    if let Ok(conn) = db.new_connection() {
                        let _ = save_capabilities_cache(&conn, &fresh_cache);
                    }
                    fresh_cache
                }
                Err(_) => cached.unwrap_or_default(),
            }
        } else {
            cached.unwrap_or_default()
        };

        capabilities.get_supported_params(&tagging_model).cloned()
    } else {
        None
    };

    // Track all tag IDs and new tag IDs across all chunks
    let mut all_tag_ids: Vec<String> = Vec::new();
    let mut all_new_tag_ids: Vec<String> = Vec::new();

    // Generate all embeddings via provider in one batch
    let chunk_texts: Vec<String> = chunks.iter().map(|s| s.to_string()).collect();
    let embeddings = generate_embeddings_with_provider(&api_key, &chunk_texts, Some(&embedding_model))
        .await
        .map_err(|e| format!("Failed to generate embeddings: {}", e))?;

    // Process each chunk with its embedding SEQUENTIALLY
    // Each chunk sees the tags extracted from previous chunks
    for (index, chunk_content) in chunks.iter().enumerate() {
        // Get fresh tag tree that includes tags from previous chunks
        let tag_tree_json = if auto_tagging_enabled {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            get_tag_tree_for_llm(&conn)?
        } else {
            String::new()
        };

        // Database operations in scope to ensure lock is dropped before async operations
        {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;

            let chunk_id = Uuid::new_v4().to_string();

            // Convert OpenRouter embedding (f32 vec) to binary blob
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
        } // Connection dropped here

        // Extract tags with current tag tree (includes tags from previous chunks)
        if auto_tagging_enabled {
            match extract_tags_from_chunk(&client, &api_key, chunk_content, &tag_tree_json, &tagging_model, supported_params.clone()).await {
                Ok(result) => {
                    let conn = db.conn.lock().map_err(|e| e.to_string())?;

                    // Process each tag: find or create
                    let mut chunk_tag_ids = Vec::new();

                    for tag_application in result.tags {
                        // Skip invalid tag names
                        let trimmed_name = tag_application.name.trim();
                        if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
                            eprintln!("Skipping invalid tag name: '{}'", tag_application.name);
                            continue;
                        }

                        // Look up tag by name (case-insensitive), create if doesn't exist
                        match get_or_create_tag(&conn, &tag_application.name, &tag_application.parent_name) {
                            Ok(tag_id) => chunk_tag_ids.push(tag_id),
                            Err(e) => eprintln!("Failed to get/create tag '{}': {}", tag_application.name, e),
                        }
                    }

                    // Link all tags to atom
                    if !chunk_tag_ids.is_empty() {
                        link_tags_to_atom(&conn, atom_id, &chunk_tag_ids)?;
                    }

                    // Track for event payload
                    all_tag_ids.extend(chunk_tag_ids.clone());
                    all_new_tag_ids.extend(chunk_tag_ids);
                },
                Err(e) => {
                    // Log warning but continue - don't fail the whole process
                    eprintln!("Tag extraction failed for chunk {}: {}", index, e);
                }
            }
        }
    }

    // CONSOLIDATION PASS (only for multi-chunk atoms)
    if chunks.len() > 1 && auto_tagging_enabled && !all_tag_ids.is_empty() {
        // Deduplicate tag IDs before consolidation
        all_tag_ids.sort();
        all_tag_ids.dedup();

        // Build tag info string (sync operation with lock)
        let tag_info = {
            let conn = db.conn.lock().map_err(|e| e.to_string())?;
            build_tag_info_for_consolidation(&conn, &all_tag_ids)?
        }; // Lock dropped here

        // Call consolidation (async operation without lock)
        match consolidate_atom_tags(&client, &api_key, tag_info, &tagging_model, supported_params.clone()).await {
            Ok(consolidation) => {
                // Re-acquire connection for consolidation operations
                let conn = db.conn.lock().map_err(|e| e.to_string())?;

                // TRANSLATION LAYER: Names → IDs
                let lookup_result = tag_names_to_ids(&conn, &consolidation.tags_to_remove)?;
                if !lookup_result.missing_names.is_empty() {
                    eprintln!("Warning: Consolidation recommended removing non-existent tags: {:?}",
                        lookup_result.missing_names);
                }
                let remove_ids = lookup_result.found_ids;

                // Remove tags from atom
                for tag_id in &remove_ids {
                    conn.execute(
                        "DELETE FROM atom_tags WHERE atom_id = ?1 AND tag_id = ?2",
                        rusqlite::params![atom_id, tag_id],
                    )
                    .map_err(|e| e.to_string())?;

                    // Check if tag is used elsewhere
                    let usage_count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM atom_tags WHERE tag_id = ?1",
                            [tag_id],
                            |row| row.get(0),
                        )
                        .map_err(|e| e.to_string())?;

                    // Delete tag entirely if unused
                    if usage_count == 0 {
                        // Check if tag has a wiki article
                        let has_wiki: bool = conn
                            .query_row(
                                "SELECT 1 FROM wiki_articles WHERE tag_id = ?1",
                                [tag_id],
                                |_| Ok(true),
                            )
                            .unwrap_or(false);

                        if has_wiki {
                            eprintln!("Skipping deletion of tag {} - has associated wiki article", tag_id);
                        } else {
                            conn.execute("DELETE FROM tags WHERE id = ?1", [tag_id])
                                .map_err(|e| e.to_string())?;

                            // Clean up orphaned parents
                            cleanup_orphaned_parents(&conn, tag_id)?;
                        }
                    }
                }

                // Create new broader tags
                let mut new_tag_ids = Vec::new();
                for tag_application in consolidation.tags_to_add {
                    // Skip invalid tag names
                    let trimmed_name = tag_application.name.trim();
                    if trimmed_name.is_empty() || trimmed_name.eq_ignore_ascii_case("null") {
                        eprintln!("Skipping invalid consolidation tag name: '{}'", tag_application.name);
                        continue;
                    }

                    match get_or_create_tag(&conn, &tag_application.name, &tag_application.parent_name) {
                        Ok(tag_id) => new_tag_ids.push(tag_id),
                        Err(e) => eprintln!("Failed to get/create consolidation tag '{}': {}", tag_application.name, e),
                    }
                }

                // Link new tags to atom
                if !new_tag_ids.is_empty() {
                    link_tags_to_atom(&conn, atom_id, &new_tag_ids)?;
                }

                // Update tracking for event payload
                all_tag_ids.retain(|id| !remove_ids.contains(id));
                all_tag_ids.extend(new_tag_ids.clone());
                all_new_tag_ids.extend(new_tag_ids.clone());

                eprintln!("Tag consolidation complete for atom {}: removed {}, added {}",
                    atom_id, remove_ids.len(), new_tag_ids.len());
            }
            Err(e) => {
                eprintln!("Tag consolidation failed for atom {}: {}", atom_id, e);
                // Don't fail the whole process - consolidation is optional
            }
        }
    }

    // Re-acquire connection for final operations
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Compute semantic edges for this atom
    // Use threshold of 0.5 to capture more relationships, max 15 edges per atom
    match compute_semantic_edges_for_atom(&conn, atom_id, 0.5, 15) {
        Ok(edge_count) => {
            if edge_count > 0 {
                eprintln!("Created {} semantic edges for atom {}", edge_count, atom_id);
            }
        }
        Err(e) => {
            // Log warning but don't fail the embedding process
            eprintln!("Warning: Failed to compute semantic edges for atom {}: {}", atom_id, e);
        }
    }

    // Set status to complete
    conn.execute(
        "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
        [atom_id],
    )
    .map_err(|e| e.to_string())?;

    // Deduplicate tag IDs for the event payload
    all_tag_ids.sort();
    all_tag_ids.dedup();
    all_new_tag_ids.sort();
    all_new_tag_ids.dedup();

    Ok((all_tag_ids, all_new_tag_ids))
}

/// Convert distance to similarity score (0-1 scale)
/// For normalized vectors using L2 distance, max distance is 2.0 (opposite vectors)
pub fn distance_to_similarity(distance: f32) -> f32 {
    // For L2 distance on normalized vectors:
    // distance = 0 means identical vectors (similarity = 1)
    // distance = 2 means opposite vectors (similarity = 0)
    (1.0 - (distance / 2.0)).max(0.0).min(1.0)
}

/// Compute semantic edges for an atom after embedding generation
/// Finds similar atoms based on vector similarity and stores edges in semantic_edges table
pub fn compute_semantic_edges_for_atom(
    conn: &rusqlite::Connection,
    atom_id: &str,
    threshold: f32,   // Default: 0.5 - lower than UI threshold to capture more relationships
    max_edges: i32,   // Default: 15 per atom
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

        // For each similar chunk, check threshold and track best match per atom
        for (chunk_id, distance) in similar_chunks {
            let similarity = distance_to_similarity(distance);

            if similarity < threshold {
                continue;
            }

            // Get the parent atom_id and chunk index for this chunk
            let chunk_info: Result<(String, i32), _> = conn.query_row(
                "SELECT atom_id, chunk_index FROM atom_chunks WHERE id = ?1",
                [&chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );

            if let Ok((target_atom_id, target_chunk_index)) = chunk_info {
                // Exclude the source atom itself
                if target_atom_id == atom_id {
                    continue;
                }

                // Keep highest similarity per target atom
                let entry = atom_similarities.entry(target_atom_id.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, *source_chunk_index, target_chunk_index));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((similarity, *source_chunk_index, target_chunk_index));
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
            (atom_id.to_string(), target_atom_id.clone(), source_chunk_index, target_chunk_index)
        } else {
            (target_atom_id.clone(), atom_id.to_string(), target_chunk_index, source_chunk_index)
        };

        let edge_id = Uuid::new_v4().to_string();

        // Insert or update (using INSERT OR REPLACE due to UNIQUE constraint)
        let result = conn.execute(
            "INSERT OR REPLACE INTO semantic_edges
             (id, source_atom_id, target_atom_id, similarity_score, source_chunk_index, target_chunk_index, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                &edge_id,
                &src_id,
                &tgt_id,
                similarity,
                src_chunk,
                tgt_chunk,
                &now,
            ],
        );

        if result.is_ok() {
            edges_created += 1;
        }
    }

    Ok(edges_created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance_to_similarity() {
        assert!((distance_to_similarity(0.0) - 1.0).abs() < 0.001);
        assert!((distance_to_similarity(2.0) - 0.0).abs() < 0.001);
        assert!((distance_to_similarity(1.0) - 0.5).abs() < 0.001);
    }
}

