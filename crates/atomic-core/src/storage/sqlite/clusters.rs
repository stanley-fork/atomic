use super::SqliteStorage;
use crate::error::AtomicCoreError;
use crate::get_dominant_tags_for_cluster;
use crate::models::*;
use crate::storage::traits::*;
use crate::{canvas_level, clustering};
use async_trait::async_trait;

impl SqliteStorage {
    pub(crate) fn compute_clusters_sync(
        &self,
        min_similarity: f32,
        min_cluster_size: i32,
    ) -> StorageResult<Vec<AtomCluster>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        clustering::compute_atom_clusters(&conn, min_similarity, min_cluster_size)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    pub(crate) fn save_clusters_sync(
        &self,
        clusters: &[AtomCluster],
    ) -> StorageResult<()> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        clustering::save_cluster_assignments(&conn, clusters)
            .map_err(|e| AtomicCoreError::Clustering(e))
    }

    pub(crate) fn get_clusters_sync(&self) -> StorageResult<Vec<AtomCluster>> {
        let conn = self
            .db
            .conn
            .lock()
            .map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM atom_clusters", [], |row| row.get(0))
            .unwrap_or(0);

        if count == 0 {
            let clusters = clustering::compute_atom_clusters(&conn, 0.5, 2)
                .map_err(|e| AtomicCoreError::Clustering(e))?;
            clustering::save_cluster_assignments(&conn, &clusters)
                .map_err(|e| AtomicCoreError::Clustering(e))?;
            return Ok(clusters);
        }

        // Rebuild from cached assignments
        let mut stmt = conn.prepare(
            "SELECT ac.cluster_id, GROUP_CONCAT(ac.atom_id)
             FROM atom_clusters ac
             GROUP BY ac.cluster_id
             ORDER BY COUNT(*) DESC",
        )?;

        let clusters: Vec<AtomCluster> = stmt
            .query_map([], |row| {
                let cluster_id: i32 = row.get(0)?;
                let atom_ids_str: String = row.get(1)?;
                let atom_ids: Vec<String> =
                    atom_ids_str.split(',').map(|s| s.to_string()).collect();
                Ok((cluster_id, atom_ids))
            })?
            .filter_map(|r| r.ok())
            .map(|(cluster_id, atom_ids)| {
                let dominant_tags =
                    get_dominant_tags_for_cluster(&conn, &atom_ids).unwrap_or_default();
                AtomCluster {
                    cluster_id,
                    atom_ids,
                    dominant_tags,
                }
            })
            .collect();

        Ok(clusters)
    }

    /// Fill in dominant_tags for clusters that were computed without DB access.
    pub(crate) fn enrich_clusters_with_tags_sync(
        &self,
        mut clusters: Vec<AtomCluster>,
    ) -> StorageResult<Vec<AtomCluster>> {
        let conn = self.db.read_conn()?;
        for cluster in &mut clusters {
            cluster.dominant_tags = get_dominant_tags_for_cluster(&conn, &cluster.atom_ids)
                .unwrap_or_default();
        }
        Ok(clusters)
    }

    pub(crate) fn get_canvas_level_sync(
        &self,
        parent_id: Option<&str>,
        children_hint: Option<Vec<String>>,
    ) -> StorageResult<CanvasLevel> {
        // Uses a fresh connection because canvas_level creates temp tables for batch queries,
        // which are blocked by PRAGMA query_only=ON on read-pool connections.
        let conn = self.db.new_connection()?;
        canvas_level::get_canvas_level(&conn, parent_id, children_hint)
    }
}

#[async_trait]
impl ClusterStore for SqliteStorage {
    async fn compute_clusters(
        &self,
        min_similarity: f32,
        min_cluster_size: i32,
    ) -> StorageResult<Vec<AtomCluster>> {
        self.compute_clusters_sync(min_similarity, min_cluster_size)
    }

    async fn save_clusters(&self, clusters: &[AtomCluster]) -> StorageResult<()> {
        self.save_clusters_sync(clusters)
    }

    async fn get_clusters(&self) -> StorageResult<Vec<AtomCluster>> {
        self.get_clusters_sync()
    }

    async fn get_canvas_level(
        &self,
        parent_id: Option<&str>,
        children_hint: Option<Vec<String>>,
    ) -> StorageResult<CanvasLevel> {
        self.get_canvas_level_sync(parent_id, children_hint)
    }
}
