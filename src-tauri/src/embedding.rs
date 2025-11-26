use crate::chunking::chunk_content;
use crate::db::Database;
use crate::extraction::{
    build_tag_info_for_consolidation, cleanup_orphaned_parents, consolidate_atom_tags,
    extract_tags_from_chunk, get_or_create_tag, get_tag_tree_for_llm, link_tags_to_atom,
    tag_names_to_ids,
};
use crate::models::EmbeddingCompletePayload;
use crate::settings;
use reqwest::Client;
use serde::{Deserialize, Serialize};
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

// OpenRouter Embeddings API types
#[derive(Serialize)]
struct OpenRouterEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct OpenRouterEmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// Generate embeddings via OpenRouter API (batch support)
pub async fn generate_openrouter_embeddings_public(
    client: &Client,
    api_key: &str,
    texts: &[String],
) -> Result<Vec<Vec<f32>>, String> {
    let request = OpenRouterEmbeddingRequest {
        model: "openai/text-embedding-3-small".to_string(),
        input: texts.to_vec(),
    };

    let response = client
        .post("https://openrouter.ai/api/v1/embeddings")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| format!("OpenRouter embeddings request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenRouter embeddings API error: {} - {}", status, body));
    }

    let embedding_response: OpenRouterEmbeddingResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenRouter embeddings response: {}", e))?;

    Ok(embedding_response.data.into_iter().map(|d| d.embedding).collect())
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
    let (auto_tagging_enabled, api_key, tagging_model, chunks) = {
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

        (auto_tagging_enabled, api_key, tagging_model, chunks)
    }; // Connection dropped here

    // Create HTTP client for OpenRouter API
    let client = Client::new();

    // Track all tag IDs and new tag IDs across all chunks
    let mut all_tag_ids: Vec<String> = Vec::new();
    let mut all_new_tag_ids: Vec<String> = Vec::new();

    // Generate all embeddings via OpenRouter in one batch
    let chunk_texts: Vec<String> = chunks.iter().map(|s| s.to_string()).collect();
    let embeddings = generate_openrouter_embeddings_public(&client, &api_key, &chunk_texts)
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
            match extract_tags_from_chunk(&client, &api_key, chunk_content, &tag_tree_json, &tagging_model).await {
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
        match consolidate_atom_tags(&client, &api_key, tag_info, &tagging_model).await {
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

