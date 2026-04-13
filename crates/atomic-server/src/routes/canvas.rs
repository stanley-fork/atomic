//! Canvas position routes

use crate::db_extractor::Db;
use crate::error::blocking_ok;
use actix_web::{web, HttpResponse};
use atomic_core::{AtomPosition, CanvasAtomPosition, CanvasClusterLabel, CanvasEdgeData, GlobalCanvasData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::{IntoParams, ToSchema};

#[utoipa::path(get, path = "/api/canvas/positions", responses((status = 200, description = "All atom positions", body = Vec<AtomPosition>)), tag = "canvas")]
pub async fn get_positions(db: Db) -> HttpResponse {
    let core = db.0;
    blocking_ok(move || core.get_atom_positions()).await
}

#[utoipa::path(put, path = "/api/canvas/positions", request_body = Vec<AtomPosition>, responses((status = 200, description = "Positions saved")), tag = "canvas")]
pub async fn save_positions(
    db: Db,
    body: web::Json<Vec<AtomPosition>>,
) -> HttpResponse {
    let positions = body.into_inner();
    let core = db.0;
    match web::block(move || core.save_atom_positions(&positions)).await {
        Ok(Ok(())) => HttpResponse::Ok().json(serde_json::json!({"status": "ok"})),
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[utoipa::path(get, path = "/api/canvas/atoms-with-embeddings", responses((status = 200, description = "Atoms with embedding vectors", body = Vec<atomic_core::AtomWithEmbedding>)), tag = "canvas")]
pub async fn get_atoms_with_embeddings(db: Db) -> HttpResponse {
    let core = db.0;
    blocking_ok(move || core.get_atoms_with_embeddings()).await
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CanvasLevelQuery {
    /// Parent node ID (null for root level)
    pub parent_id: Option<String>,
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CanvasLevelBody {
    /// Hint for which children to include
    pub children_hint: Option<Vec<String>>,
}

#[utoipa::path(post, path = "/api/canvas/level", params(CanvasLevelQuery), request_body(content = Option<CanvasLevelBody>), responses((status = 200, description = "Canvas level data", body = atomic_core::CanvasLevel)), tag = "canvas")]
pub async fn get_canvas_level(
    db: Db,
    query: web::Query<CanvasLevelQuery>,
    body: Option<web::Json<CanvasLevelBody>>,
) -> HttpResponse {
    let parent_id = query.parent_id.clone();
    let children_hint = body.and_then(|b| b.into_inner().children_hint);
    let core = db.0;
    blocking_ok(move || core.get_canvas_level(parent_id.as_deref(), children_hint)).await
}

/// Compute PCA 2D projection and return all atoms with positions, edges, and cluster labels
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GlobalCanvasQuery {
    /// If set, only atoms whose `source_url` starts with this prefix are returned.
    /// Use `obsidian://VaultName/` to filter to a specific vault.
    pub source_prefix: Option<String>,
}

#[utoipa::path(get, path = "/api/canvas/global", params(GlobalCanvasQuery), responses((status = 200, description = "Global canvas data", body = atomic_core::GlobalCanvasData)), tag = "canvas")]
pub async fn get_global_canvas(db: Db, query: web::Query<GlobalCanvasQuery>) -> HttpResponse {
    let source_prefix = query.into_inner().source_prefix;
    let core = db.0;

    match actix_web::web::block(move || core.compute_and_get_canvas_data()).await {
        Ok(Ok(data)) => {
            if let Some(ref prefix) = source_prefix {
                HttpResponse::Ok().json(filter_canvas_by_source_prefix(&data, prefix))
            } else {
                HttpResponse::Ok().json(&*data)
            }
        }
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": format!("Thread pool error: {}", e)})),
    }
}

fn filter_canvas_by_source_prefix(data: &GlobalCanvasData, prefix: &str) -> GlobalCanvasData {
    let atoms: Vec<CanvasAtomPosition> = data
        .atoms
        .iter()
        .filter(|a| {
            a.source_url
                .as_deref()
                .map(|url| url.starts_with(prefix))
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let kept: HashSet<&str> = atoms.iter().map(|a| a.atom_id.as_str()).collect();

    let edges: Vec<CanvasEdgeData> = data
        .edges
        .iter()
        .filter(|e| kept.contains(e.source.as_str()) && kept.contains(e.target.as_str()))
        .cloned()
        .collect();

    let clusters: Vec<CanvasClusterLabel> = data
        .clusters
        .iter()
        .filter_map(|c| {
            let atom_ids: Vec<String> = c
                .atom_ids
                .iter()
                .filter(|id| kept.contains(id.as_str()))
                .cloned()
                .collect();
            if atom_ids.is_empty() {
                None
            } else {
                Some(CanvasClusterLabel {
                    atom_count: atom_ids.len() as i32,
                    atom_ids,
                    ..c.clone()
                })
            }
        })
        .collect();

    GlobalCanvasData { atoms, edges, clusters }
}
