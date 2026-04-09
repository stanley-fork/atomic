use std::collections::HashMap;

use super::SqliteStorage;
use crate::embedding;
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use async_trait::async_trait;
use uuid::Uuid;

impl SqliteStorage {
    pub(crate) fn get_pending_embeddings_sync(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<(String, String)>> {
        let conn = self.db.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, content FROM atoms WHERE embedding_status = 'pending' LIMIT ?1",
        )?;
        let results = stmt
            .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub(crate) fn set_embedding_status_sync(
        &self,
        atom_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        conn.execute(
            "UPDATE atoms SET embedding_status = ?2, embedding_error = ?3 WHERE id = ?1",
            rusqlite::params![atom_id, status, error],
        )?;
        Ok(())
    }

    /// Set embedding status for multiple atoms in a single statement.
    pub(crate) fn set_embedding_status_batch_sync(
        &self,
        atom_ids: &[String],
        status: &str,
        error: Option<&str>,
    ) -> StorageResult<()> {
        if atom_ids.is_empty() {
            return Ok(());
        }
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let placeholders = atom_ids.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 3))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "UPDATE atoms SET embedding_status = ?1, embedding_error = ?2 WHERE id IN ({})",
            placeholders
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::with_capacity(2 + atom_ids.len());
        params.push(Box::new(status.to_string()));
        params.push(Box::new(error.map(|e| e.to_string())));
        for id in atom_ids {
            params.push(Box::new(id.clone()));
        }
        conn.execute(&sql, rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())))?;
        Ok(())
    }

    pub(crate) fn set_tagging_status_sync(
        &self,
        atom_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        conn.execute(
            "UPDATE atoms SET tagging_status = ?2, tagging_error = ?3 WHERE id = ?1",
            rusqlite::params![atom_id, status, error],
        )?;
        Ok(())
    }

    pub(crate) fn save_chunks_and_embeddings_sync(
        &self,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)],
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        Self::save_chunks_for_atom(&conn, atom_id, chunks)
    }

    /// Save chunks and embeddings for multiple atoms in a single transaction.
    /// Each entry is (atom_id, chunks). One lock acquire, one fsync.
    pub(crate) fn save_chunks_and_embeddings_batch_sync(
        &self,
        atoms: &[(String, Vec<(String, Vec<f32>)>)],
    ) -> StorageResult<Vec<String>> {
        if atoms.is_empty() {
            return Ok(Vec::new());
        }
        let mut conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let tx = conn.transaction()
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        let mut succeeded = Vec::new();
        for (atom_id, chunks) in atoms {
            match Self::save_chunks_for_atom(&tx, atom_id, chunks) {
                Ok(()) => succeeded.push(atom_id.clone()),
                Err(e) => {
                    tracing::warn!(atom_id = %atom_id, error = %e, "Failed to save chunks for atom");
                }
            }
        }

        tx.commit()
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(succeeded)
    }

    /// Inner helper: save chunks for a single atom on an existing connection/transaction.
    fn save_chunks_for_atom(
        conn: &rusqlite::Connection,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)],
    ) -> StorageResult<()> {
        // Remove old FTS entries before deleting chunks
        conn.execute(
            "INSERT INTO atom_chunks_fts(atom_chunks_fts, rowid, id, atom_id, chunk_index, content)
             SELECT 'delete', rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        // Delete existing vec_chunks
        conn.execute(
            "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
            [atom_id],
        )
        .ok();

        // Delete existing atom_chunks
        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])?;

        // Insert new chunks and embeddings
        for (index, (chunk_content, embedding_vec)) in chunks.iter().enumerate() {
            let chunk_id = Uuid::new_v4().to_string();
            let embedding_blob = embedding::f32_vec_to_blob_public(embedding_vec);

            conn.execute(
                "INSERT INTO atom_chunks (id, atom_id, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![&chunk_id, atom_id, index as i32, chunk_content, &embedding_blob],
            )?;

            conn.execute(
                "INSERT INTO vec_chunks (chunk_id, embedding) VALUES (?1, ?2)",
                rusqlite::params![&chunk_id, &embedding_blob],
            )?;
        }

        // Incrementally update FTS index
        conn.execute(
            "INSERT INTO atom_chunks_fts(rowid, id, atom_id, chunk_index, content)
             SELECT rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        Ok(())
    }

    pub(crate) fn delete_chunks_sync(&self, atom_id: &str) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Remove FTS entries
        conn.execute(
            "INSERT INTO atom_chunks_fts(atom_chunks_fts, rowid, id, atom_id, chunk_index, content)
             SELECT 'delete', rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
            [atom_id],
        )
        .ok();

        conn.execute(
            "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
            [atom_id],
        )
        .ok();

        conn.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])?;
        Ok(())
    }

    pub(crate) fn reset_stuck_processing_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let embedding_count = conn.execute(
            "UPDATE atoms SET embedding_status = 'pending' WHERE embedding_status = 'processing'",
            [],
        )?;

        let tagging_count = conn.execute(
            "UPDATE atoms SET tagging_status = 'pending' WHERE tagging_status = 'processing'",
            [],
        )?;

        // Reset edges stuck in 'processing' back to 'pending'
        let edges_count = conn.execute(
            "UPDATE atoms SET edges_status = 'pending' WHERE edges_status = 'processing'",
            [],
        )?;

        Ok((embedding_count + tagging_count + edges_count) as i32)
    }

    /// Reset failed embedding and tagging atoms back to pending (for auto-retry on config fix).
    pub(crate) fn reset_failed_embeddings_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let embedding_count = conn.execute(
            "UPDATE atoms SET embedding_status = 'pending', embedding_error = NULL WHERE embedding_status = 'failed'",
            [],
        )?;

        let tagging_count = conn.execute(
            "UPDATE atoms SET tagging_status = 'pending', tagging_error = NULL WHERE tagging_status = 'failed'",
            [],
        )?;

        Ok((embedding_count + tagging_count) as i32)
    }

    pub(crate) fn rebuild_semantic_edges_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Clear all existing edges and mark all embedded atoms as needing edge recomputation
        conn.execute("DELETE FROM semantic_edges", [])?;
        let count = conn.execute(
            "UPDATE atoms SET edges_status = 'pending' WHERE embedding_status = 'complete'",
            [],
        )? as i32;

        drop(conn);

        // Kick off the batched edge computation pipeline
        if count > 0 {
            tracing::info!(count, "Marked atoms for edge recomputation, starting pipeline");
            embedding::process_pending_edges(
                crate::storage::StorageBackend::Sqlite(self.clone()),
            )
            .map_err(|e| AtomicCoreError::Embedding(e))?;
        }

        Ok(count)
    }

    /// Raw edge triples (source, target, score) sorted by score DESC.
    /// Lightweight — no full SemanticEdge struct, no chunk indexes, no timestamps.
    pub(crate) fn get_semantic_edges_raw_sync(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<(String, String, f32)>> {
        let conn = self.db.read_conn()?;
        let mut stmt = conn.prepare(
            "SELECT source_atom_id, target_atom_id, similarity_score
             FROM semantic_edges
             WHERE similarity_score >= ?1
             ORDER BY similarity_score DESC"
        )?;
        let edges = stmt.query_map([min_similarity], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
        Ok(edges)
    }

    pub(crate) fn get_semantic_edges_sync(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<SemanticEdge>> {
        let conn = self.db.read_conn()?;

        let mut stmt = conn.prepare(
            "SELECT id, source_atom_id, target_atom_id, similarity_score,
                    source_chunk_index, target_chunk_index, created_at
             FROM semantic_edges
             WHERE similarity_score >= ?1
             ORDER BY similarity_score DESC
             LIMIT 10000",
        )?;

        let edges = stmt
            .query_map([min_similarity], |row| {
                Ok(SemanticEdge {
                    id: row.get(0)?,
                    source_atom_id: row.get(1)?,
                    target_atom_id: row.get(2)?,
                    similarity_score: row.get(3)?,
                    source_chunk_index: row.get(4)?,
                    target_chunk_index: row.get(5)?,
                    created_at: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(edges)
    }

    pub(crate) fn get_atom_neighborhood_sync(
        &self,
        atom_id: &str,
        depth: i32,
        min_similarity: f32,
    ) -> StorageResult<NeighborhoodGraph> {
        let conn = self.db.read_conn()?;
        crate::build_neighborhood_graph(&conn, atom_id, depth, min_similarity)
    }

    pub(crate) fn get_connection_counts_sync(
        &self,
        min_similarity: f32,
    ) -> StorageResult<HashMap<String, i32>> {
        let conn = self.db.read_conn()?;
        crate::clustering::get_connection_counts(&conn, min_similarity)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    pub(crate) fn save_tag_centroid_sync(
        &self,
        tag_id: &str,
        embedding_vec: &[f32],
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let embedding_blob = embedding::f32_vec_to_blob_public(embedding_vec);
        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "INSERT OR REPLACE INTO tag_embeddings (tag_id, embedding, atom_count, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![tag_id, &embedding_blob, 0, &now],
        )?;

        // vec0 doesn't support REPLACE, so delete + insert
        conn.execute("DELETE FROM vec_tags WHERE tag_id = ?1", [tag_id])
            .ok();
        conn.execute(
            "INSERT INTO vec_tags (tag_id, embedding) VALUES (?1, ?2)",
            rusqlite::params![tag_id, &embedding_blob],
        )?;

        Ok(())
    }

    pub(crate) fn recompute_all_tag_embeddings_sync(&self) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        // Get all tags that have at least one atom with embeddings
        let mut stmt = conn.prepare(
            "SELECT DISTINCT at.tag_id
             FROM atom_tags at
             INNER JOIN atom_chunks ac ON at.atom_id = ac.atom_id
             WHERE ac.embedding IS NOT NULL",
        )?;

        let tag_ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let count = tag_ids.len() as i32;
        tracing::info!(count, "Recomputing centroid embeddings for tags");

        embedding::compute_tag_embeddings_batch(&conn, &tag_ids)
            .map_err(|e| AtomicCoreError::Embedding(e))?;

        tracing::info!(count, "Tag centroid embeddings recomputed");
        Ok(count)
    }

    pub(crate) fn claim_pending_embeddings_sync(
        &self,
        limit: i32,
    ) -> StorageResult<Vec<(String, String)>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let mut stmt = conn.prepare(
            "UPDATE atoms SET embedding_status = 'processing'
             WHERE id IN (SELECT id FROM atoms WHERE embedding_status = 'pending' LIMIT ?1)
             RETURNING id, content",
        )?;
        let results = stmt
            .query_map([limit], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    /// Atomically claim atoms that need edge computation: sets edges_status to 'processing'
    /// and returns their IDs.
    pub(crate) fn claim_pending_edges_sync(&self, limit: i32) -> StorageResult<Vec<String>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let mut stmt = conn.prepare(
            "UPDATE atoms SET edges_status = 'processing'
             WHERE id IN (SELECT id FROM atoms WHERE edges_status = 'pending' AND embedding_status = 'complete' LIMIT ?1)
             RETURNING id",
        )?;
        let results = stmt
            .query_map([limit], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    /// Mark edges_status for a batch of atoms.
    pub(crate) fn set_edges_status_batch_sync(
        &self,
        atom_ids: &[String],
        status: &str,
    ) -> StorageResult<()> {
        if atom_ids.is_empty() {
            return Ok(());
        }
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let placeholders = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE atoms SET edges_status = ?1 WHERE id IN ({})",
            placeholders
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::with_capacity(1 + atom_ids.len());
        params.push(Box::new(status.to_string()));
        for id in atom_ids {
            params.push(Box::new(id.clone()));
        }
        conn.execute(
            &sql,
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
        )?;
        Ok(())
    }

    /// Count atoms with pending edge computation.
    pub(crate) fn count_pending_edges_sync(&self) -> StorageResult<i32> {
        let conn = self.db.read_conn()?;
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM atoms WHERE edges_status = 'pending' AND embedding_status = 'complete'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub(crate) fn delete_chunks_batch_sync(&self, atom_ids: &[String]) -> StorageResult<()> {
        if atom_ids.is_empty() {
            return Ok(());
        }
        let mut conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let tx = conn.transaction()
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;

        for atom_id in atom_ids {
            // Remove FTS entries
            tx.execute(
                "INSERT INTO atom_chunks_fts(atom_chunks_fts, rowid, id, atom_id, chunk_index, content)
                 SELECT 'delete', rowid, id, atom_id, chunk_index, content FROM atom_chunks WHERE atom_id = ?1",
                [atom_id],
            )
            .ok();

            tx.execute(
                "DELETE FROM vec_chunks WHERE chunk_id IN (SELECT id FROM atom_chunks WHERE atom_id = ?1)",
                [atom_id],
            )
            .ok();

            tx.execute("DELETE FROM atom_chunks WHERE atom_id = ?1", [atom_id])?;
        }

        tx.commit()
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(())
    }

    pub(crate) fn compute_semantic_edges_for_atom_sync(
        &self,
        atom_id: &str,
        threshold: f32,
        max_edges: i32,
    ) -> StorageResult<i32> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        embedding::compute_semantic_edges_for_atom(&conn, atom_id, threshold, max_edges)
            .map_err(|e| AtomicCoreError::Embedding(e))
    }

    /// Compute semantic edges for a batch of atoms in a single transaction.
    /// Returns total number of edges created across all atoms.
    pub(crate) fn compute_semantic_edges_batch_sync(
        &self,
        atom_ids: &[String],
        threshold: f32,
        max_edges: i32,
    ) -> StorageResult<i32> {
        let mut conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let tx = conn.transaction()
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        let mut total_edges = 0;
        for atom_id in atom_ids {
            match embedding::compute_semantic_edges_for_atom(&tx, atom_id, threshold, max_edges) {
                Ok(count) => total_edges += count,
                Err(e) => {
                    tracing::warn!(atom_id = %atom_id, error = %e, "Failed to compute edges for atom");
                }
            }
        }
        tx.commit()
            .map_err(|e| AtomicCoreError::DatabaseOperation(e.to_string()))?;
        Ok(total_edges)
    }

    pub(crate) fn rebuild_fts_index_sync(&self) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        conn.execute(
            "INSERT INTO atom_chunks_fts(atom_chunks_fts) VALUES('rebuild')",
            [],
        )?;
        Ok(())
    }

    pub(crate) fn check_vector_extension_sync(&self) -> StorageResult<String> {
        let conn = self.db.read_conn()?;
        let version: String =
            conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
        Ok(version)
    }

    pub(crate) fn claim_pending_tagging_sync(&self) -> StorageResult<Vec<String>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let mut stmt = conn.prepare(
            "UPDATE atoms SET tagging_status = 'processing'
             WHERE embedding_status = 'complete'
             AND tagging_status = 'pending'
             RETURNING id",
        )?;
        let results = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub(crate) fn get_embedding_dimension_sync(&self) -> StorageResult<Option<usize>> {
        let conn = self.db.read_conn()?;
        let dim = conn
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
            });
        Ok(dim)
    }

    pub(crate) fn recreate_vector_index_sync(&self, dimension: usize) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::db::recreate_vec_chunks_with_dimension(&conn, dimension)
    }

    pub(crate) fn claim_pending_reembedding_sync(&self) -> StorageResult<Vec<String>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let mut stmt = conn.prepare(
            "UPDATE atoms SET embedding_status = 'processing'
             WHERE embedding_status IN ('pending', 'processing')
             RETURNING id",
        )?;
        let results = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub(crate) fn claim_all_for_reembedding_sync(&self) -> StorageResult<Vec<String>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let mut stmt = conn.prepare(
            "UPDATE atoms SET embedding_status = 'processing'
             RETURNING id",
        )?;
        let results = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    pub(crate) fn get_pipeline_status_sync(&self) -> StorageResult<PipelineStatus> {
        let conn = self.db.read_conn()?;
        let pending: i32 = conn.query_row(
            "SELECT COUNT(*) FROM atoms WHERE embedding_status = 'pending'",
            [],
            |r| r.get(0),
        )?;
        let processing: i32 = conn.query_row(
            "SELECT COUNT(*) FROM atoms WHERE embedding_status = 'processing'",
            [],
            |r| r.get(0),
        )?;
        let complete: i32 = conn.query_row(
            "SELECT COUNT(*) FROM atoms WHERE embedding_status = 'complete'",
            [],
            |r| r.get(0),
        )?;
        let failed_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM atoms WHERE embedding_status = 'failed'",
            [],
            |r| r.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, title, snippet, embedding_error, updated_at FROM atoms WHERE embedding_status = 'failed' ORDER BY updated_at DESC LIMIT 100",
        )?;
        let failed: Vec<FailedAtom> = stmt
            .query_map([], |row| {
                Ok(FailedAtom {
                    atom_id: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get(2)?,
                    error: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let tagging_failed_count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM atoms WHERE tagging_status = 'failed'",
            [],
            |r| r.get(0),
        )?;

        let mut stmt = conn.prepare(
            "SELECT id, title, snippet, tagging_error, updated_at FROM atoms WHERE tagging_status = 'failed' ORDER BY updated_at DESC LIMIT 100",
        )?;
        let tagging_failed: Vec<FailedAtom> = stmt
            .query_map([], |row| {
                Ok(FailedAtom {
                    atom_id: row.get(0)?,
                    title: row.get(1)?,
                    snippet: row.get(2)?,
                    error: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PipelineStatus {
            pending,
            processing,
            complete,
            failed_count,
            failed,
            tagging_failed_count,
            tagging_failed,
        })
    }
}

#[async_trait]
impl ChunkStore for SqliteStorage {
    async fn get_pending_embeddings(&self, limit: i32) -> StorageResult<Vec<(String, String)>> {
        self.get_pending_embeddings_sync(limit)
    }

    async fn set_embedding_status(
        &self,
        atom_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> StorageResult<()> {
        self.set_embedding_status_sync(atom_id, status, error)
    }

    async fn set_tagging_status(
        &self,
        atom_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> StorageResult<()> {
        self.set_tagging_status_sync(atom_id, status, error)
    }

    async fn save_chunks_and_embeddings(
        &self,
        atom_id: &str,
        chunks: &[(String, Vec<f32>)],
    ) -> StorageResult<()> {
        self.save_chunks_and_embeddings_sync(atom_id, chunks)
    }

    async fn delete_chunks(&self, atom_id: &str) -> StorageResult<()> {
        self.delete_chunks_sync(atom_id)
    }

    async fn reset_stuck_processing(&self) -> StorageResult<i32> {
        self.reset_stuck_processing_sync()
    }

    async fn reset_failed_embeddings(&self) -> StorageResult<i32> {
        self.reset_failed_embeddings_sync()
    }

    async fn rebuild_semantic_edges(&self) -> StorageResult<i32> {
        self.rebuild_semantic_edges_sync()
    }

    async fn get_semantic_edges(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<SemanticEdge>> {
        self.get_semantic_edges_sync(min_similarity)
    }

    async fn get_semantic_edges_raw(
        &self,
        min_similarity: f32,
    ) -> StorageResult<Vec<(String, String, f32)>> {
        self.get_semantic_edges_raw_sync(min_similarity)
    }

    async fn get_atom_neighborhood(
        &self,
        atom_id: &str,
        depth: i32,
        min_similarity: f32,
    ) -> StorageResult<NeighborhoodGraph> {
        self.get_atom_neighborhood_sync(atom_id, depth, min_similarity)
    }

    async fn get_connection_counts(
        &self,
        min_similarity: f32,
    ) -> StorageResult<HashMap<String, i32>> {
        self.get_connection_counts_sync(min_similarity)
    }

    async fn save_tag_centroid(
        &self,
        tag_id: &str,
        embedding: &[f32],
    ) -> StorageResult<()> {
        self.save_tag_centroid_sync(tag_id, embedding)
    }

    async fn recompute_all_tag_embeddings(&self) -> StorageResult<i32> {
        self.recompute_all_tag_embeddings_sync()
    }

    async fn check_vector_extension(&self) -> StorageResult<String> {
        self.check_vector_extension_sync()
    }

    async fn claim_pending_embeddings(&self, limit: i32) -> StorageResult<Vec<(String, String)>> {
        self.claim_pending_embeddings_sync(limit)
    }

    async fn delete_chunks_batch(&self, atom_ids: &[String]) -> StorageResult<()> {
        self.delete_chunks_batch_sync(atom_ids)
    }

    async fn compute_semantic_edges_for_atom(
        &self,
        atom_id: &str,
        threshold: f32,
        max_edges: i32,
    ) -> StorageResult<i32> {
        self.compute_semantic_edges_for_atom_sync(atom_id, threshold, max_edges)
    }

    async fn rebuild_fts_index(&self) -> StorageResult<()> {
        self.rebuild_fts_index_sync()
    }

    async fn claim_pending_tagging(&self) -> StorageResult<Vec<String>> {
        self.claim_pending_tagging_sync()
    }

    async fn get_embedding_dimension(&self) -> StorageResult<Option<usize>> {
        self.get_embedding_dimension_sync()
    }

    async fn recreate_vector_index(&self, dimension: usize) -> StorageResult<()> {
        self.recreate_vector_index_sync(dimension)
    }

    async fn claim_pending_reembedding(&self) -> StorageResult<Vec<String>> {
        self.claim_pending_reembedding_sync()
    }

    async fn claim_all_for_reembedding(&self) -> StorageResult<Vec<String>> {
        self.claim_all_for_reembedding_sync()
    }

    async fn claim_pending_edges(&self, limit: i32) -> StorageResult<Vec<String>> {
        self.claim_pending_edges_sync(limit)
    }

    async fn set_edges_status_batch(
        &self,
        atom_ids: &[String],
        status: &str,
    ) -> StorageResult<()> {
        self.set_edges_status_batch_sync(atom_ids, status)
    }

    async fn count_pending_edges(&self) -> StorageResult<i32> {
        self.count_pending_edges_sync()
    }
}
