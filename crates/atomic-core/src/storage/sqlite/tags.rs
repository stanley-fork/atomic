use super::SqliteStorage;
use crate::compaction::{CompactionResult, TagMerge};
use crate::error::AtomicCoreError;
use crate::models::*;
use crate::storage::traits::*;
use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
use chrono::Utc;

/// Load all tags and their direct (denormalized) atom counts from the database.
fn load_tags_and_counts(conn: &Connection) -> Result<(Vec<Tag>, HashMap<String, i32>), AtomicCoreError> {
    let mut stmt = conn
        .prepare("SELECT id, name, parent_id, created_at, atom_count, is_autotag_target FROM tags ORDER BY name")?;

    let mut direct_counts: HashMap<String, i32> = HashMap::new();
    let all_tags: Vec<Tag> = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let count: i32 = row.get(4)?;
            direct_counts.insert(id.clone(), count);
            Ok(Tag {
                id,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                created_at: row.get(3)?,
                is_autotag_target: row.get::<_, i32>(5)? != 0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok((all_tags, direct_counts))
}

impl SqliteStorage {
    pub(crate) fn get_all_tags_impl(&self) -> StorageResult<Vec<TagWithCount>> {
        self.get_all_tags_filtered_impl(0)
    }

    pub(crate) fn get_all_tags_filtered_impl(&self, min_count: i32) -> StorageResult<Vec<TagWithCount>> {
        let conn = self.db.read_conn()?;
        let (all_tags, direct_counts) = load_tags_and_counts(&conn)?;
        Ok(crate::build_tag_tree_with_counts(&all_tags, None, &direct_counts, min_count))
    }

    pub(crate) fn get_tag_children_impl(
        &self,
        parent_id: &str,
        min_count: i32,
        limit: i32,
        offset: i32,
    ) -> StorageResult<PaginatedTagChildren> {
        let conn = self.db.read_conn()?;

        // Fast total count using the parent_id index
        let total: i32 = conn.query_row(
            "SELECT COUNT(*) FROM tags WHERE parent_id = ?1",
            [parent_id],
            |row| row.get(0),
        )?;

        if total == 0 {
            return Ok(PaginatedTagChildren { children: Vec::new(), total: 0 });
        }

        // atom_count is denormalized on the tags table (maintained by triggers),
        // so no JOIN or GROUP BY needed — just read the column directly.
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.parent_id, t.created_at, t.atom_count,
                (SELECT COUNT(*) FROM tags c WHERE c.parent_id = t.id) AS children_total,
                t.is_autotag_target
            FROM tags t
            WHERE t.parent_id = ?1
            ORDER BY t.atom_count DESC
            LIMIT ?2 OFFSET ?3"
        )?;

        let mut children: Vec<TagWithCount> = stmt
            .query_map(rusqlite::params![parent_id, limit, offset], |row| {
                Ok(TagWithCount {
                    tag: Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        parent_id: row.get(2)?,
                        created_at: row.get(3)?,
                        is_autotag_target: row.get::<_, i32>(6)? != 0,
                    },
                    atom_count: row.get(4)?,
                    children_total: row.get(5)?,
                    children: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if min_count > 0 {
            children.retain(|t| t.atom_count >= min_count || t.children_total > 0);
        }

        Ok(PaginatedTagChildren { children, total })
    }

    pub(crate) fn create_tag_impl(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();

        conn.execute(
            "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            (&id, name, &parent_id, &now),
        )?;

        Ok(Tag {
            id,
            name: name.to_string(),
            parent_id: parent_id.map(String::from),
            created_at: now,
            is_autotag_target: false,
        })
    }

    pub(crate) fn update_tag_impl(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        conn.execute(
            "UPDATE tags SET name = ?1, parent_id = ?2 WHERE id = ?3",
            (name, &parent_id, id),
        )?;

        let tag = conn
            .query_row(
                "SELECT id, name, parent_id, created_at, is_autotag_target FROM tags WHERE id = ?1",
                [id],
                |row| {
                    Ok(Tag {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        parent_id: row.get(2)?,
                        created_at: row.get(3)?,
                        is_autotag_target: row.get::<_, i32>(4)? != 0,
                    })
                },
            )?;

        Ok(tag)
    }

    pub(crate) fn set_tag_autotag_target_impl(
        &self,
        id: &str,
        value: bool,
    ) -> StorageResult<()> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let val = if value { 1 } else { 0 };
        let affected = conn.execute(
            "UPDATE tags SET is_autotag_target = ?1 WHERE id = ?2",
            rusqlite::params![val, id],
        )?;
        if affected == 0 {
            return Err(AtomicCoreError::NotFound(format!("tag {}", id)));
        }
        Ok(())
    }

    /// Apply a full auto-tag-target configuration in a single transaction.
    ///
    /// Steps run atomically: any error rolls back the savepoint, leaving the
    /// tags table in its prior state. The orchestration mirrors the previous
    /// `AtomicCore::configure_autotag_targets` flow but acquires the connection
    /// lock once and uses an `unchecked_transaction` so a panic or early return
    /// can't leave the DB partially modified.
    pub(crate) fn configure_autotag_targets_impl(
        &self,
        keep_default_names: &[String],
        add_custom_names: &[String],
    ) -> StorageResult<Vec<Tag>> {
        const DEFAULT_NAMES: &[&str] = &["Topics", "People", "Locations", "Organizations", "Events"];

        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let tx = conn.unchecked_transaction()?;

        let keep_lower: HashSet<String> = keep_default_names
            .iter()
            .map(|n| n.trim().to_lowercase())
            .filter(|n| !n.is_empty())
            .collect();

        // Snapshot current top-level tags with the counts we need to decide
        // delete-vs-unflag for unrequested defaults.
        #[derive(Clone)]
        struct TopLevel {
            id: String,
            name: String,
            is_target: bool,
            atom_count: i32,
            children_count: i32,
        }
        let mut stmt = tx.prepare(
            "SELECT t.id, t.name, t.is_autotag_target, t.atom_count,
                    (SELECT COUNT(*) FROM tags c WHERE c.parent_id = t.id) AS children_count
             FROM tags t
             WHERE t.parent_id IS NULL"
        )?;
        let top_level: Vec<TopLevel> = stmt
            .query_map([], |row| Ok(TopLevel {
                id: row.get(0)?,
                name: row.get(1)?,
                is_target: row.get::<_, i32>(2)? != 0,
                atom_count: row.get(3)?,
                children_count: row.get(4)?,
            }))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let now = Utc::now().to_rfc3339();

        // Step 1: ensure each requested default exists and is flagged.
        for default_name in DEFAULT_NAMES {
            if !keep_lower.contains(&default_name.to_lowercase()) {
                continue;
            }
            let existing = top_level.iter().find(|t| t.name.eq_ignore_ascii_case(default_name));
            let id = match existing {
                Some(t) => t.id.clone(),
                None => {
                    let new_id = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO tags (id, name, parent_id, created_at, is_autotag_target)
                         VALUES (?1, ?2, NULL, ?3, 1)",
                        rusqlite::params![&new_id, default_name, &now],
                    )?;
                    new_id
                }
            };
            tx.execute(
                "UPDATE tags SET is_autotag_target = 1 WHERE id = ?1",
                rusqlite::params![&id],
            )?;
        }

        // Step 2: handle unrequested defaults — delete if empty, otherwise unflag.
        for t in &top_level {
            let is_default = DEFAULT_NAMES.iter().any(|d| d.eq_ignore_ascii_case(&t.name));
            let is_kept = keep_lower.contains(&t.name.to_lowercase());
            if !is_default || is_kept {
                continue;
            }
            if t.atom_count == 0 && t.children_count == 0 {
                tx.execute("DELETE FROM tags WHERE id = ?1", rusqlite::params![&t.id])?;
            } else if t.is_target {
                tx.execute(
                    "UPDATE tags SET is_autotag_target = 0 WHERE id = ?1",
                    rusqlite::params![&t.id],
                )?;
            }
        }

        // Step 3: custom additions — create or flag in place. Re-query each
        // name since Step 2 may have deleted rows that were in our snapshot.
        let mut custom_tags: Vec<Tag> = Vec::new();
        for name in add_custom_names {
            let trimmed = name.trim();
            if trimmed.is_empty() {
                continue;
            }
            let existing_id: Option<String> = tx
                .query_row(
                    "SELECT id FROM tags WHERE parent_id IS NULL AND LOWER(name) = LOWER(?1) LIMIT 1",
                    [trimmed],
                    |row| row.get(0),
                )
                .optional()?;
            let id = match existing_id {
                Some(id) => id,
                None => {
                    let new_id = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO tags (id, name, parent_id, created_at, is_autotag_target)
                         VALUES (?1, ?2, NULL, ?3, 1)",
                        rusqlite::params![&new_id, trimmed, &now],
                    )?;
                    new_id
                }
            };
            tx.execute(
                "UPDATE tags SET is_autotag_target = 1 WHERE id = ?1",
                rusqlite::params![&id],
            )?;
            let tag = tx.query_row(
                "SELECT id, name, parent_id, created_at, is_autotag_target FROM tags WHERE id = ?1",
                [&id],
                |row| Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    parent_id: row.get(2)?,
                    created_at: row.get(3)?,
                    is_autotag_target: row.get::<_, i32>(4)? != 0,
                }),
            )?;
            custom_tags.push(tag);
        }

        tx.commit()?;
        Ok(custom_tags)
    }

    pub(crate) fn delete_tag_impl(&self, id: &str, recursive: bool) -> StorageResult<()> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;

        if recursive {
            // Delete tag and all descendants via recursive CTE
            conn.execute(
                "WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM tags WHERE id = ?1
                    UNION ALL
                    SELECT t.id FROM tags t JOIN descendants d ON t.parent_id = d.id
                )
                DELETE FROM tags WHERE id IN (SELECT id FROM descendants)",
                [id],
            )?;
        } else {
            conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
        }

        Ok(())
    }

    pub(crate) fn get_related_tags_impl(
        &self,
        tag_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<RelatedTag>> {
        let conn = self.db.read_conn()?;
        crate::wiki::get_related_tags(&conn, tag_id, limit)
            .map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn get_tags_for_compaction_impl(&self) -> StorageResult<String> {
        let conn = self.db.read_conn()?;
        crate::compaction::read_all_tags(&conn)
            .map_err(|e| AtomicCoreError::Compaction(e))
    }

    pub(crate) fn get_or_create_tag_impl(
        &self,
        name: &str,
        parent_name: Option<&str>,
    ) -> StorageResult<String> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let parent_name_owned = parent_name.map(String::from);
        crate::extraction::get_or_create_tag(&conn, name, &parent_name_owned)
            .map_err(|e| AtomicCoreError::Validation(e))
    }

    pub(crate) fn link_tags_to_atom_impl(
        &self,
        atom_id: &str,
        tag_ids: &[String],
    ) -> StorageResult<()> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::extraction::link_tags_to_atom(&conn, atom_id, tag_ids)
            .map_err(|e| AtomicCoreError::Validation(e))
    }

    pub(crate) fn get_tag_tree_for_llm_impl(&self) -> StorageResult<String> {
        let conn = self.db.read_conn()?;
        crate::extraction::get_tag_tree_for_llm(&conn)
            .map_err(|e| AtomicCoreError::Validation(e))
    }

    pub(crate) fn compute_tag_centroids_batch_impl(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<()> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::embedding::compute_tag_embeddings_batch(&conn, tag_ids)
            .map_err(|e| AtomicCoreError::Embedding(e))
    }

    pub(crate) fn cleanup_orphaned_parents_impl(
        &self,
        tag_id: &str,
    ) -> StorageResult<()> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        crate::extraction::cleanup_orphaned_parents(&conn, tag_id)
            .map_err(|e| AtomicCoreError::Validation(e))
    }

    pub(crate) fn get_tag_hierarchy_impl(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<String>> {
        let conn = self.db.read_conn()?;
        crate::wiki::get_tag_hierarchy(&conn, tag_id)
            .map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn count_atoms_with_tags_impl(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<i32> {
        let conn = self.db.read_conn()?;
        crate::wiki::count_atoms_with_tags(&conn, tag_ids)
            .map_err(|e| AtomicCoreError::Wiki(e))
    }

    pub(crate) fn apply_tag_merges_impl(
        &self,
        merges: &[TagMerge],
    ) -> StorageResult<CompactionResult> {
        let conn = self.db.conn.lock().map_err(|e| AtomicCoreError::Lock(e.to_string()))?;
        let (tags_merged, atoms_retagged, errors) = crate::compaction::apply_merge_operations(&conn, merges);

        if !errors.is_empty() {
            tracing::error!(errors = ?errors, "Merge errors");
        }

        Ok(CompactionResult {
            tags_merged,
            atoms_retagged,
        })
    }
}

#[async_trait]
impl TagStore for SqliteStorage {
    async fn get_all_tags(&self) -> StorageResult<Vec<TagWithCount>> {
        self.get_all_tags_impl()
    }

    async fn get_all_tags_filtered(&self, min_count: i32) -> StorageResult<Vec<TagWithCount>> {
        self.get_all_tags_filtered_impl(min_count)
    }

    async fn get_tag_children(
        &self,
        parent_id: &str,
        min_count: i32,
        limit: i32,
        offset: i32,
    ) -> StorageResult<PaginatedTagChildren> {
        self.get_tag_children_impl(parent_id, min_count, limit, offset)
    }

    async fn create_tag(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag> {
        self.create_tag_impl(name, parent_id)
    }

    async fn update_tag(
        &self,
        id: &str,
        name: &str,
        parent_id: Option<&str>,
    ) -> StorageResult<Tag> {
        self.update_tag_impl(id, name, parent_id)
    }

    async fn delete_tag(&self, id: &str, recursive: bool) -> StorageResult<()> {
        self.delete_tag_impl(id, recursive)
    }

    async fn set_tag_autotag_target(&self, id: &str, value: bool) -> StorageResult<()> {
        self.set_tag_autotag_target_impl(id, value)
    }

    async fn configure_autotag_targets(
        &self,
        keep_default_names: &[String],
        add_custom_names: &[String],
    ) -> StorageResult<Vec<Tag>> {
        self.configure_autotag_targets_impl(keep_default_names, add_custom_names)
    }

    async fn get_related_tags(
        &self,
        tag_id: &str,
        limit: usize,
    ) -> StorageResult<Vec<RelatedTag>> {
        self.get_related_tags_impl(tag_id, limit)
    }

    async fn get_tags_for_compaction(&self) -> StorageResult<String> {
        self.get_tags_for_compaction_impl()
    }

    async fn apply_tag_merges(
        &self,
        merges: &[TagMerge],
    ) -> StorageResult<CompactionResult> {
        self.apply_tag_merges_impl(merges)
    }

    async fn get_or_create_tag(
        &self,
        name: &str,
        parent_name: Option<&str>,
    ) -> StorageResult<String> {
        self.get_or_create_tag_impl(name, parent_name)
    }

    async fn link_tags_to_atom(
        &self,
        atom_id: &str,
        tag_ids: &[String],
    ) -> StorageResult<()> {
        self.link_tags_to_atom_impl(atom_id, tag_ids)
    }

    async fn get_tag_tree_for_llm(&self) -> StorageResult<String> {
        self.get_tag_tree_for_llm_impl()
    }

    async fn compute_tag_centroids_batch(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<()> {
        self.compute_tag_centroids_batch_impl(tag_ids)
    }

    async fn cleanup_orphaned_parents(
        &self,
        tag_id: &str,
    ) -> StorageResult<()> {
        self.cleanup_orphaned_parents_impl(tag_id)
    }

    async fn get_tag_hierarchy(
        &self,
        tag_id: &str,
    ) -> StorageResult<Vec<String>> {
        self.get_tag_hierarchy_impl(tag_id)
    }

    async fn count_atoms_with_tags(
        &self,
        tag_ids: &[String],
    ) -> StorageResult<i32> {
        self.count_atoms_with_tags_impl(tag_ids)
    }
}
