use crate::db::{Database, SharedDatabase};
use crate::embedding::{distance_to_similarity, spawn_embedding_task};
use crate::models::{Atom, AtomPosition, AtomWithEmbedding, AtomWithTags, SemanticSearchResult, SimilarAtomResult, Tag, TagWithCount};
use crate::settings;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;

// Helper function to get tags for an atom
fn get_tags_for_atom(conn: &rusqlite::Connection, atom_id: &str) -> Result<Vec<Tag>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.parent_id, t.created_at 
             FROM tags t 
             INNER JOIN atom_tags at ON t.id = at.tag_id 
             WHERE at.atom_id = ?1",
        )
        .map_err(|e| e.to_string())?;

    let tags = stmt
        .query_map([atom_id], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(tags)
}

// Atom operations
#[tauri::command]
pub fn get_all_atoms(db: State<Database>) -> Result<Vec<AtomWithTags>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT id, content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending') FROM atoms ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let atoms: Vec<Atom> = stmt
        .query_map([], |row| {
            Ok(Atom {
                id: row.get(0)?,
                content: row.get(1)?,
                source_url: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                embedding_status: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for atom in atoms {
        let tags = get_tags_for_atom(&conn, &atom.id)?;
        result.push(AtomWithTags { atom, tags });
    }

    Ok(result)
}

#[tauri::command]
pub fn get_atom(db: State<Database>, id: String) -> Result<AtomWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let atom: Atom = conn
        .query_row(
            "SELECT id, content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
            [&id],
            |row| {
                Ok(Atom {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source_url: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    embedding_status: row.get(5)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let tags = get_tags_for_atom(&conn, &atom.id)?;
    Ok(AtomWithTags { atom, tags })
}

#[tauri::command]
pub fn create_atom(
    app_handle: tauri::AppHandle,
    db: State<Database>,
    shared_db: State<SharedDatabase>,
    content: String,
    source_url: Option<String>,
    tag_ids: Vec<String>,
) -> Result<AtomWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let embedding_status = "pending";

    conn.execute(
        "INSERT INTO atoms (id, content, source_url, created_at, updated_at, embedding_status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (&id, &content, &source_url, &now, &now, &embedding_status),
    )
    .map_err(|e| e.to_string())?;

    // Add tags
    for tag_id in &tag_ids {
        conn.execute(
            "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
            (&id, tag_id),
        )
        .map_err(|e| e.to_string())?;
    }

    let atom = Atom {
        id: id.clone(),
        content: content.clone(),
        source_url,
        created_at: now.clone(),
        updated_at: now,
        embedding_status: embedding_status.to_string(),
    };

    let tags = get_tags_for_atom(&conn, &id)?;
    
    // Drop the connection lock before spawning the embedding task
    drop(conn);
    
    // Spawn embedding task (non-blocking)
    spawn_embedding_task(
        app_handle,
        Arc::clone(&shared_db),
        id,
        content,
    );

    Ok(AtomWithTags { atom, tags })
}

#[tauri::command]
pub fn update_atom(
    app_handle: tauri::AppHandle,
    db: State<Database>,
    shared_db: State<SharedDatabase>,
    id: String,
    content: String,
    source_url: Option<String>,
    tag_ids: Vec<String>,
) -> Result<AtomWithTags, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let now = Utc::now().to_rfc3339();
    let embedding_status = "pending"; // Reset to pending when content changes

    conn.execute(
        "UPDATE atoms SET content = ?1, source_url = ?2, updated_at = ?3, embedding_status = ?4 WHERE id = ?5",
        (&content, &source_url, &now, &embedding_status, &id),
    )
    .map_err(|e| e.to_string())?;

    // Remove existing tags and add new ones
    conn.execute("DELETE FROM atom_tags WHERE atom_id = ?1", [&id])
        .map_err(|e| e.to_string())?;

    for tag_id in &tag_ids {
        conn.execute(
            "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
            (&id, tag_id),
        )
        .map_err(|e| e.to_string())?;
    }

    // Get the updated atom
    let atom: Atom = conn
        .query_row(
            "SELECT id, content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
            [&id],
            |row| {
                Ok(Atom {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source_url: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                    embedding_status: row.get(5)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    let tags = get_tags_for_atom(&conn, &id)?;
    
    // Drop the connection lock before spawning the embedding task
    drop(conn);
    
    // Spawn embedding task (non-blocking)
    spawn_embedding_task(
        app_handle,
        Arc::clone(&shared_db),
        id,
        content,
    );

    Ok(AtomWithTags { atom, tags })
}

#[tauri::command]
pub fn delete_atom(db: State<Database>, id: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM atoms WHERE id = ?1", [&id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

// Tag operations
#[tauri::command]
pub fn get_all_tags(db: State<Database>) -> Result<Vec<TagWithCount>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Get all tags with their atom counts
    let mut stmt = conn
        .prepare(
            "SELECT t.id, t.name, t.parent_id, t.created_at, 
                    (SELECT COUNT(*) FROM atom_tags WHERE tag_id = t.id) as atom_count
             FROM tags t
             ORDER BY t.name",
        )
        .map_err(|e| e.to_string())?;

    let tags: Vec<(Tag, i32)> = stmt
        .query_map([], |row| {
            Ok((
                Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    created_at: row.get(3)?,
                },
                row.get(4)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    // Build hierarchical structure
    fn build_tree(tags: &[(Tag, i32)], parent_id: Option<&str>) -> Vec<TagWithCount> {
        tags.iter()
            .filter(|(tag, _)| tag.parent_id.as_deref() == parent_id)
            .map(|(tag, count)| TagWithCount {
                tag: tag.clone(),
                atom_count: *count,
                children: build_tree(tags, Some(&tag.id)),
            })
            .collect()
    }

    Ok(build_tree(&tags, None))
}

#[tauri::command]
pub fn create_tag(
    db: State<Database>,
    name: String,
    parent_id: Option<String>,
) -> Result<Tag, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        (&id, &name, &parent_id, &now),
    )
    .map_err(|e| e.to_string())?;

    Ok(Tag {
        id,
        name,
        parent_id,
        created_at: now,
    })
}

#[tauri::command]
pub fn update_tag(
    db: State<Database>,
    id: String,
    name: String,
    parent_id: Option<String>,
) -> Result<Tag, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    conn.execute(
        "UPDATE tags SET name = ?1, parent_id = ?2 WHERE id = ?3",
        (&name, &parent_id, &id),
    )
    .map_err(|e| e.to_string())?;

    // Get the updated tag
    let tag: Tag = conn
        .query_row(
            "SELECT id, name, parent_id, created_at FROM tags WHERE id = ?1",
            [&id],
            |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
        .map_err(|e| e.to_string())?;

    Ok(tag)
}

#[tauri::command]
pub fn delete_tag(db: State<Database>, id: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    conn.execute("DELETE FROM tags WHERE id = ?1", [&id])
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn get_atoms_by_tag(db: State<Database>, tag_id: String) -> Result<Vec<AtomWithTags>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let mut stmt = conn
        .prepare(
            "SELECT a.id, a.content, a.source_url, a.created_at, a.updated_at, COALESCE(a.embedding_status, 'pending')
             FROM atoms a
             INNER JOIN atom_tags at ON a.id = at.atom_id
             WHERE at.tag_id = ?1
             ORDER BY a.updated_at DESC",
        )
        .map_err(|e| e.to_string())?;

    let atoms: Vec<Atom> = stmt
        .query_map([&tag_id], |row| {
            Ok(Atom {
                id: row.get(0)?,
                content: row.get(1)?,
                source_url: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                embedding_status: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    let mut result = Vec::new();
    for atom in atoms {
        let tags = get_tags_for_atom(&conn, &atom.id)?;
        result.push(AtomWithTags { atom, tags });
    }

    Ok(result)
}

// sqlite-vec verification command
#[tauri::command]
pub fn check_sqlite_vec(db: State<Database>) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let version: String = conn
        .query_row("SELECT vec_version()", [], |row| row.get(0))
        .map_err(|e| format!("sqlite-vec not loaded: {}", e))?;

    Ok(version)
}

// Embedding-related commands

/// Find similar atoms based on vector similarity
/// 1. Get all chunks for the given atom
/// 2. For each chunk, find similar chunks in vec_chunks
/// 3. Filter by threshold (convert distance to similarity)
/// 4. Deduplicate by parent atom_id, keep highest similarity
/// 5. Exclude the source atom itself
/// 6. Return up to `limit` results
#[tauri::command]
pub fn find_similar_atoms(
    db: State<Database>,
    atom_id: String,
    limit: i32,
    threshold: f32,
) -> Result<Vec<SimilarAtomResult>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // 1. Get all chunks for the given atom
    let mut stmt = conn
        .prepare("SELECT id, embedding FROM atom_chunks WHERE atom_id = ?1")
        .map_err(|e| format!("Failed to prepare chunk query: {}", e))?;

    let source_chunks: Vec<(String, Vec<u8>)> = stmt
        .query_map([&atom_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("Failed to query chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect chunks: {}", e))?;

    if source_chunks.is_empty() {
        return Ok(Vec::new());
    }

    // Map to store best similarity per atom_id
    let mut atom_similarities: HashMap<String, (f32, String, i32)> = HashMap::new();

    // 2. For each source chunk, find similar chunks
    for (_, embedding_blob) in &source_chunks {
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
            .query_map(rusqlite::params![embedding_blob, limit * 10], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query similar chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

        // 3. For each similar chunk, get its parent atom and check threshold
        for (chunk_id, distance) in similar_chunks {
            let similarity = distance_to_similarity(distance);

            if similarity < threshold {
                continue;
            }

            // Get the parent atom_id and chunk info for this chunk
            let chunk_info: Result<(String, String, i32), _> = conn.query_row(
                "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
                [&chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            );

            if let Ok((parent_atom_id, chunk_content, chunk_index)) = chunk_info {
                // 5. Exclude the source atom itself
                if parent_atom_id == atom_id {
                    continue;
                }

                // 4. Keep highest similarity per atom
                let entry = atom_similarities.entry(parent_atom_id.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, chunk_content, chunk_index));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((similarity, chunk_content, chunk_index));
                    }
                }
            }
        }
    }

    // 6. Build results, sorted by similarity
    let mut results: Vec<(String, f32, String, i32)> = atom_similarities
        .into_iter()
        .map(|(atom_id, (sim, content, idx))| (atom_id, sim, content, idx))
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit as usize);

    // Fetch full atom data for results
    let mut final_results = Vec::new();
    for (result_atom_id, similarity, chunk_content, chunk_index) in results {
        let atom: Atom = conn
            .query_row(
                "SELECT id, content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
                [&result_atom_id],
                |row| {
                    Ok(Atom {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source_url: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        embedding_status: row.get(5)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to get atom: {}", e))?;

        let tags = get_tags_for_atom(&conn, &result_atom_id)?;

        final_results.push(SimilarAtomResult {
            atom: AtomWithTags { atom, tags },
            similarity_score: similarity,
            matching_chunk_content: chunk_content,
            matching_chunk_index: chunk_index,
        });
    }

    Ok(final_results)
}

/// Search atoms semantically using a query string
/// 1. Generate embedding for query text using sqlite-lembed
/// 2. Search vec_chunks for similar chunks
/// 3. Filter by threshold
/// 4. Deduplicate by parent atom_id
/// 5. Return atoms with matching chunk content
#[tauri::command]
pub fn search_atoms_semantic(
    db: State<Database>,
    query: String,
    limit: i32,
    threshold: f32,
) -> Result<Vec<SemanticSearchResult>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // 1. Generate REAL embedding for query using sqlite-lembed
    let query_blob: Vec<u8> = conn
        .query_row(
            "SELECT lembed('all-MiniLM-L6-v2', ?1)",
            [&query],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to generate query embedding: {}", e))?;

    // 2. Search vec_chunks for similar chunks
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
        .query_map(rusqlite::params![&query_blob, limit * 10], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("Failed to query similar chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

    // Map to store best similarity per atom_id
    let mut atom_similarities: HashMap<String, (f32, String, i32)> = HashMap::new();

    // 3. Filter by threshold and deduplicate
    for (chunk_id, distance) in similar_chunks {
        let similarity = distance_to_similarity(distance);

        if similarity < threshold {
            continue;
        }

        // Get the parent atom_id and chunk info
        let chunk_info: Result<(String, String, i32), _> = conn.query_row(
            "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
            [&chunk_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        if let Ok((parent_atom_id, chunk_content, chunk_index)) = chunk_info {
            // 4. Keep highest similarity per atom
            let entry = atom_similarities.entry(parent_atom_id.clone());
            match entry {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if similarity > e.get().0 {
                        e.insert((similarity, chunk_content, chunk_index));
                    }
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert((similarity, chunk_content, chunk_index));
                }
            }
        }
    }

    // 5. Build results, sorted by similarity
    let mut results: Vec<(String, f32, String, i32)> = atom_similarities
        .into_iter()
        .map(|(atom_id, (sim, content, idx))| (atom_id, sim, content, idx))
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit as usize);

    // Fetch full atom data for results
    let mut final_results = Vec::new();
    for (result_atom_id, similarity, chunk_content, chunk_index) in results {
        let atom: Atom = conn
            .query_row(
                "SELECT id, content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
                [&result_atom_id],
                |row| {
                    Ok(Atom {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source_url: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        embedding_status: row.get(5)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to get atom: {}", e))?;

        let tags = get_tags_for_atom(&conn, &result_atom_id)?;

        final_results.push(SemanticSearchResult {
            atom: AtomWithTags { atom, tags },
            similarity_score: similarity,
            matching_chunk_content: chunk_content,
            matching_chunk_index: chunk_index,
        });
    }

    Ok(final_results)
}

/// Retry embedding generation for a failed atom
/// Reset status to 'pending' and trigger embedding again
#[tauri::command]
pub fn retry_embedding(
    app_handle: tauri::AppHandle,
    db: State<Database>,
    shared_db: State<SharedDatabase>,
    atom_id: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    // Get the atom content
    let content: String = conn
        .query_row(
            "SELECT content FROM atoms WHERE id = ?1",
            [&atom_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get atom: {}", e))?;

    // Reset status to pending
    conn.execute(
        "UPDATE atoms SET embedding_status = 'pending' WHERE id = ?1",
        [&atom_id],
    )
    .map_err(|e| e.to_string())?;

    // Drop the connection lock before spawning the embedding task
    drop(conn);

    // Spawn embedding task
    spawn_embedding_task(app_handle, Arc::clone(&shared_db), atom_id, content);

    Ok(())
}

/// Trigger embedding generation for all atoms with 'pending' status
#[tauri::command]
pub fn process_pending_embeddings(
    app_handle: tauri::AppHandle,
    db: State<Database>,
    shared_db: State<SharedDatabase>,
) -> Result<i32, String> {
    // Scope the database access to release the lock before spawning tasks
    let pending_atoms: Vec<(String, String)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        // Get all atoms with pending status
        let mut stmt = conn
            .prepare("SELECT id, content FROM atoms WHERE embedding_status = 'pending'")
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let result = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(|e| format!("Failed to query pending atoms: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect pending atoms: {}", e))?;
        result
    };

    let count = pending_atoms.len() as i32;

    // Spawn embedding tasks for each pending atom
    for (atom_id, content) in pending_atoms {
        spawn_embedding_task(
            app_handle.clone(),
            Arc::clone(&shared_db),
            atom_id,
            content,
        );
    }

    Ok(count)
}

/// Get the embedding status for an atom
#[tauri::command]
pub fn get_embedding_status(db: State<Database>, atom_id: String) -> Result<String, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let status: String = conn
        .query_row(
            "SELECT COALESCE(embedding_status, 'pending') FROM atoms WHERE id = ?1",
            [&atom_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to get embedding status: {}", e))?;

    Ok(status)
}

// Settings commands

#[tauri::command]
pub fn get_settings(db: State<Database>) -> Result<HashMap<String, String>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    settings::get_all_settings(&conn)
}

#[tauri::command]
pub fn set_setting(db: State<Database>, key: String, value: String) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    settings::set_setting(&conn, &key, &value)
}

#[tauri::command]
pub async fn test_openrouter_connection(api_key: String) -> Result<bool, String> {
    let client = reqwest::Client::new();
    
    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "anthropic/claude-haiku-4.5",
            "messages": [
                {
                    "role": "user",
                    "content": "Hi"
                }
            ],
            "max_tokens": 5
        }))
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    
    if response.status().is_success() {
        Ok(true)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(format!("API error ({}): {}", status, body))
    }
}

// Wiki commands

use crate::wiki;

/// Get a wiki article for a tag (if it exists)
#[tauri::command]
pub fn get_wiki_article(
    db: State<Database>,
    tag_id: String,
) -> Result<Option<crate::models::WikiArticleWithCitations>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::load_wiki_article(&conn, &tag_id)
}

/// Get the status of a wiki article for a tag
#[tauri::command]
pub fn get_wiki_article_status(
    db: State<Database>,
    tag_id: String,
) -> Result<crate::models::WikiArticleStatus, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::get_article_status(&conn, &tag_id)
}

/// Generate a new wiki article for a tag
#[tauri::command]
pub async fn generate_wiki_article(
    db: State<'_, Database>,
    tag_id: String,
    tag_name: String,
) -> Result<crate::models::WikiArticleWithCitations, String> {
    // Get settings and prepare data (sync, with db lock)
    let (api_key, input) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = settings::get_all_settings(&conn)?;
        let api_key = settings_map.get("openrouter_api_key").cloned();
        let input = wiki::prepare_wiki_generation(&conn, &tag_id, &tag_name)?;
        (api_key, input)
    };
    // Lock released here

    let api_key = api_key.ok_or("OpenRouter API key not configured. Please set it in Settings.")?;

    // Generate article via API (async, no db lock needed)
    let client = reqwest::Client::new();
    let result = wiki::generate_wiki_content(&client, &api_key, &input).await?;

    // Save to database (sync, with db lock)
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        wiki::save_wiki_article(&conn, &result.article, &result.citations)?;
    }

    Ok(result)
}

/// Update an existing wiki article with new atoms
#[tauri::command]
pub async fn update_wiki_article(
    db: State<'_, Database>,
    tag_id: String,
    tag_name: String,
) -> Result<crate::models::WikiArticleWithCitations, String> {
    // Get settings, existing article, and prepare update data (sync, with db lock)
    let (api_key, existing, update_input) = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = settings::get_all_settings(&conn)?;
        let api_key = settings_map.get("openrouter_api_key").cloned();
        let existing = wiki::load_wiki_article(&conn, &tag_id)?;
        
        let update_input = if let Some(ref ex) = existing {
            wiki::prepare_wiki_update(&conn, &tag_id, &tag_name, &ex.article, &ex.citations)?
        } else {
            None
        };
        
        (api_key, existing, update_input)
    };
    // Lock released here

    let api_key = api_key.ok_or("OpenRouter API key not configured. Please set it in Settings.")?;
    let existing = existing.ok_or("No existing article to update")?;

    // Check if there are new atoms to incorporate
    let input = match update_input {
        Some(input) => input,
        None => {
            // No new atoms, return existing article unchanged
            return Ok(existing);
        }
    };

    // Update article via API (async, no db lock needed)
    let client = reqwest::Client::new();
    let result = wiki::update_wiki_content(&client, &api_key, &input).await?;

    // Save to database (sync, with db lock)
    {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        wiki::save_wiki_article(&conn, &result.article, &result.citations)?;
    }

    Ok(result)
}

/// Delete a wiki article for a tag
#[tauri::command]
pub fn delete_wiki_article(
    db: State<Database>,
    tag_id: String,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    wiki::delete_article(&conn, &tag_id)
}

// Canvas commands

/// Get all stored atom positions from the database
#[tauri::command]
pub fn get_atom_positions(db: State<Database>) -> Result<Vec<AtomPosition>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    let mut stmt = conn
        .prepare("SELECT atom_id, x, y FROM atom_positions")
        .map_err(|e| e.to_string())?;
    
    let positions = stmt
        .query_map([], |row| {
            Ok(AtomPosition {
                atom_id: row.get(0)?,
                x: row.get(1)?,
                y: row.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    Ok(positions)
}

/// Bulk save/update positions after simulation completes
#[tauri::command]
pub fn save_atom_positions(
    db: State<Database>,
    positions: Vec<AtomPosition>,
) -> Result<(), String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let now = Utc::now().to_rfc3339();
    
    for pos in positions {
        conn.execute(
            "INSERT OR REPLACE INTO atom_positions (atom_id, x, y, updated_at) VALUES (?1, ?2, ?3, ?4)",
            (&pos.atom_id, &pos.x, &pos.y, &now),
        )
        .map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

/// Get atoms with their average embedding vector for similarity calculations
#[tauri::command]
pub fn get_atoms_with_embeddings(db: State<Database>) -> Result<Vec<AtomWithEmbedding>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    
    // First get all atoms with tags
    let mut stmt = conn
        .prepare(
            "SELECT id, content, source_url, created_at, updated_at, COALESCE(embedding_status, 'pending') FROM atoms ORDER BY updated_at DESC",
        )
        .map_err(|e| e.to_string())?;
    
    let atoms: Vec<Atom> = stmt
        .query_map([], |row| {
            Ok(Atom {
                id: row.get(0)?,
                content: row.get(1)?,
                source_url: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                embedding_status: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    let mut result = Vec::new();
    for atom in atoms {
        let tags = get_tags_for_atom(&conn, &atom.id)?;
        
        // Get average embedding for this atom
        let embedding = get_average_embedding(&conn, &atom.id)?;
        
        result.push(AtomWithEmbedding {
            atom: AtomWithTags { atom, tags },
            embedding,
        });
    }
    
    Ok(result)
}

/// Helper function to calculate average embedding from all chunks of an atom
fn get_average_embedding(conn: &rusqlite::Connection, atom_id: &str) -> Result<Option<Vec<f32>>, String> {
    let mut stmt = conn
        .prepare("SELECT embedding FROM atom_chunks WHERE atom_id = ?1 AND embedding IS NOT NULL")
        .map_err(|e| e.to_string())?;
    
    let embeddings: Vec<Vec<u8>> = stmt
        .query_map([atom_id], |row| row.get(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    
    if embeddings.is_empty() {
        return Ok(None);
    }
    
    // Convert blob embeddings to f32 vectors and average them
    // Each embedding is 384 dimensions * 4 bytes = 1536 bytes
    let dimension = 384;
    let mut avg_embedding = vec![0.0f32; dimension];
    let count = embeddings.len() as f32;
    
    for blob in &embeddings {
        if blob.len() != dimension * 4 {
            continue; // Skip malformed embeddings
        }
        
        for i in 0..dimension {
            let bytes: [u8; 4] = [
                blob[i * 4],
                blob[i * 4 + 1],
                blob[i * 4 + 2],
                blob[i * 4 + 3],
            ];
            avg_embedding[i] += f32::from_le_bytes(bytes);
        }
    }
    
    // Divide by count to get average
    for val in &mut avg_embedding {
        *val /= count;
    }
    
    Ok(Some(avg_embedding))
}

