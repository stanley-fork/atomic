//! Unified search module for Atomic
//!
//! Provides a single search implementation that supports keyword (BM25), semantic (vector),
//! and hybrid (RRF-combined) search modes. Used by UI, chat agent, MCP, and wiki generation.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::embedding::{distance_to_similarity, f32_vec_to_blob_public};
use crate::models::{Atom, AtomWithTags, SemanticSearchResult, SimilarAtomResult, Tag};
use crate::providers::{get_embedding_provider, EmbeddingConfig, ProviderConfig};
use crate::settings::get_all_settings;

/// Search mode - determines which search algorithm(s) to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchMode {
    /// BM25 keyword search using FTS5
    Keyword,
    /// Vector similarity search using embeddings
    Semantic,
    /// Combines keyword and semantic using Reciprocal Rank Fusion
    Hybrid,
}

/// Options for search queries
#[derive(Debug, Clone)]
pub struct SearchOptions {
    /// The search query text
    pub query: String,
    /// Search algorithm to use
    pub mode: SearchMode,
    /// Maximum number of results to return
    pub limit: i32,
    /// Minimum similarity threshold (0.0-1.0) for semantic/hybrid modes
    pub threshold: f32,
    /// Optional tag IDs to filter results (only return atoms with these tags)
    pub scope_tag_ids: Vec<String>,
}

impl SearchOptions {
    pub fn new(query: impl Into<String>, mode: SearchMode, limit: i32) -> Self {
        Self {
            query: query.into(),
            mode,
            limit,
            threshold: 0.3,
            scope_tag_ids: vec![],
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = threshold;
        self
    }

    pub fn with_scope(mut self, tag_ids: Vec<String>) -> Self {
        self.scope_tag_ids = tag_ids;
        self
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            mode: SearchMode::Hybrid,
            limit: 10,
            threshold: 0.3,
            scope_tag_ids: vec![],
        }
    }
}

/// A single chunk result from search
#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub chunk_id: String,
    pub atom_id: String,
    pub content: String,
    pub chunk_index: i32,
    /// Normalized score (0.0-1.0), higher is better
    pub score: f32,
}

/// RRF constant - standard value that prevents high ranks from dominating
const RRF_K: f32 = 60.0;

/// Escape special characters for FTS5 MATCH query
/// Wraps each word in quotes to treat them as literal terms
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

/// Normalize BM25 score to 0-1 range
/// BM25 scores are negative (lower = better), typically -30 to 0
fn normalize_bm25_score(score: f64) -> f32 {
    let clamped = score.clamp(-30.0, 0.0);
    (1.0 - (clamped / -30.0) * 0.7) as f32
}

/// Core search function - returns raw chunks without atom deduplication
///
/// Use this when you need multiple chunks per atom (e.g., wiki generation).
/// For most UI cases, use `search_atoms()` instead.
pub async fn search_chunks(db: &Database, options: SearchOptions) -> Result<Vec<ChunkResult>, String> {
    match options.mode {
        SearchMode::Keyword => search_keyword_chunks(db, &options).await,
        SearchMode::Semantic => search_semantic_chunks(db, &options).await,
        SearchMode::Hybrid => search_hybrid_chunks(db, &options).await,
    }
}

