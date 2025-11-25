use crate::chunking::chunk_content;
use crate::db::Database;
use crate::extraction::{
    create_extracted_tags, extract_tags_from_chunk, get_tag_tree_json, link_tags_to_atom,
    merge_chunk_extractions, validate_tag_ids, ExtractionResult,
};
use crate::models::EmbeddingCompletePayload;
use crate::settings;
use reqwest::Client;
use std::sync::Arc;
use tauri::AppHandle;
use tauri::Emitter;
use uuid::Uuid;

/// Process embeddings for an atom asynchronously
/// This spawns a background task that:
/// 1. Sets embedding_status to 'processing'
/// 2. Chunks the content
/// 3. Generates real embeddings for each chunk using sqlite-lembed
/// 4. Extracts tags using OpenRouter LLM (if enabled)
/// 5. Stores chunks and embeddings in database
/// 6. Links extracted tags to the atom
/// 7. Sets embedding_status to 'complete' or 'failed'
/// 8. Emits 'embedding-complete' event with tag info
pub fn spawn_embedding_task(
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

async fn process_embeddings(
    db: &Database,
    atom_id: &str,
    content: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Set status to processing
    conn.execute(
        "UPDATE atoms SET embedding_status = 'processing' WHERE id = ?1",
        [atom_id],
    )
    .map_err(|e| e.to_string())?;

    // Get settings for auto-tagging
    let settings_map = settings::get_all_settings(&conn)?;
    let auto_tagging_enabled = settings_map
        .get("auto_tagging_enabled")
        .map(|v| v == "true")
        .unwrap_or(true); // Default to true
    let api_key = settings_map.get("openrouter_api_key").cloned();

    // Get tag tree for extraction context (if auto-tagging is enabled)
    let tag_tree_json = if auto_tagging_enabled && api_key.is_some() {
        get_tag_tree_json(&conn)?
    } else {
        String::new()
    };

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

    // Drop the connection lock before async operations
    drop(conn);

    // Create HTTP client for OpenRouter API
    let client = Client::new();

    // Collect extraction results
    let mut all_extraction_results: Vec<ExtractionResult> = Vec::new();

    // Process each chunk
    for (index, chunk_content) in chunks.iter().enumerate() {
        // Re-acquire connection for embedding generation
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        
        let chunk_id = Uuid::new_v4().to_string();

        // Generate REAL embedding using sqlite-lembed
        let embedding_blob: Vec<u8> = conn
            .query_row(
                "SELECT lembed('all-MiniLM-L6-v2', ?1)",
                [chunk_content],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to generate embedding: {}", e))?;

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

        // Drop connection before async extraction
        drop(conn);

        // Extract tags (if enabled and API key is set)
        if auto_tagging_enabled {
            if let Some(ref key) = api_key {
                match extract_tags_from_chunk(&client, key, chunk_content, &tag_tree_json).await {
                    Ok(result) => all_extraction_results.push(result),
                    Err(e) => {
                        // Log warning but continue - don't fail the whole process
                        eprintln!("Tag extraction failed for chunk {}: {}", index, e);
                    }
                }
            }
        }
    }

    // Re-acquire connection for final operations
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Merge extraction results and apply tags
    let mut all_tag_ids: Vec<String> = Vec::new();
    let mut new_tag_ids: Vec<String> = Vec::new();

    if !all_extraction_results.is_empty() {
        let merged = merge_chunk_extractions(all_extraction_results);

        // Validate existing tag IDs
        let valid_existing_ids = validate_tag_ids(&conn, &merged.existing_tag_ids);

        // Create new tags
        new_tag_ids = create_extracted_tags(&conn, merged.new_tags)?;

        // Combine all tag IDs
        all_tag_ids = valid_existing_ids
            .into_iter()
            .chain(new_tag_ids.clone())
            .collect();

        // Link tags to atom
        link_tags_to_atom(&conn, atom_id, &all_tag_ids)?;
    }

    // Set status to complete
    conn.execute(
        "UPDATE atoms SET embedding_status = 'complete' WHERE id = ?1",
        [atom_id],
    )
    .map_err(|e| e.to_string())?;

    Ok((all_tag_ids, new_tag_ids))
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

