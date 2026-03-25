//! Hierarchical canvas level computation
//!
//! Computes a single level of the drill-down canvas view, returning nodes and edges
//! appropriate for the current navigation depth.

use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use crate::clustering;
use crate::error::AtomicCoreError;
use crate::models::*;

/// Maximum nodes to show before aggregating into semantic clusters
const MAX_TAGS_PER_LEVEL: usize = 40;
/// Maximum atoms to show before sub-clustering
const MAX_ATOMS_PER_LEVEL: usize = 50;
/// Top N tags to show individually when aggregating
const TOP_TAGS_SHOWN: usize = 20;
/// Minimum similarity for semantic edges used in clustering
const CLUSTER_MIN_SIMILARITY: f32 = 0.5;
/// Minimum weight to include a canvas edge
const EDGE_MIN_WEIGHT: f32 = 0.2;
/// Maximum number of bind parameters per query (SQLite default limit is 999)
const MAX_SQL_VARS: usize = 400;
/// Skip edge computation when total atom count across nodes exceeds this
const MAX_ATOMS_FOR_EDGES: usize = 2000;
/// Maximum tags to attempt semantic clustering on (above this, use count-based grouping)
const MAX_TAGS_FOR_CLUSTERING: usize = 200;
/// Target number of groups when doing count-based grouping
const COUNT_GROUP_TARGET: usize = 15;

// ==================== Tag Tree (precomputed) ====================

/// Precomputed tag tree data — loaded once per request, reused across functions.
struct TagTree {
    /// (id, name, parent_id) for every tag
    all_tags: Vec<(String, String, Option<String>)>,
    /// tag_id → number of atoms directly tagged (not counting descendants)
    direct_counts: HashMap<String, i32>,
    /// tag_id → total atom count including all descendant tags (computed in one pass)
    transitive_counts: HashMap<String, i32>,
    /// tag_id → list of direct child tag IDs
    children_map: HashMap<String, Vec<String>>,
    /// tag_id → tag name (for fast lookup)
    tag_names: HashMap<String, String>,
}

impl TagTree {
    fn load(conn: &Connection) -> Result<Self, AtomicCoreError> {
        let mut stmt = conn.prepare("SELECT id, name, parent_id FROM tags ORDER BY name")?;
        let all_tags: Vec<(String, String, Option<String>)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut count_stmt =
            conn.prepare("SELECT tag_id, COUNT(*) FROM atom_tags GROUP BY tag_id")?;
        let direct_counts: HashMap<String, i32> = count_stmt
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        // Build children map
        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        for (id, _, parent) in &all_tags {
            if let Some(p) = parent {
                children_map
                    .entry(p.clone())
                    .or_default()
                    .push(id.clone());
            }
        }

        // Build name map
        let tag_names: HashMap<String, String> = all_tags
            .iter()
            .map(|(id, name, _)| (id.clone(), name.clone()))
            .collect();

        // Single-pass transitive counts (DFS with memoization)
        let mut transitive_counts: HashMap<String, i32> = HashMap::new();
        for (id, _, _) in &all_tags {
            compute_transitive_cached(id, &children_map, &direct_counts, &mut transitive_counts);
        }

        Ok(TagTree {
            all_tags,
            direct_counts,
            transitive_counts,
            children_map,
            tag_names,
        })
    }

    fn has_children(&self, tag_id: &str) -> bool {
        self.children_map
            .get(tag_id)
            .map_or(false, |c| !c.is_empty())
    }

    fn transitive_count(&self, tag_id: &str) -> i32 {
        self.transitive_counts.get(tag_id).copied().unwrap_or(0)
    }

    fn name(&self, tag_id: &str) -> String {
        self.tag_names
            .get(tag_id)
            .cloned()
            .unwrap_or_else(|| tag_id.to_string())
    }

    /// Get all descendant tag IDs (inclusive of the given tag) using the children_map.
    /// Pure Rust traversal — no SQL.
    fn descendant_tag_ids(&self, tag_id: &str) -> Vec<String> {
        let mut result = vec![tag_id.to_string()];
        let mut stack = vec![tag_id.to_string()];
        while let Some(tid) = stack.pop() {
            if let Some(kids) = self.children_map.get(&tid) {
                for kid in kids {
                    result.push(kid.clone());
                    stack.push(kid.clone());
                }
            }
        }
        result
    }
}

fn compute_transitive_cached(
    tag_id: &str,
    children_map: &HashMap<String, Vec<String>>,
    direct_counts: &HashMap<String, i32>,
    cache: &mut HashMap<String, i32>,
) -> i32 {
    if let Some(&cached) = cache.get(tag_id) {
        return cached;
    }

    let own = direct_counts.get(tag_id).copied().unwrap_or(0);
    let child_sum: i32 = children_map
        .get(tag_id)
        .map(|kids| {
            kids.iter()
                .map(|kid| compute_transitive_cached(kid, children_map, direct_counts, cache))
                .sum()
        })
        .unwrap_or(0);

    let total = own + child_sum;
    cache.insert(tag_id.to_string(), total);
    total
}

// ==================== Public Entry Point ====================

/// Get a single level of the hierarchical canvas.
pub fn get_canvas_level(
    conn: &Connection,
    parent_id: Option<&str>,
    children_hint: Option<Vec<String>>,
) -> Result<CanvasLevel, AtomicCoreError> {
    match (parent_id, &children_hint) {
        // Root level: tag categories
        (None, _) => build_root_level(conn),
        // Aggregate cluster drill-down: frontend tells us which IDs to show
        (Some(pid), Some(hint)) => build_hint_level(conn, pid, hint),
        // Regular tag/category drill-down
        (Some(pid), None) => build_tag_level(conn, pid),
    }
}

// ==================== Level Builders ====================

