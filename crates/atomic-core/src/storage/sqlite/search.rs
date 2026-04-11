use std::collections::HashMap;

use super::SqliteStorage;
use crate::embedding::{distance_to_similarity, f32_vec_to_blob_public};
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::search;
use crate::storage::traits::*;
use async_trait::async_trait;

/// Sync helper methods for search operations.
impl SqliteStorage {
    pub(crate) fn vector_search_sync(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>> {
        let query_blob = f32_vec_to_blob_public(query_embedding);
        let conn = self.db.read_conn()?;
        let fetch_limit = limit * 10;

        let mut vec_stmt = conn
            .prepare(
                "SELECT chunk_id, distance
                 FROM vec_chunks
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2",
            )
            .map_err(|e| AtomicCoreError::Search(format!("Failed to prepare vec query: {}", e)))?;

        let similar_chunks: Vec<(String, f32)> = vec_stmt
            .query_map(rusqlite::params![&query_blob, fetch_limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| {
                AtomicCoreError::Search(format!("Failed to query similar chunks: {}", e))
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AtomicCoreError::Search(format!("Failed to collect similar chunks: {}", e))
            })?;

        // Filter by threshold
        let filtered: Vec<(String, f32)> = similar_chunks
            .into_iter()
            .filter(|(_, distance)| distance_to_similarity(*distance) >= threshold)
            .collect();

        // Batch-load chunk details
        let chunk_ids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();
        let chunk_map = batch_fetch_chunk_info(&conn, &chunk_ids)?;

        // Scope filtering
        let scope_tag_ids: Vec<String> = tag_id.map(|t| vec![t.to_string()]).unwrap_or_default();
        let scope_atom_ids: std::collections::HashSet<String> = if !scope_tag_ids.is_empty() {
            let candidate_atom_ids: Vec<&str> =
                chunk_map.values().map(|(aid, _, _)| aid.as_str()).collect();
            batch_atoms_with_scope_tags(&conn, &candidate_atom_ids, &scope_tag_ids)?
        } else {
            std::collections::HashSet::new()
        };

        // Deduplicate by atom_id, keeping best score
        let mut atom_best: HashMap<String, (f32, String, i32)> = HashMap::new();
        for (chunk_id, distance) in &filtered {
            let similarity = distance_to_similarity(*distance);
            if let Some((atom_id, content, chunk_index)) = chunk_map.get(chunk_id) {
                if !scope_tag_ids.is_empty() && !scope_atom_ids.contains(atom_id) {
                    continue;
                }
                let entry = atom_best.entry(atom_id.clone());
                match entry {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, content.clone(), *chunk_index));
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert((similarity, content.clone(), *chunk_index));
                    }
                }
            }
        }

        // Sort and limit
        let mut deduped: Vec<(String, f32, String, i32)> = atom_best
            .into_iter()
            .map(|(atom_id, (sim, content, idx))| (atom_id, sim, content, idx))
            .collect();
        deduped.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        deduped.truncate(limit as usize);

        // Batch fetch atom data
        let atom_ids: Vec<String> = deduped.iter().map(|(id, _, _, _)| id.clone()).collect();
        let atom_map = batch_fetch_atoms(&conn, &atom_ids)?;
        let tag_map = batch_fetch_tags(&conn, &atom_ids)?;

        let mut results = Vec::with_capacity(deduped.len());
        for (atom_id, similarity, content, chunk_index) in deduped {
            if let Some(atom) = atom_map.get(&atom_id) {
                let tags = tag_map.get(&atom_id).cloned().unwrap_or_default();
                results.push(SemanticSearchResult {
                    atom: AtomWithTags {
                        atom: atom.clone(),
                        tags,
                    },
                    similarity_score: similarity,
                    matching_chunk_content: content,
                    matching_chunk_index: chunk_index,
                });
            }
        }