/// Search and return deduplicated atoms with full data
///
/// This is the main entry point for UI/chat/MCP search. Returns one result per atom
/// with the best-matching chunk info attached.
pub async fn search_atoms(
    db: &Database,
    options: SearchOptions,
) -> Result<Vec<SemanticSearchResult>, String> {
    // Get raw chunk results
    let chunks = search_chunks(db, options.clone()).await?;

    // Deduplicate by atom_id, keeping highest score per atom
    let mut atom_best: HashMap<String, ChunkResult> = HashMap::new();
    for chunk in chunks {
        let entry = atom_best.entry(chunk.atom_id.clone());
        match entry {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if chunk.score > e.get().score {
                    e.insert(chunk);
                }
            }
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(chunk);
            }
        }
    }

    // Sort by score descending and limit
    let mut deduped: Vec<ChunkResult> = atom_best.into_values().collect();
    deduped.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    deduped.truncate(options.limit as usize);

    // Fetch full atom data
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let mut results = Vec::with_capacity(deduped.len());

    for chunk in deduped {
        let atom: Atom = conn
            .query_row(
                "SELECT id, content, source_url, created_at, updated_at,
                 COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
                 FROM atoms WHERE id = ?1",
                [&chunk.atom_id],
                |row| {
                    Ok(Atom {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source_url: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        embedding_status: row.get(5)?,
                        tagging_status: row.get(6)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to get atom: {}", e))?;

        let tags = get_tags_for_atom(&conn, &chunk.atom_id)?;

        results.push(SemanticSearchResult {
            atom: AtomWithTags { atom, tags },
            similarity_score: chunk.score,
            matching_chunk_content: chunk.content,
            matching_chunk_index: chunk.chunk_index,
        });
    }

    Ok(results)
}

/// Get tags for an atom
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

/// Keyword search using FTS5/BM25
async fn search_keyword_chunks(
    db: &Database,
    options: &SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    let conn = db.conn.lock().map_err(|e| e.to_string())?;

    let mut fts_stmt = conn
        .prepare(
            "SELECT chunk_id, atom_id, content, chunk_index, bm25(atom_chunks_fts) as score
             FROM atom_chunks_fts
             WHERE atom_chunks_fts MATCH ?1
             ORDER BY bm25(atom_chunks_fts)
             LIMIT ?2",
        )
        .map_err(|e| format!("Failed to prepare FTS query: {}", e))?;

    let escaped_query = escape_fts5_query(&options.query);
    let fetch_limit = options.limit * 5; // Fetch extra for filtering

    let raw_results: Vec<(String, String, String, i32, f64)> = fts_stmt
        .query_map(rusqlite::params![&escaped_query, fetch_limit], |row| {
            Ok((
                row.get(0)?, // chunk_id
                row.get(1)?, // atom_id
                row.get(2)?, // content
                row.get(3)?, // chunk_index
                row.get(4)?, // BM25 score
            ))
        })
        .map_err(|e| format!("Failed to query FTS: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect FTS results: {}", e))?;

    // Apply tag scope filter if specified
    let filtered = filter_by_scope(&conn, raw_results, &options.scope_tag_ids)?;

    // Convert to ChunkResult with normalized scores
    Ok(filtered
        .into_iter()
        .map(|(chunk_id, atom_id, content, chunk_index, bm25_score)| ChunkResult {
            chunk_id,
            atom_id,
            content,
            chunk_index,
            score: normalize_bm25_score(bm25_score),
        })
        .collect())
}

/// Semantic search using vector similarity
async fn search_semantic_chunks(
    db: &Database,
    options: &SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    // Get provider config from settings
    let provider_config = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = get_all_settings(&conn).map_err(|e| e.to_string())?;
        ProviderConfig::from_settings(&settings_map)
    };

    // Create embedding provider and generate query embedding
    let provider = get_embedding_provider(&provider_config)
        .map_err(|e| format!("Failed to create embedding provider: {}", e))?;
    let embedding_config = EmbeddingConfig::new(provider_config.embedding_model());
    let embeddings = provider
        .embed_batch(&[options.query.clone()], &embedding_config)
        .await
        .map_err(|e| format!("Failed to generate query embedding: {}", e))?;

    let query_blob = f32_vec_to_blob_public(&embeddings[0]);

    // Query vec_chunks
    let conn = db.conn.lock().map_err(|e| e.to_string())?;
    let fetch_limit = options.limit * 10;

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
        .query_map(rusqlite::params![&query_blob, fetch_limit], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .map_err(|e| format!("Failed to query similar chunks: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

    // Get chunk details and filter
    let mut results = Vec::new();
    for (chunk_id, distance) in similar_chunks {
        let similarity = distance_to_similarity(distance);

        if similarity < options.threshold {
            continue;
        }

        let chunk_info: Result<(String, String, i32), _> = conn.query_row(
            "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
            [&chunk_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        if let Ok((atom_id, content, chunk_index)) = chunk_info {
            // Check tag scope if specified
            if !options.scope_tag_ids.is_empty()
                && !atom_has_scope_tag(&conn, &atom_id, &options.scope_tag_ids)?
            {
                continue;
            }

            results.push(ChunkResult {
                chunk_id,
                atom_id,
                content,
                chunk_index,
                score: similarity,
            });
        }
    }

    Ok(results)
}

/// Hybrid search combining keyword and semantic with RRF
async fn search_hybrid_chunks(
    db: &Database,
    options: &SearchOptions,
) -> Result<Vec<ChunkResult>, String> {
    // Get provider config from settings
    let provider_config = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;
        let settings_map = get_all_settings(&conn).map_err(|e| e.to_string())?;
        ProviderConfig::from_settings(&settings_map)
    };

    let fetch_limit = options.limit * 5;

    // Phase 1: Keyword search
    let keyword_results: Vec<(String, String, String, i32)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

        let mut fts_stmt = conn
            .prepare(
                "SELECT chunk_id, atom_id, content, chunk_index
                 FROM atom_chunks_fts
                 WHERE atom_chunks_fts MATCH ?1
                 ORDER BY bm25(atom_chunks_fts)
                 LIMIT ?2",
            )
            .map_err(|e| format!("Failed to prepare FTS query: {}", e))?;

        let escaped_query = escape_fts5_query(&options.query);

        let results: Vec<(String, String, String, i32)> = fts_stmt
            .query_map(rusqlite::params![&escaped_query, fetch_limit], |row| {
                Ok((
                    row.get::<_, String>(0)?, // chunk_id
                    row.get::<_, String>(1)?, // atom_id
                    row.get::<_, String>(2)?, // content
                    row.get::<_, i32>(3)?,    // chunk_index
                ))
            })
            .map_err(|e| format!("Failed to query FTS: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect FTS results: {}", e))?;

        // Filter by scope
        if !options.scope_tag_ids.is_empty() {
            results
                .into_iter()
                .filter(|(_, atom_id, _, _)| {
                    atom_has_scope_tag(&conn, atom_id, &options.scope_tag_ids).unwrap_or(false)
                })
                .collect()
        } else {
            results
        }
    };

    // Phase 2: Semantic search
    let provider = get_embedding_provider(&provider_config)
        .map_err(|e| format!("Failed to create embedding provider: {}", e))?;
    let embedding_config = EmbeddingConfig::new(provider_config.embedding_model());
    let embeddings = provider
        .embed_batch(&[options.query.clone()], &embedding_config)
        .await
        .map_err(|e| format!("Failed to generate query embedding: {}", e))?;

    let query_blob = f32_vec_to_blob_public(&embeddings[0]);

    let semantic_results: Vec<(String, String, String, i32, f32)> = {
        let conn = db.conn.lock().map_err(|e| e.to_string())?;

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
            .query_map(rusqlite::params![&query_blob, fetch_limit], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .map_err(|e| format!("Failed to query similar chunks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect similar chunks: {}", e))?;

        let mut results = Vec::new();
        for (chunk_id, distance) in similar_chunks {
            let similarity = distance_to_similarity(distance);

            if similarity < options.threshold {
                continue;
            }

            let chunk_info: Result<(String, String, i32), _> = conn.query_row(
                "SELECT atom_id, content, chunk_index FROM atom_chunks WHERE id = ?1",
                [&chunk_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            );

            if let Ok((atom_id, content, chunk_index)) = chunk_info {
                if !options.scope_tag_ids.is_empty()
                    && !atom_has_scope_tag(&conn, &atom_id, &options.scope_tag_ids)?
                {
                    continue;
                }

                results.push((chunk_id, atom_id, content, chunk_index, similarity));
            }
        }
        results
    };

    // Phase 3: Combine with RRF
    // RRF score = 1/(k + rank) summed across result sets
    let mut chunk_scores: HashMap<String, (f32, String, String, i32)> = HashMap::new();

    // Add keyword results with RRF scores
    for (rank, (chunk_id, atom_id, content, chunk_index)) in keyword_results.iter().enumerate() {
        let rrf = 1.0 / (RRF_K + (rank + 1) as f32);
        chunk_scores.insert(
            chunk_id.clone(),
            (rrf, atom_id.clone(), content.clone(), *chunk_index),
        );
    }

    // Add semantic results with RRF scores
    for (rank, (chunk_id, atom_id, content, chunk_index, _)) in semantic_results.iter().enumerate()
    {
        let rrf = 1.0 / (RRF_K + (rank + 1) as f32);
        chunk_scores
            .entry(chunk_id.clone())
            .and_modify(|(score, _, _, _)| *score += rrf)
            .or_insert((rrf, atom_id.clone(), content.clone(), *chunk_index));
    }

    // Sort by RRF score and convert to ChunkResult
    let mut combined: Vec<ChunkResult> = chunk_scores
        .into_iter()
        .map(|(chunk_id, (score, atom_id, content, chunk_index))| {
            // Normalize RRF score to 0-1 range
            let max_rrf = 2.0 / (RRF_K + 1.0);
            ChunkResult {
                chunk_id,
                atom_id,
                content,
                chunk_index,
                score: (score / max_rrf).clamp(0.0, 1.0),
            }
        })
        .collect();

    combined.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    Ok(combined)
}

/// Check if an atom has any of the specified scope tags
fn atom_has_scope_tag(
    conn: &rusqlite::Connection,
    atom_id: &str,
    scope_tag_ids: &[String],
) -> Result<bool, String> {
    if scope_tag_ids.is_empty() {
        return Ok(true);
    }

    let placeholders: Vec<&str> = scope_tag_ids.iter().map(|_| "?").collect();
    let query = format!(
        "SELECT EXISTS(SELECT 1 FROM atom_tags WHERE atom_id = ?1 AND tag_id IN ({}))",
        placeholders.join(",")
    );

    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&atom_id];
    for tag_id in scope_tag_ids {
        params.push(tag_id);
    }

    conn.query_row(&query, rusqlite::params_from_iter(params), |row| row.get(0))
        .map_err(|e| format!("Failed to check tag scope: {}", e))
}

/// Filter results by tag scope (for keyword search batch filtering)
fn filter_by_scope<T>(
    conn: &rusqlite::Connection,
    results: Vec<(String, String, String, i32, T)>,
    scope_tag_ids: &[String],
) -> Result<Vec<(String, String, String, i32, T)>, String> {
    if scope_tag_ids.is_empty() {
        return Ok(results);
    }

    let mut filtered = Vec::new();
    for result in results {
        if atom_has_scope_tag(conn, &result.1, scope_tag_ids)? {
            filtered.push(result);
        }
    }
    Ok(filtered)
}

/// Find atoms similar to a given atom based on embedding similarity
///
/// 1. Get all chunks for the given atom
/// 2. For each chunk, find similar chunks in vec_chunks
/// 3. Filter by threshold (convert distance to similarity)
/// 4. Deduplicate by parent atom_id, keep highest similarity
/// 5. Exclude the source atom itself
/// 6. Return up to `limit` results
pub fn find_similar_atoms(
    conn: &rusqlite::Connection,
    atom_id: &str,
    limit: i32,
    threshold: f32,
) -> Result<Vec<SimilarAtomResult>, String> {
    use crate::embedding::distance_to_similarity;
    use std::collections::hash_map::Entry;

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
                match atom_similarities.entry(parent_atom_id) {
                    Entry::Occupied(mut e) => {
                        if similarity > e.get().0 {
                            e.insert((similarity, chunk_content, chunk_index));
                        }
                    }
                    Entry::Vacant(e) => {
                        e.insert((similarity, chunk_content, chunk_index));
                    }
                }
            }
        }
    }

    // 6. Build results, sorted by similarity
    let mut results: Vec<(String, f32, String, i32)> = atom_similarities
        .into_iter()
        .map(|(id, (sim, content, idx))| (id, sim, content, idx))
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(limit as usize);

    // Fetch atom data for results
    let mut final_results = Vec::new();
    for (result_atom_id, similarity, chunk_content, chunk_index) in results {
        let atom: Atom = conn
            .query_row(
                "SELECT id, SUBSTR(content, 1, 150) as content, source_url, created_at, updated_at,
                 COALESCE(embedding_status, 'pending'), COALESCE(tagging_status, 'pending')
                 FROM atoms WHERE id = ?1",
                [&result_atom_id],
                |row| {
                    Ok(Atom {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source_url: row.get(2)?,
                        created_at: row.get(3)?,
                        updated_at: row.get(4)?,
                        embedding_status: row.get(5)?,
                        tagging_status: row.get(6)?,
                    })
                },
            )
            .map_err(|e| format!("Failed to get atom: {}", e))?;

        final_results.push(SimilarAtomResult {
            atom: AtomWithTags { atom, tags: vec![] },
            similarity_score: similarity,
            matching_chunk_content: chunk_content,
            matching_chunk_index: chunk_index,
        });
    }

    Ok(final_results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::NamedTempFile;

    fn create_test_db() -> (Database, NamedTempFile) {
        let temp_file = NamedTempFile::new().unwrap();
        let db = Database::open_or_create(temp_file.path()).unwrap();
        (db, temp_file)
    }

    // ==================== Pure Function Tests ====================

    #[test]
    fn test_escape_fts5_query_basic() {
        let result = escape_fts5_query("hello world");
        assert_eq!(result, "\"hello\" \"world\"");
    }

    #[test]
    fn test_escape_fts5_query_special_chars() {
        // Quotes should be removed/escaped
        let result = escape_fts5_query("hello \"world\"");
        assert_eq!(result, "\"hello\" \"world\"");
    }

    #[test]
    fn test_escape_fts5_query_empty() {
        let result = escape_fts5_query("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_normalize_bm25_score_range() {
        // BM25 scores are negative, lower (more negative) = worse match
        let best = normalize_bm25_score(0.0); // Best match
        let mid = normalize_bm25_score(-15.0); // Medium match
        let worst = normalize_bm25_score(-30.0); // Worst match

        // Best should be highest, worst should be lowest
        assert!(best > mid, "best {} > mid {}", best, mid);
        assert!(mid > worst, "mid {} > worst {}", mid, worst);

        // All should be in 0-1 range
        assert!(best >= 0.0 && best <= 1.0);
        assert!(mid >= 0.0 && mid <= 1.0);
        assert!(worst >= 0.0 && worst <= 1.0);
    }

    // ==================== SearchOptions Tests ====================

    #[test]
    fn test_search_options_defaults() {
        let options = SearchOptions::default();

        assert_eq!(options.query, "");
        assert_eq!(options.mode, SearchMode::Hybrid);
        assert_eq!(options.limit, 10);
        assert_eq!(options.threshold, 0.3);
        assert!(options.scope_tag_ids.is_empty());
    }

    #[test]
    fn test_search_options_builder() {
        let options = SearchOptions::new("test query", SearchMode::Semantic, 20)
            .with_threshold(0.5)
            .with_scope(vec!["tag1".to_string()]);

        assert_eq!(options.query, "test query");
        assert_eq!(options.mode, SearchMode::Semantic);
        assert_eq!(options.limit, 20);
        assert_eq!(options.threshold, 0.5);
        assert_eq!(options.scope_tag_ids, vec!["tag1".to_string()]);
    }

    // ==================== find_similar_atoms Tests ====================

    #[test]
    fn test_find_similar_empty_atom() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        // Create an atom with no chunks
        let atom_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO atoms (id, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&atom_id, "test content", &now, &now],
        )
        .unwrap();

        // Find similar should return empty (no chunks to compare)
        let results = find_similar_atoms(&conn, &atom_id, 10, 0.5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_find_similar_excludes_source() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        // Create source atom
        let source_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO atoms (id, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&source_id, "source content", &now, &now],
        )
        .unwrap();

        // Create source chunk with embedding
        let chunk_id = uuid::Uuid::new_v4().to_string();
        let embedding: Vec<f32> = vec![0.1; 1536];
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        conn.execute(
            "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![&chunk_id, &source_id, 0, "chunk content", &blob],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![&chunk_id, &blob],
        )
        .unwrap();

        // Results should never include the source atom itself
        let results = find_similar_atoms(&conn, &source_id, 10, 0.0).unwrap();

        // Even with zero threshold, source shouldn't be in results
        for result in &results {
            assert_ne!(result.atom.atom.id, source_id);
        }
    }

    #[test]
    fn test_find_similar_threshold_filtering() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        // Create source atom with chunk
        let source_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO atoms (id, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&source_id, "source", &now, &now],
        )
        .unwrap();

        let source_chunk_id = uuid::Uuid::new_v4().to_string();
        let embedding: Vec<f32> = vec![1.0; 1536]; // Unit vector
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        conn.execute(
            "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![&source_chunk_id, &source_id, 0, "source chunk", &blob],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![&source_chunk_id, &blob],
        )
        .unwrap();

        // Create a target atom with very different embedding
        let target_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO atoms (id, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![&target_id, "target", &now, &now],
        )
        .unwrap();

        let target_chunk_id = uuid::Uuid::new_v4().to_string();
        let diff_embedding: Vec<f32> = vec![-1.0; 1536]; // Opposite vector
        let diff_blob: Vec<u8> = diff_embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        conn.execute(
            "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![&target_chunk_id, &target_id, 0, "target chunk", &diff_blob],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![&target_chunk_id, &diff_blob],
        )
        .unwrap();

        // With very high threshold, should find nothing (vectors are opposite)
        let results = find_similar_atoms(&conn, &source_id, 10, 0.99).unwrap();
        assert!(results.is_empty(), "High threshold should filter out dissimilar atoms");
    }
}