/// Root level: show semantic clusters of all atoms
fn build_root_level(conn: &Connection) -> Result<CanvasLevel, AtomicCoreError> {
    // Compute semantic clusters on-demand from all semantic edges
    let clusters = clustering::compute_atom_clusters(conn, CLUSTER_MIN_SIMILARITY, 3)
        .map_err(|e| AtomicCoreError::Configuration(e))?;

    let mut nodes: Vec<CanvasNode> = Vec::new();
    let mut clustered_atom_ids: HashSet<String> = HashSet::new();

    for cluster in &clusters {
        for aid in &cluster.atom_ids {
            clustered_atom_ids.insert(aid.clone());
        }

        let label = if cluster.dominant_tags.len() >= 2 {
            format!("{}, {}", cluster.dominant_tags[0], cluster.dominant_tags[1])
        } else if !cluster.dominant_tags.is_empty() {
            cluster.dominant_tags[0].clone()
        } else {
            format!("Cluster {}", cluster.cluster_id + 1)
        };

        nodes.push(CanvasNode {
            id: format!("cluster:{}", cluster.cluster_id),
            node_type: CanvasNodeType::SemanticCluster,
            label,
            atom_count: cluster.atom_ids.len() as i32,
            children_ids: cluster.atom_ids.clone(),
            dominant_tags: cluster.dominant_tags.clone(),
            centroid: None,
        });
    }

    // Find unclustered atoms (no semantic edges or below min_cluster_size)
    let unclustered_ids = get_unclustered_atom_ids(conn, &clustered_atom_ids)?;

    if !unclustered_ids.is_empty() {
        if unclustered_ids.len() <= MAX_ATOMS_PER_LEVEL {
            // Few enough to show individually
            let mut atom_nodes = build_flat_atom_nodes(conn, &unclustered_ids)?;
            nodes.append(&mut atom_nodes);
        } else {
            // Too many — wrap in a single "Unclustered" bubble
            let dominant = get_dominant_tags_for_atoms(conn, &unclustered_ids).unwrap_or_default();
            nodes.push(CanvasNode {
                id: "cluster:unclustered".to_string(),
                node_type: CanvasNodeType::SemanticCluster,
                label: "Unclustered".to_string(),
                atom_count: unclustered_ids.len() as i32,
                children_ids: unclustered_ids,
                dominant_tags: dominant,
                centroid: None,
            });
        }
    }

    // Compute inter-cluster edges
    let edges = compute_edges_between_nodes_simple(conn, &nodes)?;

    Ok(CanvasLevel {
        parent_id: None,
        parent_label: None,
        breadcrumb: vec![],
        nodes,
        edges,
    })
}

/// Get atom IDs not present in any cluster
fn get_unclustered_atom_ids(
    conn: &Connection,
    clustered_ids: &HashSet<String>,
) -> Result<Vec<String>, AtomicCoreError> {
    if clustered_ids.is_empty() {
        let mut stmt = conn.prepare("SELECT id FROM atoms ORDER BY updated_at DESC")?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        return Ok(ids);
    }

    let clustered_vec: Vec<String> = clustered_ids.iter().cloned().collect();
    populate_temp_table(conn, "_clustered_atoms", &clustered_vec)?;

    let mut stmt = conn.prepare(
        "SELECT id FROM atoms WHERE id NOT IN (SELECT id FROM _clustered_atoms)
         ORDER BY updated_at DESC",
    )?;
    let ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(ids)
}