        Ok(results)
    }

    pub(crate) fn keyword_search_sync(
        &self,
        query: &str,
        limit: i32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>> {
        let conn = self.db.read_conn()?;

        let escaped_query = escape_fts5_query(query);
        if escaped_query.is_empty() {
            return Ok(Vec::new());
        }
        let fetch_limit = limit * 5;

        let mut fts_stmt = conn
            .prepare(
                "SELECT id, atom_id, content, chunk_index, bm25(atom_chunks_fts) as score
                 FROM atom_chunks_fts
                 WHERE atom_chunks_fts MATCH ?1
                 ORDER BY bm25(atom_chunks_fts)
                 LIMIT ?2",
            )
            .map_err(|e| AtomicCoreError::Search(format!("Failed to prepare FTS query: {}", e)))?;

        let raw_results: Vec<(String, String, String, i32, f64)> = fts_stmt
            .query_map(rusqlite::params![&escaped_query, fetch_limit], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(|e| AtomicCoreError::Search(format!("Failed to query FTS: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                AtomicCoreError::Search(format!("Failed to collect FTS results: {}", e))
            })?;

        // Apply tag scope filter if specified
        let scope_tag_ids: Vec<String> = tag_id.map(|t| vec![t.to_string()]).unwrap_or_default();
        let filtered = if scope_tag_ids.is_empty() {
            raw_results
        } else {
            let candidate_atom_ids: Vec<&str> =
                raw_results.iter().map(|r| r.1.as_str()).collect();
            let matching =
                batch_atoms_with_scope_tags(&conn, &candidate_atom_ids, &scope_tag_ids)?;
            raw_results
                .into_iter()
                .filter(|r| matching.contains(r.1.as_str()))
                .collect()
        };

        // Deduplicate by atom_id, keeping best score
        let mut atom_best: HashMap<String, (f32, String, i32)> = HashMap::new();
        for (_chunk_id, atom_id, content, chunk_index, bm25_score) in &filtered {
            let score = normalize_bm25_score(*bm25_score);
            let entry = atom_best.entry(atom_id.clone());
            match entry {
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    if score > e.get().0 {
                        e.insert((score, content.clone(), *chunk_index));
                    }
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert((score, content.clone(), *chunk_index));
                }
            }
        }

        // Sort and limit
        let mut deduped: Vec<(String, f32, String, i32)> = atom_best
            .into_iter()
            .map(|(atom_id, (score, content, idx))| (atom_id, score, content, idx))
            .collect();
        deduped.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        deduped.truncate(limit as usize);

        // Batch fetch atom data
        let atom_ids: Vec<String> = deduped.iter().map(|(id, _, _, _)| id.clone()).collect();
        let atom_map = batch_fetch_atoms(&conn, &atom_ids)?;
        let tag_map = batch_fetch_tags(&conn, &atom_ids)?;

        let mut results = Vec::with_capacity(deduped.len());
        for (atom_id, score, content, chunk_index) in deduped {
            if let Some(atom) = atom_map.get(&atom_id) {
                let tags = tag_map.get(&atom_id).cloned().unwrap_or_default();
                results.push(SemanticSearchResult {
                    atom: AtomWithTags {
                        atom: atom.clone(),
                        tags,
                    },
                    similarity_score: score,
                    matching_chunk_content: content,
                    matching_chunk_index: chunk_index,
                });
            }
        }

        Ok(results)
    }

    pub(crate) fn keyword_search_chunks_sync(
        &self,
        query: &str,
        limit: i32,
        scope_tag_ids: &[String],
    ) -> StorageResult<Vec<ChunkSearchResult>> {
        let conn = self.db.read_conn()?;

        let escaped_query = escape_fts5_query(query);
        if escaped_query.is_empty() {
            return Ok(Vec::new());
        }
        let fetch_limit = limit * 3;

        let mut fts_stmt = conn
            .prepare(
                "SELECT id, atom_id, content, chunk_index, bm25(atom_chunks_fts) as score
                 FROM atom_chunks_fts
                 WHERE atom_chunks_fts MATCH ?1
                 ORDER BY bm25(atom_chunks_fts)
                 LIMIT ?2",
            )
            .map_err(|e| AtomicCoreError::Search(format!("Failed to prepare FTS query: {}", e)))?;

        let raw_results: Vec<(String, String, String, i32, f64)> = fts_stmt
            .query_map(rusqlite::params![&escaped_query, fetch_limit], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .map_err(|e| AtomicCoreError::Search(format!("Failed to query FTS: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AtomicCoreError::Search(format!("Failed to collect FTS results: {}", e)))?;

        // Apply tag scope filter if specified
        let filtered = if scope_tag_ids.is_empty() {
            raw_results
        } else {
            let candidate_atom_ids: Vec<&str> =
                raw_results.iter().map(|r| r.1.as_str()).collect();
            let matching =
                batch_atoms_with_scope_tags(&conn, &candidate_atom_ids, scope_tag_ids)?;
            raw_results
                .into_iter()
                .filter(|r| matching.contains(r.1.as_str()))
                .collect()
        };

        let results: Vec<ChunkSearchResult> = filtered
            .into_iter()
            .take(limit as usize)
            .map(|(chunk_id, atom_id, content, chunk_index, bm25_score)| {
                ChunkSearchResult {
                    chunk_id,
                    atom_id,
                    content,
                    chunk_index,
                    score: normalize_bm25_score(bm25_score),
                }
            })
            .collect();

        Ok(results)
    }

    pub(crate) fn vector_search_chunks_sync(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        scope_tag_ids: &[String],
    ) -> StorageResult<Vec<ChunkSearchResult>> {
        let query_blob = f32_vec_to_blob_public(query_embedding);
        let conn = self.db.read_conn()?;
        let fetch_limit = limit * 5;

        let mut vec_stmt = conn
            .prepare(
                "SELECT chunk_id, distance
                 FROM vec_chunks
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2",
            )
            .map_err(|e| AtomicCoreError::Search(format!("Failed to prepare vec query: {}", e)))?;

        let similar_chunks: Vec<(String, f32)> = vec_stmt
            .query_map(rusqlite::params![&query_blob, fetch_limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| AtomicCoreError::Search(format!("Failed to query similar chunks: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AtomicCoreError::Search(format!("Failed to collect similar chunks: {}", e)))?;

        // Filter by threshold
        let filtered: Vec<(String, f32)> = similar_chunks
            .into_iter()
            .filter(|(_, distance)| distance_to_similarity(*distance) >= threshold)
            .collect();

        // Batch-load chunk details
        let chunk_ids: Vec<String> = filtered.iter().map(|(id, _)| id.clone()).collect();
        let chunk_map = batch_fetch_chunk_info(&conn, &chunk_ids)?;

        // Apply tag scope filter
        let scope_atom_ids: std::collections::HashSet<String> = if !scope_tag_ids.is_empty() {
            let candidate_atom_ids: Vec<&str> =
                chunk_map.values().map(|(aid, _, _)| aid.as_str()).collect();
            batch_atoms_with_scope_tags(&conn, &candidate_atom_ids, scope_tag_ids)?
        } else {
            std::collections::HashSet::new()
        };

        let mut results = Vec::new();
        for (chunk_id, distance) in &filtered {
            let similarity = distance_to_similarity(*distance);
            if let Some((atom_id, content, chunk_index)) = chunk_map.get(chunk_id) {
                if !scope_tag_ids.is_empty() && !scope_atom_ids.contains(atom_id) {
                    continue;
                }
                results.push(ChunkSearchResult {
                    chunk_id: chunk_id.clone(),
                    atom_id: atom_id.clone(),
                    content: content.clone(),
                    chunk_index: *chunk_index,
                    score: similarity,
                });
            }
            if results.len() >= limit as usize {
                break;
            }
        }

        Ok(results)
    }

    pub(crate) fn find_similar_sync(
        &self,
        atom_id: &str,
        limit: i32,
        threshold: f32,
    ) -> StorageResult<Vec<SimilarAtomResult>> {
        let conn = self.db.read_conn()?;
        search::find_similar_atoms(&conn, atom_id, limit, threshold)
            .map_err(|e| AtomicCoreError::Search(e))
    }
}

#[async_trait]
impl SearchStore for SqliteStorage {
    async fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>> {
        let storage = self.clone();
        let query_embedding = query_embedding.to_vec();
        let tag_id = tag_id.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            storage.vector_search_sync(&query_embedding, limit, threshold, tag_id.as_deref())
        })
        .await
        .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn keyword_search(
        &self,
        query: &str,
        limit: i32,
        tag_id: Option<&str>,
    ) -> StorageResult<Vec<SemanticSearchResult>> {
        let storage = self.clone();
        let query = query.to_string();
        let tag_id = tag_id.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            storage.keyword_search_sync(&query, limit, tag_id.as_deref())
        })
        .await
        .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn find_similar(
        &self,
        atom_id: &str,
        limit: i32,
        threshold: f32,
    ) -> StorageResult<Vec<SimilarAtomResult>> {
        let storage = self.clone();
        let atom_id = atom_id.to_string();
        tokio::task::spawn_blocking(move || {
            storage.find_similar_sync(&atom_id, limit, threshold)
        })
        .await
        .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn keyword_search_chunks(
        &self,
        query: &str,
        limit: i32,
        scope_tag_ids: &[String],
    ) -> StorageResult<Vec<ChunkSearchResult>> {
        let storage = self.clone();
        let query = query.to_string();
        let scope_tag_ids = scope_tag_ids.to_vec();
        tokio::task::spawn_blocking(move || {
            storage.keyword_search_chunks_sync(&query, limit, &scope_tag_ids)
        })
        .await
        .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }

    async fn vector_search_chunks(
        &self,
        query_embedding: &[f32],
        limit: i32,
        threshold: f32,
        scope_tag_ids: &[String],
    ) -> StorageResult<Vec<ChunkSearchResult>> {
        let storage = self.clone();
        let query_embedding = query_embedding.to_vec();
        let scope_tag_ids = scope_tag_ids.to_vec();
        tokio::task::spawn_blocking(move || {
            storage.vector_search_chunks_sync(&query_embedding, limit, threshold, &scope_tag_ids)
        })
        .await
        .map_err(|e| AtomicCoreError::Lock(e.to_string()))?
    }
}

// ==================== Local Helper Functions ====================

/// Escape special characters for FTS5 MATCH query.
/// Wraps each word in quotes to treat them as literal terms.
fn escape_fts5_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            let cleaned = word.replace('"', "");
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("\"{}\"", cleaned)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Normalize BM25 score to 0-1 range.
/// BM25 scores are negative (lower = better), typically -30 to 0.
fn normalize_bm25_score(score: f64) -> f32 {
    let clamped = score.clamp(-30.0, 0.0);
    (1.0 - (clamped / -30.0) * 0.7) as f32
}

/// Batch fetch atoms by IDs in a single query.
fn batch_fetch_atoms(
    conn: &rusqlite::Connection,
    atom_ids: &[String],
) -> Result<HashMap<String, Atom>, AtomicCoreError> {
    if atom_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT {} FROM atoms WHERE id IN ({})",
        crate::ATOM_COLUMNS,
        placeholders
    );
    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(atom_ids.iter()), crate::atom_from_row)
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?;

    Ok(rows.into_iter().map(|a| (a.id.clone(), a)).collect())
}

/// Batch fetch tags for multiple atoms in a single query.
fn batch_fetch_tags(
    conn: &rusqlite::Connection,
    atom_ids: &[String],
) -> Result<HashMap<String, Vec<Tag>>, AtomicCoreError> {
    if atom_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT at.atom_id, t.id, t.name, t.parent_id, t.created_at, t.is_autotag_target
         FROM atom_tags at
         INNER JOIN tags t ON at.tag_id = t.id
         WHERE at.atom_id IN ({})",
        placeholders
    );
    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?;
    let mut map: HashMap<String, Vec<Tag>> = HashMap::new();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(atom_ids.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                Tag {
                    id: row.get(1)?,
                    name: row.get(2)?,
                    parent_id: row.get(3)?,
                    created_at: row.get(4)?,
                    is_autotag_target: row.get::<_, i32>(5)? != 0,
                },
            ))
        })
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?;
    for row in rows {
        let (atom_id, tag) = row.map_err(|e| AtomicCoreError::Search(e.to_string()))?;
        map.entry(atom_id).or_default().push(tag);
    }
    Ok(map)
}

/// Batch fetch chunk info by IDs in a single query.
fn batch_fetch_chunk_info(
    conn: &rusqlite::Connection,
    chunk_ids: &[String],
) -> Result<HashMap<String, (String, String, i32)>, AtomicCoreError> {
    if chunk_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!(
        "SELECT id, atom_id, content, chunk_index FROM atom_chunks WHERE id IN ({})",
        placeholders
    );
    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(chunk_ids.iter()), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
            ))
        })
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AtomicCoreError::Search(e.to_string()))?;

    Ok(rows
        .into_iter()
        .map(|(id, atom_id, content, idx)| (id, (atom_id, content, idx)))
        .collect())
}

