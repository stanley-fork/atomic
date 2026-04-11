//! Semantic graph routes

use crate::db_extractor::Db;
use crate::error::blocking_ok;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use utoipa::IntoParams;

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct EdgesQuery {
    /// Minimum similarity score (default: 0.5)
    pub min_similarity: Option<f32>,
}

#[utoipa::path(get, path = "/api/graph/edges", params(EdgesQuery), responses((status = 200, description = "Semantic edges", body = Vec<atomic_core::SemanticEdge>)), tag = "graph")]
pub async fn get_semantic_edges(
    db: Db,
    query: web::Query<EdgesQuery>,
) -> HttpResponse {
    let min_similarity = query.min_similarity.unwrap_or(0.5);
    let core = db.0;
    blocking_ok(move || core.get_semantic_edges(min_similarity)).await
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NeighborhoodQuery {
    /// Graph traversal depth (default: 1)
    pub depth: Option<i32>,
    /// Minimum similarity score (default: 0.5)
    pub min_similarity: Option<f32>,
}

#[utoipa::path(get, path = "/api/graph/neighborhood/{atom_id}", params(("atom_id" = String, Path, description = "Center atom ID"), NeighborhoodQuery), responses((status = 200, description = "Neighborhood graph", body = atomic_core::NeighborhoodGraph)), tag = "graph")]
pub async fn get_atom_neighborhood(
    db: Db,
    path: web::Path<String>,
    query: web::Query<NeighborhoodQuery>,
) -> HttpResponse {
    let atom_id = path.into_inner();
    let depth = query.depth.unwrap_or(1);
    let min_similarity = query.min_similarity.unwrap_or(0.5);
    let core = db.0;
    blocking_ok(move || core.get_atom_neighborhood(&atom_id, depth, min_similarity)).await
}

/// Queue a full rebuild of the semantic-edge graph.
///
/// Returns the number of atoms **queued** for edge recomputation, not the
/// number of edges written. The actual edge computation runs asynchronously
/// on the background pipeline; this endpoint returns as soon as the work
/// is spawned. Clients that need completion should subscribe to pipeline
/// events over WebSocket.
#[utoipa::path(post, path = "/api/graph/rebuild-edges", responses((status = 200, description = "Edge rebuild queued; returns the number of atoms queued for recomputation")), tag = "graph")]
pub async fn rebuild_semantic_edges(db: Db) -> HttpResponse {
    let core = db.0;
    blocking_ok(move || core.rebuild_semantic_edges()).await
}