/// Tag/Category drill-down: show children of a tag
fn build_tag_level(conn: &Connection, tag_id: &str) -> Result<CanvasLevel, AtomicCoreError> {
    if tag_id == "untagged" {
        return build_untagged_level(conn);
    }

    let tree = TagTree::load(conn)?;

    let (parent_name, _parent_parent_id) = conn
        .query_row(
            "SELECT name, parent_id FROM tags WHERE id = ?1",
            [tag_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .map_err(|_| AtomicCoreError::NotFound(format!("Tag {} not found", tag_id)))?;

    let breadcrumb = build_breadcrumb(conn, tag_id)?;

    // Find direct children of this tag
    let child_ids: Vec<String> = tree
        .children_map
        .get(tag_id)
        .cloned()
        .unwrap_or_default();

    if !child_ids.is_empty() {
        // This tag has child tags — show them as nodes
        let mut tag_nodes: Vec<(CanvasNode, i32)> = child_ids
            .iter()
            .map(|id| {
                let count = tree.transitive_count(id);
                let node_type = if tree.has_children(id) {
                    CanvasNodeType::Category
                } else {
                    CanvasNodeType::Tag
                };
                (
                    CanvasNode {
                        id: id.clone(),
                        node_type,
                        label: tree.name(id),
                        atom_count: count,
                        children_ids: vec![],
                        dominant_tags: vec![],
                        centroid: None,
                    },
                    count,
                )
            })
            .filter(|(_, count)| *count > 0)
            .collect();

        tag_nodes.sort_by(|a, b| b.1.cmp(&a.1));

        let mut nodes: Vec<CanvasNode>;

        if tag_nodes.len() <= MAX_TAGS_PER_LEVEL {
            nodes = tag_nodes.into_iter().map(|(n, _)| n).collect();
        } else {
            let (top, rest) = tag_nodes.split_at(TOP_TAGS_SHOWN);
            nodes = top.iter().map(|(n, _)| n.clone()).collect();

            if rest.len() <= MAX_TAGS_FOR_CLUSTERING {
                // Small enough for semantic clustering
                let rest_ids: Vec<String> = rest.iter().map(|(n, _)| n.id.clone()).collect();
                let cluster_nodes =
                    cluster_tags_by_similarity(conn, &rest_ids, &tree, tag_id)?;
                nodes.extend(cluster_nodes);
            } else {
                // Too many for semantic clustering — group by count ranking (O(n), no SQL)
                let group_nodes = group_tags_by_count(rest, &tree, tag_id);
                nodes.extend(group_nodes);
            }
        }

        // Also add atoms directly tagged with this tag (not just children)
        let direct_atom_count = tree.direct_counts.get(tag_id).copied().unwrap_or(0);
        if direct_atom_count > 0 {
            nodes.push(CanvasNode {
                id: format!("direct:{}", tag_id),
                node_type: CanvasNodeType::Tag,
                label: format!("{} (direct)", parent_name),
                atom_count: direct_atom_count,
                children_ids: vec![],
                dominant_tags: vec![],
                centroid: None,
            });
        }

        let edges = compute_edges_if_small(conn, &nodes)?;

        Ok(CanvasLevel {
            parent_id: Some(tag_id.to_string()),
            parent_label: Some(parent_name),
            breadcrumb,
            nodes,
            edges,
        })
    } else {
        // Leaf tag — show atoms
        build_atoms_for_tag(conn, tag_id, &parent_name, &breadcrumb)
    }
}

/// Show untagged atoms
fn build_untagged_level(conn: &Connection) -> Result<CanvasLevel, AtomicCoreError> {
    let breadcrumb = vec![BreadcrumbEntry {
        id: "untagged".to_string(),
        label: "Untagged".to_string(),
    }];

    let mut stmt = conn.prepare(
        "SELECT id, SUBSTR(content, 1, 100) FROM atoms
         WHERE id NOT IN (SELECT atom_id FROM atom_tags)
         ORDER BY updated_at DESC",
    )?;

    let atoms: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    if atoms.len() <= MAX_ATOMS_PER_LEVEL {
        let atom_ids: Vec<String> = atoms.iter().map(|(id, _)| id.clone()).collect();
        let nodes = build_flat_atom_nodes(conn, &atom_ids)?;
        let edges = compute_edges_for_atom_set(conn, &atom_ids)?;

        Ok(CanvasLevel {
            parent_id: Some("untagged".to_string()),
            parent_label: Some("Untagged".to_string()),
            breadcrumb,
            nodes,
            edges,
        })
    } else {
        let atom_ids: Vec<String> = atoms.iter().map(|(id, _)| id.clone()).collect();
        let nodes = cluster_atoms_into_groups(conn, &atom_ids, "untagged")?;
        let edges = compute_edges_between_nodes_simple(conn, &nodes)?;

        Ok(CanvasLevel {
            parent_id: Some("untagged".to_string()),
            parent_label: Some("Untagged".to_string()),
            breadcrumb,
            nodes,
            edges,
        })
    }
}

/// Show atoms for a leaf tag
fn build_atoms_for_tag(
    conn: &Connection,
    tag_id: &str,
    tag_name: &str,
    breadcrumb: &[BreadcrumbEntry],
) -> Result<CanvasLevel, AtomicCoreError> {
    let actual_tag_id = tag_id.strip_prefix("direct:").unwrap_or(tag_id);

    let mut stmt = conn.prepare(
        "SELECT a.id, SUBSTR(a.content, 1, 100) FROM atoms a
         INNER JOIN atom_tags at ON a.id = at.atom_id
         WHERE at.tag_id = ?1
         ORDER BY a.updated_at DESC",
    )?;

    let atoms: Vec<(String, String)> = stmt
        .query_map([actual_tag_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    if atoms.len() <= MAX_ATOMS_PER_LEVEL {
        let atom_ids: Vec<String> = atoms.iter().map(|(id, _)| id.clone()).collect();
        let nodes = build_flat_atom_nodes(conn, &atom_ids)?;
        let edges = compute_edges_for_atom_set(conn, &atom_ids)?;

        Ok(CanvasLevel {
            parent_id: Some(tag_id.to_string()),
            parent_label: Some(tag_name.to_string()),
            breadcrumb: breadcrumb.to_vec(),
            nodes,
            edges,
        })
    } else {
        let atom_ids: Vec<String> = atoms.iter().map(|(id, _)| id.clone()).collect();
        let nodes = cluster_atoms_into_groups(conn, &atom_ids, tag_id)?;
        let edges = compute_edges_between_nodes_simple(conn, &nodes)?;

        Ok(CanvasLevel {
            parent_id: Some(tag_id.to_string()),
            parent_label: Some(tag_name.to_string()),
            breadcrumb: breadcrumb.to_vec(),
            nodes,
            edges,
        })
    }
}

/// Handle drill-down into a SemanticCluster (frontend provides children_ids)
fn build_hint_level(
    conn: &Connection,
    parent_id: &str,
    hint_ids: &[String],
) -> Result<CanvasLevel, AtomicCoreError> {
    if hint_ids.is_empty() {
        return Ok(CanvasLevel {
            parent_id: Some(parent_id.to_string()),
            parent_label: None,
            breadcrumb: vec![],
            nodes: vec![],
            edges: vec![],
        });
    }

    // Determine if these are tag IDs or atom IDs by checking the tags table
    let found_tags: HashMap<String, String> = batch_lookup_tag_names(conn, hint_ids)?;

    // Build breadcrumb from parent
    let breadcrumb = if parent_id.starts_with("cluster:") {
        let parts: Vec<&str> = parent_id.split(':').collect();
        if parts.len() >= 2 {
            let ancestor_id = parts[1];
            let mut bc = build_breadcrumb(conn, ancestor_id).unwrap_or_default();
            bc.push(BreadcrumbEntry {
                id: parent_id.to_string(),
                label: "Cluster".to_string(),
            });
            bc
        } else {
            vec![]
        }
    } else {
        build_breadcrumb(conn, parent_id).unwrap_or_default()
    };

    let parent_label = breadcrumb.last().map(|b| b.label.clone());

    if found_tags.len() == hint_ids.len() {
        // All are tags — build tag nodes with counts
        let tree = TagTree::load(conn)?;

        let mut tag_nodes: Vec<(CanvasNode, i32)> = hint_ids
            .iter()
            .filter_map(|id| {
                let name = found_tags.get(id)?;
                let count = tree.transitive_count(id);
                let node_type = if tree.has_children(id) {
                    CanvasNodeType::Category
                } else {
                    CanvasNodeType::Tag
                };
                Some((
                    CanvasNode {
                        id: id.clone(),
                        node_type,
                        label: name.clone(),
                        atom_count: count,
                        children_ids: vec![],
                        dominant_tags: vec![],
                        centroid: None,
                    },
                    count,
                ))
            })
            .filter(|(_, count)| *count > 0)
            .collect();

        tag_nodes.sort_by(|a, b| b.1.cmp(&a.1));

        let nodes: Vec<CanvasNode> = if tag_nodes.len() <= MAX_TAGS_PER_LEVEL {
            tag_nodes.into_iter().map(|(n, _)| n).collect()
        } else {
            // Apply same top-N + grouping as build_tag_level
            let (top, rest) = tag_nodes.split_at(TOP_TAGS_SHOWN);
            let mut result: Vec<CanvasNode> = top.iter().map(|(n, _)| n.clone()).collect();

            if rest.len() <= MAX_TAGS_FOR_CLUSTERING {
                let rest_ids: Vec<String> = rest.iter().map(|(n, _)| n.id.clone()).collect();
                let cluster_nodes =
                    cluster_tags_by_similarity(conn, &rest_ids, &tree, parent_id)?;
                result.extend(cluster_nodes);
            } else {
                let group_nodes = group_tags_by_count(rest, &tree, parent_id);
                result.extend(group_nodes);
            }
            result
        };

        let edges = compute_edges_if_small(conn, &nodes)?;

        Ok(CanvasLevel {
            parent_id: Some(parent_id.to_string()),
            parent_label,
            breadcrumb,
            nodes,
            edges,
        })
    } else {
        // Assume atoms
        let atom_ids = hint_ids.to_vec();
        if atom_ids.len() <= MAX_ATOMS_PER_LEVEL {
            let nodes = build_flat_atom_nodes(conn, &atom_ids)?;

            let edges = compute_edges_for_atom_set(conn, &atom_ids)?;

            Ok(CanvasLevel {
                parent_id: Some(parent_id.to_string()),
                parent_label,
                breadcrumb,
                nodes,
                edges,
            })
        } else {
            let nodes = cluster_atoms_into_groups(conn, &atom_ids, parent_id)?;
            let edges = compute_edges_between_nodes_simple(conn, &nodes)?;

            Ok(CanvasLevel {
                parent_id: Some(parent_id.to_string()),
                parent_label,
                breadcrumb,
                nodes,
                edges,
            })
        }
    }
}

// ==================== Helper Functions ====================

/// Build breadcrumb path from root to the given tag
fn build_breadcrumb(
    conn: &Connection,
    tag_id: &str,
) -> Result<Vec<BreadcrumbEntry>, AtomicCoreError> {
    let mut path = Vec::new();
    let mut current_id = Some(tag_id.to_string());

    while let Some(id) = current_id {
        let result = conn.query_row(
            "SELECT name, parent_id FROM tags WHERE id = ?1",
            [&id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        );

        match result {
            Ok((name, parent_id)) => {
                path.push(BreadcrumbEntry {
                    id: id.clone(),
                    label: name,
                });
                current_id = parent_id;
            }
            Err(_) => break,
        }
    }

    path.reverse();
    Ok(path)
}

// ==================== Clustering ====================

/// Cluster a set of tags by similarity when there are too many to display.
///
/// Uses batch atom-tag resolution (one SQL query) instead of per-tag recursive CTEs.
fn cluster_tags_by_similarity(
    conn: &Connection,
    tag_ids: &[String],
    tree: &TagTree,
    parent_id: &str,
) -> Result<Vec<CanvasNode>, AtomicCoreError> {
    // Batch-resolve atom IDs for all tags using the precomputed children_map.
    // Expand each tag to its full descendant set (Rust-side, no SQL), then do
    // a single batch query for atom_tags.
    let tag_to_atoms = batch_get_atom_ids_for_tags(conn, tag_ids, tree)?;

    // Collect all unique atom IDs
    let mut all_atom_ids: Vec<String> = tag_to_atoms.values().flatten().cloned().collect();
    all_atom_ids.sort();
    all_atom_ids.dedup();

    // Load semantic edges within this atom set
    let edges = load_semantic_edges_for_atoms(conn, &all_atom_ids)?;

    // Build atom-to-tag mapping
    let mut atom_to_tag: HashMap<String, String> = HashMap::new();
    for (tid, atoms) in &tag_to_atoms {
        for aid in atoms {
            atom_to_tag.insert(aid.clone(), tid.clone());
        }
    }

    // Convert atom-level edges to tag-level edges
    let mut tag_edge_counts: HashMap<(String, String), (f32, i32)> = HashMap::new();
    for (src, tgt, score) in &edges {
        let src_tag = atom_to_tag.get(src);
        let tgt_tag = atom_to_tag.get(tgt);
        if let (Some(st), Some(tt)) = (src_tag, tgt_tag) {
            if st != tt {
                let key = if st < tt {
                    (st.clone(), tt.clone())
                } else {
                    (tt.clone(), st.clone())
                };
                let entry = tag_edge_counts.entry(key).or_insert((0.0, 0));
                entry.0 += score;
                entry.1 += 1;
            }
        }
    }

    let tag_edges: Vec<(String, String, f32)> = tag_edge_counts
        .into_iter()
        .map(|((a, b), (total_score, count))| (a, b, total_score / count as f32))
        .collect();

    // Run label propagation on tag-level edges
    let labels = clustering::label_propagation(&tag_edges);
    let groups = clustering::group_labels_into_clusters(&labels, 1);

    // Tags not in any edge get their own single-element groups
    let clustered_ids: HashSet<&String> = labels.keys().collect();
    let mut extra_groups: Vec<Vec<String>> = tag_ids
        .iter()
        .filter(|id| !clustered_ids.contains(id))
        .map(|id| vec![id.clone()])
        .collect();

    let mut all_groups = groups;
    all_groups.append(&mut extra_groups);

    // Convert groups into CanvasNodes
    let mut nodes = Vec::new();
    for (i, group) in all_groups.iter().enumerate() {
        if group.len() == 1 {
            let tid = &group[0];
            let count = tree.transitive_count(tid);
            nodes.push(CanvasNode {
                id: tid.clone(),
                node_type: CanvasNodeType::Tag,
                label: tree.name(tid),
                atom_count: count,
                children_ids: vec![],
                dominant_tags: vec![],
                centroid: None,
            });
        } else {
            let total_count: i32 = group.iter().map(|tid| tree.transitive_count(tid)).sum();

            // Get top 2 tag names for labeling
            let mut tag_counts: Vec<(&String, i32)> = group
                .iter()
                .map(|tid| (tid, tree.transitive_count(tid)))
                .collect();
            tag_counts.sort_by(|a, b| b.1.cmp(&a.1));
            let dominant: Vec<String> = tag_counts
                .iter()
                .take(2)
                .map(|(tid, _)| tree.name(tid))
                .collect();

            let label = if dominant.len() >= 2 {
                format!("{}, {} +{}", dominant[0], dominant[1], group.len() - 2)
            } else if !dominant.is_empty() {
                format!("{} +{}", dominant[0], group.len() - 1)
            } else {
                format!("Cluster {}", i + 1)
            };

            nodes.push(CanvasNode {
                id: format!("cluster:{}:{}", parent_id, i),
                node_type: CanvasNodeType::SemanticCluster,
                label,
                atom_count: total_count,
                children_ids: group.clone(),
                dominant_tags: dominant,
                centroid: None,
            });
        }
    }

    Ok(nodes)
}

/// Fast count-based grouping for very large tag sets.
/// Groups tags (already sorted by count desc) into ~15 chunks.
/// O(n) with zero SQL — just slices the sorted array.
fn group_tags_by_count(
    sorted_tags: &[(CanvasNode, i32)],
    tree: &TagTree,
    parent_id: &str,
) -> Vec<CanvasNode> {
    if sorted_tags.is_empty() {
        return vec![];
    }

    let group_size = (sorted_tags.len() + COUNT_GROUP_TARGET - 1) / COUNT_GROUP_TARGET;

    sorted_tags
        .chunks(group_size)
        .enumerate()
        .map(|(i, chunk)| {
            let total_count: i32 = chunk.iter().map(|(_, c)| *c).sum();
            let children_ids: Vec<String> = chunk.iter().map(|(n, _)| n.id.clone()).collect();

            // Use top 2 tag names from this chunk as label
            let dominant: Vec<String> = chunk
                .iter()
                .take(2)
                .map(|(n, _)| tree.name(&n.id))
                .collect();

            let label = if dominant.len() >= 2 {
                format!("{}, {} +{}", dominant[0], dominant[1], chunk.len() - 2)
            } else if !dominant.is_empty() {
                format!("{} +{}", dominant[0], chunk.len() - 1)
            } else {
                format!("Group {}", i + 1)
            };

            CanvasNode {
                id: format!("cluster:{}:{}", parent_id, i),
                node_type: CanvasNodeType::SemanticCluster,
                label,
                atom_count: total_count,
                children_ids,
                dominant_tags: dominant,
                centroid: None,
            }
        })
        .collect()
}

/// Cluster atoms into groups when there are too many for a flat view
fn cluster_atoms_into_groups(
    conn: &Connection,
    atom_ids: &[String],
    parent_id: &str,
) -> Result<Vec<CanvasNode>, AtomicCoreError> {
    let edges = load_semantic_edges_for_atoms(conn, atom_ids)?;

    if edges.is_empty() {
        return build_flat_atom_nodes(conn, atom_ids);
    }

    let labels = clustering::label_propagation(&edges);
    let groups = clustering::group_labels_into_clusters(&labels, 2);

    let clustered: HashSet<&String> = labels.keys().collect();
    let unclustered: Vec<String> = atom_ids
        .iter()
        .filter(|id| !clustered.contains(id))
        .cloned()
        .collect();

    let mut nodes = Vec::new();

    for (i, group) in groups.iter().enumerate() {
        if group.len() <= 3 {
            let mut atom_nodes = build_flat_atom_nodes(conn, group)?;
            nodes.append(&mut atom_nodes);
        } else {
            let dominant = get_dominant_tags_for_atoms(conn, group)?;
            let label = if dominant.len() >= 2 {
                format!("{}, {}", dominant[0], dominant[1])
            } else if !dominant.is_empty() {
                dominant[0].clone()
            } else {
                format!("Group {}", i + 1)
            };

            nodes.push(CanvasNode {
                id: format!("cluster:{}:{}", parent_id, i),
                node_type: CanvasNodeType::SemanticCluster,
                label,
                atom_count: group.len() as i32,
                children_ids: group.clone(),
                dominant_tags: dominant,
                centroid: None,
            });
        }
    }

    if !unclustered.is_empty() {
        let limit = MAX_ATOMS_PER_LEVEL
            .saturating_sub(nodes.len())
            .min(unclustered.len());
        let mut atom_nodes = build_flat_atom_nodes(conn, &unclustered[..limit])?;
        nodes.append(&mut atom_nodes);

        if unclustered.len() > limit {
            let remaining = &unclustered[limit..];
            let dominant = get_dominant_tags_for_atoms(conn, remaining)?;
            nodes.push(CanvasNode {
                id: format!("cluster:{}:unclustered", parent_id),
                node_type: CanvasNodeType::SemanticCluster,
                label: "Other".to_string(),
                atom_count: remaining.len() as i32,
                children_ids: remaining.to_vec(),
                dominant_tags: dominant,
                centroid: None,
            });
        }
    }

    Ok(nodes)
}

// ==================== Batch SQL Helpers ====================

/// Populate a temp table with IDs, batching inserts to stay under SQLite limits.
fn populate_temp_table(
    conn: &Connection,
    table_name: &str,
    ids: &[String],
) -> Result<(), AtomicCoreError> {
    // table_name is always a compile-time literal from internal code
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE IF NOT EXISTS {0} (id TEXT PRIMARY KEY);
         DELETE FROM {0};",
        table_name
    ))?;

    for chunk in ids.chunks(MAX_SQL_VARS) {
        let placeholders = chunk.iter().map(|_| "(?)").collect::<Vec<_>>().join(",");
        let sql = format!(
            "INSERT OR IGNORE INTO {} (id) VALUES {}",
            table_name, placeholders
        );
        conn.execute(&sql, rusqlite::params_from_iter(chunk.iter()))?;
    }

    Ok(())
}

/// Batch-resolve atom IDs for multiple tags using the precomputed children_map.
/// One SQL query instead of N recursive CTEs.
fn batch_get_atom_ids_for_tags(
    conn: &Connection,
    tag_ids: &[String],
    tree: &TagTree,
) -> Result<HashMap<String, Vec<String>>, AtomicCoreError> {
    // Expand each tag to its full descendant set (pure Rust, using children_map)
    let mut descendant_to_original: HashMap<String, String> = HashMap::new();
    for tid in tag_ids {
        for desc in tree.descendant_tag_ids(tid) {
            // If multiple originals claim the same descendant, first one wins
            descendant_to_original.entry(desc).or_insert_with(|| tid.clone());
        }
    }

    let all_desc_ids: Vec<String> = descendant_to_original.keys().cloned().collect();

    // Single batch query for all atom_tags
    populate_temp_table(conn, "_canvas_tag_ids", &all_desc_ids)?;

    let mut stmt = conn.prepare(
        "SELECT tag_id, atom_id FROM atom_tags
         WHERE tag_id IN (SELECT id FROM _canvas_tag_ids)",
    )?;

    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;

    // Group by original tag
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    for tid in tag_ids {
        result.insert(tid.clone(), Vec::new());
    }
    for (desc_tag, atom_id) in rows {
        if let Some(original) = descendant_to_original.get(&desc_tag) {
            result.entry(original.clone()).or_default().push(atom_id);
        }
    }

    Ok(result)
}

/// Batch lookup tag names for a set of IDs
fn batch_lookup_tag_names(
    conn: &Connection,
    ids: &[String],
) -> Result<HashMap<String, String>, AtomicCoreError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    if ids.len() <= MAX_SQL_VARS {
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!("SELECT id, name FROM tags WHERE id IN ({})", placeholders);
        let mut stmt = conn.prepare(&query)?;
        let result = stmt
            .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        return Ok(result);
    }

    populate_temp_table(conn, "_canvas_tag_ids", ids)?;
    let mut stmt = conn.prepare(
        "SELECT id, name FROM tags WHERE id IN (SELECT id FROM _canvas_tag_ids)",
    )?;
    let result = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(result)
}

/// Batch lookup atom content snippets
fn batch_lookup_atom_snippets(
    conn: &Connection,
    ids: &[String],
) -> Result<HashMap<String, String>, AtomicCoreError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    if ids.len() <= MAX_SQL_VARS {
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, SUBSTR(content, 1, 100) FROM atoms WHERE id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&query)?;
        let result = stmt
            .query_map(rusqlite::params_from_iter(ids.iter()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        return Ok(result);
    }

    populate_temp_table(conn, "_canvas_atom_ids", ids)?;
    let mut stmt = conn.prepare(
        "SELECT id, SUBSTR(content, 1, 100) FROM atoms
         WHERE id IN (SELECT id FROM _canvas_atom_ids)",
    )?;
    let result = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(result)
}

/// Build flat CanvasNode::Atom entries for a set of atom IDs
fn build_flat_atom_nodes(
    conn: &Connection,
    atom_ids: &[String],
) -> Result<Vec<CanvasNode>, AtomicCoreError> {
    if atom_ids.is_empty() {
        return Ok(vec![]);
    }

    let atoms = batch_lookup_atom_snippets(conn, atom_ids)?;
    let atom_tags = batch_lookup_atom_tags(conn, atom_ids)?;

    Ok(atoms
        .into_iter()
        .map(|(id, content)| {
            let tags = atom_tags.get(&id).cloned().unwrap_or_default();
            CanvasNode {
                id,
                node_type: CanvasNodeType::Atom,
                label: snippet_label(&content),
                atom_count: 1,
                children_ids: vec![],
                dominant_tags: tags,
                centroid: None,
            }
        })
        .collect())
}

/// Batch lookup tag names for each atom (returns atom_id → vec of tag names)
fn batch_lookup_atom_tags(
    conn: &Connection,
    atom_ids: &[String],
) -> Result<HashMap<String, Vec<String>>, AtomicCoreError> {
    if atom_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let query_and_collect = |stmt: &mut rusqlite::Statement, params: &[&dyn rusqlite::ToSql]| -> Result<HashMap<String, Vec<String>>, AtomicCoreError> {
        let mut result: HashMap<String, Vec<String>> = HashMap::new();
        let rows = stmt.query_map(params, |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (atom_id, tag_name) = row?;
            result.entry(atom_id).or_default().push(tag_name);
        }
        Ok(result)
    };

    if atom_ids.len() <= MAX_SQL_VARS {
        let placeholders = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT at.atom_id, t.name FROM atom_tags at
             JOIN tags t ON at.tag_id = t.id
             WHERE at.atom_id IN ({})
             ORDER BY t.name",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = atom_ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        return query_and_collect(&mut stmt, &params);
    }

    populate_temp_table(conn, "_canvas_atom_ids", atom_ids)?;
    let mut stmt = conn.prepare(
        "SELECT at.atom_id, t.name FROM atom_tags at
         JOIN tags t ON at.tag_id = t.id
         WHERE at.atom_id IN (SELECT id FROM _canvas_atom_ids)
         ORDER BY t.name",
    )?;
    query_and_collect(&mut stmt, &[])
}

// ==================== Edge Computation ====================

/// Load semantic edges where both endpoints are in the given atom set.
fn load_semantic_edges_for_atoms(
    conn: &Connection,
    atom_ids: &[String],
) -> Result<Vec<(String, String, f32)>, AtomicCoreError> {
    if atom_ids.is_empty() {
        return Ok(vec![]);
    }

    // For small sets, use direct IN clause (faster, no temp table overhead)
    if atom_ids.len() <= MAX_SQL_VARS {
        let placeholders = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT source_atom_id, target_atom_id, similarity_score
             FROM semantic_edges
             WHERE source_atom_id IN ({0}) AND target_atom_id IN ({0})
             AND similarity_score >= ?",
            placeholders
        );

        let mut params: Vec<String> = atom_ids.to_vec();
        params.extend(atom_ids.to_vec());
        params.push(CLUSTER_MIN_SIMILARITY.to_string());

        let mut stmt = conn.prepare(&query)?;
        let edges = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f32>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        return Ok(edges);
    }

    // For large sets, use a temp table
    populate_temp_table(conn, "_canvas_atom_ids", atom_ids)?;

    let mut stmt = conn.prepare(
        "SELECT source_atom_id, target_atom_id, similarity_score
         FROM semantic_edges
         WHERE source_atom_id IN (SELECT id FROM _canvas_atom_ids)
         AND target_atom_id IN (SELECT id FROM _canvas_atom_ids)
         AND similarity_score >= ?",
    )?;

    let edges = stmt
        .query_map([CLUSTER_MIN_SIMILARITY], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f32>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(edges)
}

/// Compute edges between nodes only if total atom count is manageable.
/// For large datasets (root level with 34K atoms), skip — not useful.
fn compute_edges_if_small(
    conn: &Connection,
    nodes: &[CanvasNode],
) -> Result<Vec<CanvasEdge>, AtomicCoreError> {
    if nodes.len() <= 1 {
        return Ok(vec![]);
    }

    let total_atoms: i64 = nodes.iter().map(|n| n.atom_count as i64).sum();
    if total_atoms > MAX_ATOMS_FOR_EDGES as i64 {
        return Ok(vec![]);
    }

    compute_edges_between_nodes(conn, nodes)
}

/// Compute edges between sibling canvas nodes based on their shared semantic edges.
fn compute_edges_between_nodes(
    conn: &Connection,
    nodes: &[CanvasNode],
) -> Result<Vec<CanvasEdge>, AtomicCoreError> {
    if nodes.len() <= 1 {
        return Ok(vec![]);
    }

    // Build node-to-atom-ids mapping
    let mut node_atoms: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        let atom_ids = get_atom_ids_for_node(conn, node)?;
        node_atoms.insert(node.id.clone(), atom_ids);
    }

    let mut all_atom_ids: Vec<String> = node_atoms.values().flatten().cloned().collect();
    all_atom_ids.sort();
    all_atom_ids.dedup();

    if all_atom_ids.is_empty() {
        return Ok(vec![]);
    }

    let sem_edges = load_semantic_edges_for_atoms(conn, &all_atom_ids)?;

    // Build atom-to-node mapping
    let mut atom_to_node: HashMap<String, String> = HashMap::new();
    for (node_id, atoms) in &node_atoms {
        for aid in atoms {
            atom_to_node.insert(aid.clone(), node_id.clone());
        }
    }

    // Count cross-node edges
    let mut edge_data: HashMap<(String, String), (f32, i32)> = HashMap::new();
    for (src, tgt, score) in &sem_edges {
        let src_node = atom_to_node.get(src);
        let tgt_node = atom_to_node.get(tgt);
        if let (Some(sn), Some(tn)) = (src_node, tgt_node) {
            if sn != tn {
                let key = if sn < tn {
                    (sn.clone(), tn.clone())
                } else {
                    (tn.clone(), sn.clone())
                };
                let entry = edge_data.entry(key).or_insert((0.0, 0));
                entry.0 += score;
                entry.1 += 1;
            }
        }
    }

    let max_count = edge_data.values().map(|(_, c)| *c).max().unwrap_or(1) as f32;
    let edges: Vec<CanvasEdge> = edge_data
        .into_iter()
        .map(|((src, tgt), (_, count))| {
            let weight = (count as f32 / max_count).min(1.0);
            CanvasEdge {
                source_id: src,
                target_id: tgt,
                weight,
            }
        })
        .filter(|e| e.weight >= EDGE_MIN_WEIGHT)
        .collect();

    Ok(edges)
}

/// Simplified edge computation for atom-level nodes (direct semantic edge lookup)
fn compute_edges_for_atom_set(
    conn: &Connection,
    atom_ids: &[String],
) -> Result<Vec<CanvasEdge>, AtomicCoreError> {
    let edges = load_semantic_edges_for_atoms(conn, atom_ids)?;

    let max_score = edges.iter().map(|(_, _, s)| *s).fold(0.0f32, f32::max);
    if max_score == 0.0 {
        return Ok(vec![]);
    }

    Ok(edges
        .into_iter()
        .map(|(src, tgt, score)| CanvasEdge {
            source_id: src,
            target_id: tgt,
            weight: score / max_score,
        })
        .filter(|e| e.weight >= EDGE_MIN_WEIGHT)
        .collect())
}

/// Simple edge computation between canvas nodes (for cluster views)
fn compute_edges_between_nodes_simple(
    conn: &Connection,
    nodes: &[CanvasNode],
) -> Result<Vec<CanvasEdge>, AtomicCoreError> {
    if nodes.len() <= 1 {
        return Ok(vec![]);
    }

    let mut node_atoms: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        match node.node_type {
            CanvasNodeType::Atom => {
                node_atoms.insert(node.id.clone(), vec![node.id.clone()]);
            }
            CanvasNodeType::SemanticCluster => {
                node_atoms.insert(node.id.clone(), node.children_ids.clone());
            }
            _ => {}
        }
    }

    let mut all_atom_ids: Vec<String> = node_atoms.values().flatten().cloned().collect();
    all_atom_ids.sort();
    all_atom_ids.dedup();

    if all_atom_ids.is_empty() {
        return Ok(vec![]);
    }

    // Check total count before loading edges
    if all_atom_ids.len() > MAX_ATOMS_FOR_EDGES {
        return Ok(vec![]);
    }

    let sem_edges = load_semantic_edges_for_atoms(conn, &all_atom_ids)?;

    let mut atom_to_node: HashMap<String, String> = HashMap::new();
    for (node_id, atoms) in &node_atoms {
        for aid in atoms {
            atom_to_node.insert(aid.clone(), node_id.clone());
        }
    }

    let mut edge_data: HashMap<(String, String), i32> = HashMap::new();
    for (src, tgt, _) in &sem_edges {
        let src_node = atom_to_node.get(src);
        let tgt_node = atom_to_node.get(tgt);
        if let (Some(sn), Some(tn)) = (src_node, tgt_node) {
            if sn != tn {
                let key = if sn < tn {
                    (sn.clone(), tn.clone())
                } else {
                    (tn.clone(), sn.clone())
                };
                *edge_data.entry(key).or_insert(0) += 1;
            }
        }
    }

    let max_count = edge_data.values().max().copied().unwrap_or(1) as f32;
    Ok(edge_data
        .into_iter()
        .map(|((src, tgt), count)| CanvasEdge {
            source_id: src,
            target_id: tgt,
            weight: (count as f32 / max_count).min(1.0),
        })
        .filter(|e| e.weight >= EDGE_MIN_WEIGHT)
        .collect())
}