/// Batch check which atom_ids have at least one of the specified scope tags.
fn batch_atoms_with_scope_tags(
    conn: &rusqlite::Connection,
    atom_ids: &[&str],
    scope_tag_ids: &[String],
) -> Result<std::collections::HashSet<String>, AtomicCoreError> {
    if atom_ids.is_empty() || scope_tag_ids.is_empty() {
        return Ok(std::collections::HashSet::new());
    }

    // Use recursive CTE to include atoms tagged with descendants of the scope tags
    let atom_placeholders: Vec<&str> = atom_ids.iter().map(|_| "?").collect();
    let tag_placeholders: Vec<&str> = scope_tag_ids.iter().map(|_| "?").collect();
    let query = format!(
        "WITH RECURSIVE scope_tags(id) AS (
            SELECT id FROM tags WHERE id IN ({tag_ph})
            UNION ALL
            SELECT t.id FROM tags t
            INNER JOIN scope_tags st ON t.parent_id = st.id
         )
         SELECT DISTINCT atom_id FROM atom_tags
         WHERE atom_id IN ({atom_ph}) AND tag_id IN (SELECT id FROM scope_tags)",
        tag_ph = tag_placeholders.join(","),
        atom_ph = atom_placeholders.join(","),
    );

    // Bind order matches SQL: tag_ids first (CTE), then atom_ids (WHERE)
    let mut params: Vec<&dyn rusqlite::ToSql> =
        Vec::with_capacity(atom_ids.len() + scope_tag_ids.len());
    for id in scope_tag_ids {
        params.push(id);
    }
    for id in atom_ids {
        params.push(id);
    }

    let mut stmt = conn
        .prepare(&query)
        .map_err(|e| AtomicCoreError::Search(format!("Failed to prepare scope query: {}", e)))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| {
            AtomicCoreError::Search(format!("Failed to execute scope query: {}", e))
        })?;

    let mut matching = std::collections::HashSet::new();
    for row in rows {
        matching.insert(
            row.map_err(|e| {
                AtomicCoreError::Search(format!("Failed to read scope result: {}", e))
            })?,
        );
    }
    Ok(matching)
}