/// Get atom IDs belonging to a canvas node (used by edge computation)
fn get_atom_ids_for_node(
    conn: &Connection,
    node: &CanvasNode,
) -> Result<Vec<String>, AtomicCoreError> {
    match node.node_type {
        CanvasNodeType::Category | CanvasNodeType::Tag => {
            if node.id == "untagged" {
                let mut stmt = conn.prepare(
                    "SELECT id FROM atoms WHERE id NOT IN (SELECT atom_id FROM atom_tags)",
                )?;
                let ids = stmt
                    .query_map([], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(ids)
            } else if let Some(tid) = node.id.strip_prefix("direct:") {
                let mut stmt =
                    conn.prepare("SELECT atom_id FROM atom_tags WHERE tag_id = ?1")?;
                let ids = stmt
                    .query_map([tid], |row| row.get(0))?
                    .collect::<Result<Vec<String>, _>>()?;
                Ok(ids)
            } else {
                get_atom_ids_for_tag(conn, &node.id)
            }
        }
        CanvasNodeType::SemanticCluster => {
            // children_ids are either tag IDs or atom IDs
            let mut atoms = Vec::new();
            for child_id in &node.children_ids {
                let is_tag: bool = conn
                    .query_row(
                        "SELECT COUNT(*) FROM tags WHERE id = ?1",
                        [child_id],
                        |row| row.get::<_, i32>(0),
                    )
                    .map(|c| c > 0)
                    .unwrap_or(false);

                if is_tag {
                    atoms.extend(get_atom_ids_for_tag(conn, child_id)?);
                } else {
                    atoms.push(child_id.clone());
                }
            }
            Ok(atoms)
        }
        CanvasNodeType::Atom => Ok(vec![node.id.clone()]),
    }
}

/// Get atom IDs for a tag (including all descendant tags via recursive CTE)
fn get_atom_ids_for_tag(conn: &Connection, tag_id: &str) -> Result<Vec<String>, AtomicCoreError> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE descendant_tags(id) AS (
            SELECT ?1
            UNION ALL
            SELECT t.id FROM tags t
            INNER JOIN descendant_tags dt ON t.parent_id = dt.id
        )
        SELECT DISTINCT at.atom_id FROM atom_tags at
        WHERE at.tag_id IN (SELECT id FROM descendant_tags)",
    )?;

    let ids = stmt
        .query_map([tag_id], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    Ok(ids)
}

// ==================== Misc Helpers ====================

/// Get dominant tag names for a set of atom IDs
fn get_dominant_tags_for_atoms(
    conn: &Connection,
    atom_ids: &[String],
) -> Result<Vec<String>, AtomicCoreError> {
    if atom_ids.is_empty() {
        return Ok(vec![]);
    }

    if atom_ids.len() <= MAX_SQL_VARS {
        let placeholders = atom_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT t.name, COUNT(*) as cnt
             FROM atom_tags at
             JOIN tags t ON at.tag_id = t.id
             WHERE at.atom_id IN ({})
             AND t.parent_id IS NOT NULL
             GROUP BY t.id
             ORDER BY cnt DESC
             LIMIT 3",
            placeholders
        );

        let mut stmt = conn.prepare(&sql)?;
        let tags = stmt
            .query_map(rusqlite::params_from_iter(atom_ids.iter()), |row| {
                row.get::<_, String>(0)
            })?
            .filter_map(|r| r.ok())
            .collect();

        return Ok(tags);
    }

    populate_temp_table(conn, "_canvas_atom_ids", atom_ids)?;

    let mut stmt = conn.prepare(
        "SELECT t.name, COUNT(*) as cnt
         FROM atom_tags at
         JOIN tags t ON at.tag_id = t.id
         WHERE at.atom_id IN (SELECT id FROM _canvas_atom_ids)
         AND t.parent_id IS NOT NULL
         GROUP BY t.id
         ORDER BY cnt DESC
         LIMIT 3",
    )?;

    let tags = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tags)
}

/// Create a short label from atom content
fn snippet_label(content: &str) -> String {
    let first_line = content.lines().next().unwrap_or(content);
    let trimmed = first_line.trim().trim_start_matches('#').trim();
    if trimmed.len() > 60 {
        format!("{}...", &trimmed[..57])
    } else if trimmed.is_empty() {
        "Empty".to_string()
    } else {
        trimmed.to_string()
    }
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

    fn insert_atom(conn: &Connection, id: &str, content: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO atoms (id, content, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, content, now, now],
        )
        .unwrap();
    }

    fn insert_tag(conn: &Connection, id: &str, name: &str, parent_id: Option<&str>) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO tags (id, name, parent_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, name, parent_id, now],
        )
        .unwrap();
    }

    fn tag_atom(conn: &Connection, atom_id: &str, tag_id: &str) {
        conn.execute(
            "INSERT INTO atom_tags (atom_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![atom_id, tag_id],
        )
        .unwrap();
    }

    fn insert_semantic_edge(conn: &Connection, source: &str, target: &str, similarity: f32) {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO semantic_edges (id, source_atom_id, target_atom_id, similarity_score, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, source, target, similarity, now],
        )
        .unwrap();
    }

    #[test]
    fn test_root_level_shows_clusters() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();

        // Create atoms with semantic edges so they form a cluster
        insert_atom(&conn, "a1", "AI content");
        insert_atom(&conn, "a2", "ML content");
        insert_atom(&conn, "a3", "Deep learning content");
        insert_semantic_edge(&conn, "a1", "a2", 0.9);
        insert_semantic_edge(&conn, "a2", "a3", 0.85);
        insert_semantic_edge(&conn, "a1", "a3", 0.8);

        let level = get_canvas_level(&conn, None, None).unwrap();
        assert!(level.parent_id.is_none());
        assert!(!level.nodes.is_empty());

        // Should have a semantic cluster node containing all 3 atoms
        let cluster = level
            .nodes
            .iter()
            .find(|n| n.node_type == CanvasNodeType::SemanticCluster);
        assert!(cluster.is_some());
        assert_eq!(cluster.unwrap().atom_count, 3);
    }

    #[test]
    fn test_root_level_shows_unclustered_atoms() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();

        // Single atom with no edges — should appear as individual atom node
        insert_atom(&conn, "a1", "Lonely content");

        let level = get_canvas_level(&conn, None, None).unwrap();
        assert_eq!(level.nodes.len(), 1);
        assert_eq!(level.nodes[0].node_type, CanvasNodeType::Atom);
    }

    #[test]
    fn test_cluster_drilldown_shows_atoms() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();

        insert_atom(&conn, "a1", "AI content");
        insert_atom(&conn, "a2", "ML content");
        insert_atom(&conn, "a3", "Deep learning content");
        insert_semantic_edge(&conn, "a1", "a2", 0.9);
        insert_semantic_edge(&conn, "a2", "a3", 0.85);

        // Drill into cluster with children_hint
        let children = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
        let level = get_canvas_level(&conn, Some("cluster:0"), Some(children)).unwrap();
        assert_eq!(level.parent_id.as_deref(), Some("cluster:0"));
        assert_eq!(level.nodes.len(), 3);
        assert!(level
            .nodes
            .iter()
            .all(|n| n.node_type == CanvasNodeType::Atom));
    }

    #[test]
    fn test_category_drilldown_shows_children() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();

        insert_tag(&conn, "cat1", "Topics", None);
        insert_tag(&conn, "t1", "AI", Some("cat1"));
        insert_tag(&conn, "t2", "Physics", Some("cat1"));
        insert_atom(&conn, "a1", "AI content");
        insert_atom(&conn, "a2", "Physics content");
        tag_atom(&conn, "a1", "t1");
        tag_atom(&conn, "a2", "t2");

        let level = get_canvas_level(&conn, Some("cat1"), None).unwrap();
        assert_eq!(level.parent_id.as_deref(), Some("cat1"));
        assert_eq!(level.parent_label.as_deref(), Some("Topics"));
        assert!(level.nodes.len() >= 2);

        let ai = level.nodes.iter().find(|n| n.label == "AI");
        assert!(ai.is_some());
    }

    #[test]
    fn test_leaf_tag_shows_atoms() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();

        insert_tag(&conn, "cat1", "Topics", None);
        insert_tag(&conn, "t1", "AI", Some("cat1"));
        insert_atom(&conn, "a1", "First AI note");
        insert_atom(&conn, "a2", "Second AI note");
        tag_atom(&conn, "a1", "t1");
        tag_atom(&conn, "a2", "t1");

        let level = get_canvas_level(&conn, Some("t1"), None).unwrap();
        assert_eq!(level.nodes.len(), 2);
        assert!(level
            .nodes
            .iter()
            .all(|n| n.node_type == CanvasNodeType::Atom));
    }

    #[test]
    fn test_breadcrumb() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();

        insert_tag(&conn, "cat1", "Topics", None);
        insert_tag(&conn, "t1", "AI", Some("cat1"));
        insert_tag(&conn, "t2", "ML", Some("t1"));
        insert_atom(&conn, "a1", "ML content");
        tag_atom(&conn, "a1", "t2");

        let level = get_canvas_level(&conn, Some("t2"), None).unwrap();
        assert_eq!(level.breadcrumb.len(), 3); // Topics > AI > ML
        assert_eq!(level.breadcrumb[0].label, "Topics");
        assert_eq!(level.breadcrumb[1].label, "AI");
        assert_eq!(level.breadcrumb[2].label, "ML");
    }

    #[test]
    fn test_empty_level() {
        let (db, _temp) = create_test_db();
        let conn = db.conn.lock().unwrap();

        conn.execute("DELETE FROM tags", []).unwrap();
        conn.execute("DELETE FROM atoms", []).unwrap();

        let level = get_canvas_level(&conn, None, None).unwrap();
        assert!(level.nodes.is_empty());
    }
}
