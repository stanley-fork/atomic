//! Atom and Tag CRUD routes

use crate::db_extractor::Db;
use crate::error::{blocking_ok, ApiErrorResponse};
use crate::event_bridge::embedding_event_callback;
use crate::state::{AppState, ServerEvent};
use actix_web::{web, HttpResponse};
use atomic_core::{
    AtomWithTags, BulkCreateResult, PaginatedAtoms, PaginatedTagChildren, SourceInfo, Tag,
    TagWithCount,
};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

// ==================== Atoms ====================

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetAtomsQuery {
    /// Filter by tag ID
    pub tag_id: Option<String>,
    /// Max results to return (default: 50)
    pub limit: Option<i32>,
    /// Offset for pagination
    pub offset: Option<i32>,
    /// Cursor for keyset pagination (updated_at value)
    pub cursor: Option<String>,
    /// Cursor tiebreaker (atom id)
    pub cursor_id: Option<String>,
    /// Source filter: "all", "manual", or "external"
    pub source: Option<String>,
    /// Filter by specific source domain (e.g. "nytimes.com")
    pub source_value: Option<String>,
    /// Sort field: "updated", "created", "published", or "title"
    pub sort_by: Option<String>,
    /// Sort direction: "desc" or "asc"
    pub sort_order: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/atoms",
    params(GetAtomsQuery),
    responses(
        (status = 200, description = "Paginated list of atoms", body = PaginatedAtoms),
        (status = 500, description = "Internal error", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn get_atoms(
    db: Db,
    query: web::Query<GetAtomsQuery>,
) -> HttpResponse {
    let source_filter = match query.source.as_deref() {
        Some("manual") => atomic_core::SourceFilter::Manual,
        Some("external") => atomic_core::SourceFilter::External,
        _ => atomic_core::SourceFilter::All,
    };
    let sort_by = match query.sort_by.as_deref() {
        Some("created") => atomic_core::SortField::Created,
        Some("published") => atomic_core::SortField::Published,
        Some("title") => atomic_core::SortField::Title,
        _ => atomic_core::SortField::Updated,
    };
    let sort_order = match query.sort_order.as_deref() {
        Some("asc") => atomic_core::SortOrder::Asc,
        _ => atomic_core::SortOrder::Desc,
    };
    let params = atomic_core::ListAtomsParams {
        tag_id: query.tag_id.clone(),
        limit: query.limit.unwrap_or(50),
        offset: query.offset.unwrap_or(0),
        cursor: query.cursor.clone(),
        cursor_id: query.cursor_id.clone(),
        source_filter,
        source_value: query.source_value.clone(),
        sort_by,
        sort_order,
    };
    let core = db.0;
    blocking_ok(move || core.list_atoms(&params)).await
}

#[utoipa::path(
    get,
    path = "/api/atoms/sources",
    responses(
        (status = 200, description = "List of sources with counts", body = Vec<SourceInfo>),
    ),
    tag = "atoms",
)]
pub async fn get_source_list(db: Db) -> HttpResponse {
    let core = db.0;
    blocking_ok(move || core.get_source_list()).await
}

#[utoipa::path(
    get,
    path = "/api/atoms/{id}",
    params(
        ("id" = String, Path, description = "Atom ID"),
    ),
    responses(
        (status = 200, description = "Atom with tags", body = AtomWithTags),
        (status = 404, description = "Atom not found", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn get_atom(db: Db, path: web::Path<String>) -> HttpResponse {
    let id = path.into_inner();
    let core = db.0;
    match web::block(move || core.get_atom(&id)).await {
        Ok(Ok(Some(atom))) => HttpResponse::Ok().json(atom),
        Ok(Ok(None)) => HttpResponse::NotFound().json(serde_json::json!({"error": "Atom not found"})),
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetAtomBySourceUrlQuery {
    /// The source URL to look up
    pub url: String,
}

#[utoipa::path(
    get,
    path = "/api/atoms/by-source-url",
    params(GetAtomBySourceUrlQuery),
    responses(
        (status = 200, description = "Atom found", body = AtomWithTags),
        (status = 404, description = "No atom with this source URL", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn get_atom_by_source_url(
    db: Db,
    query: web::Query<GetAtomBySourceUrlQuery>,
) -> HttpResponse {
    let url = query.into_inner().url;
    let core = db.0;
    match web::block(move || core.get_atom_by_source_url(&url)).await {
        Ok(Ok(Some(atom))) => HttpResponse::Ok().json(atom),
        Ok(Ok(None)) => HttpResponse::NotFound().json(serde_json::json!({"error": "No atom found with this source URL"})),
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateAtomRequest {
    /// Markdown content of the atom
    pub content: String,
    /// Optional source URL
    pub source_url: Option<String>,
    /// Optional publication date (ISO 8601)
    pub published_at: Option<String>,
    /// Tag IDs to assign
    #[serde(default)]
    pub tag_ids: Vec<String>,
}

#[utoipa::path(
    post,
    path = "/api/atoms",
    request_body = CreateAtomRequest,
    responses(
        (status = 201, description = "Created atom", body = AtomWithTags),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn create_atom(
    state: web::Data<AppState>,
    db: Db,
    body: web::Json<CreateAtomRequest>,
) -> HttpResponse {
    let req = body.into_inner();
    let on_event = embedding_event_callback(state.event_tx.clone());
    let core = db.0;
    let event_tx = state.event_tx.clone();
    match web::block(move || {
        core.create_atom(
            atomic_core::CreateAtomRequest {
                content: req.content,
                source_url: req.source_url,
                published_at: req.published_at,
                tag_ids: req.tag_ids,
            },
            on_event,
        )
    }).await {
        Ok(Ok(atom)) => {
            let _ = event_tx.send(ServerEvent::AtomCreated { atom: atom.clone() });
            HttpResponse::Created().json(atom)
        }
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[utoipa::path(
    post,
    path = "/api/atoms/bulk",
    request_body = Vec<CreateAtomRequest>,
    responses(
        (status = 201, description = "Bulk create result", body = BulkCreateResult),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn bulk_create_atoms(
    state: web::Data<AppState>,
    db: Db,
    body: web::Json<Vec<CreateAtomRequest>>,
) -> HttpResponse {
    let requests: Vec<atomic_core::CreateAtomRequest> = body
        .into_inner()
        .into_iter()
        .map(|r| atomic_core::CreateAtomRequest {
            content: r.content,
            source_url: r.source_url,
            published_at: r.published_at,
            tag_ids: r.tag_ids,
        })
        .collect();
    let on_event = embedding_event_callback(state.event_tx.clone());
    let core = db.0;
    let event_tx = state.event_tx.clone();
    match web::block(move || core.create_atoms_bulk(requests, on_event)).await {
        Ok(Ok(result)) => {
            for atom in &result.atoms {
                let _ = event_tx.send(ServerEvent::AtomCreated { atom: atom.clone() });
            }
            HttpResponse::Created().json(result)
        }
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct UpdateAtomRequest {
    /// Updated markdown content
    pub content: String,
    /// Updated source URL
    pub source_url: Option<String>,
    /// Updated publication date
    pub published_at: Option<String>,
    /// Updated tag IDs (if provided, replaces all tags)
    pub tag_ids: Option<Vec<String>>,
}

#[utoipa::path(
    put,
    path = "/api/atoms/{id}",
    params(
        ("id" = String, Path, description = "Atom ID"),
    ),
    request_body = UpdateAtomRequest,
    responses(
        (status = 200, description = "Updated atom", body = AtomWithTags),
        (status = 404, description = "Atom not found", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn update_atom(
    state: web::Data<AppState>,
    db: Db,
    path: web::Path<String>,
    body: web::Json<UpdateAtomRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    let req = body.into_inner();
    let on_event = embedding_event_callback(state.event_tx.clone());
    let core = db.0;
    blocking_ok(move || {
        core.update_atom(
            &id,
            atomic_core::UpdateAtomRequest {
                content: req.content,
                source_url: req.source_url,
                published_at: req.published_at,
                tag_ids: req.tag_ids,
            },
            on_event,
        )
    }).await
}

#[utoipa::path(
    delete,
    path = "/api/atoms/{id}",
    params(
        ("id" = String, Path, description = "Atom ID"),
    ),
    responses(
        (status = 200, description = "Atom deleted"),
        (status = 404, description = "Atom not found", body = ApiErrorResponse),
    ),
    tag = "atoms",
)]
pub async fn delete_atom(db: Db, path: web::Path<String>) -> HttpResponse {
    let id = path.into_inner();
    let core = db.0;
    blocking_ok(move || core.delete_atom(&id)).await
}

// ==================== Tags ====================

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetTagsQuery {
    /// Minimum atom count to include (default: 2)
    pub min_count: Option<i32>,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GetTagChildrenQuery {
    /// Minimum atom count to include (default: 0)
    pub min_count: Option<i32>,
    /// Max results (default: 100)
    pub limit: Option<i32>,
    /// Offset for pagination
    pub offset: Option<i32>,
}

#[utoipa::path(
    get,
    path = "/api/tags",
    params(GetTagsQuery),
    responses(
        (status = 200, description = "Hierarchical tag tree", body = Vec<TagWithCount>),
    ),
    tag = "tags",
)]
pub async fn get_tags(
    db: Db,
    query: web::Query<GetTagsQuery>,
) -> HttpResponse {
    let min_count = query.min_count.unwrap_or(2);
    let core = db.0;
    blocking_ok(move || core.get_all_tags_filtered(min_count)).await
}

#[utoipa::path(
    get,
    path = "/api/tags/{id}/children",
    params(
        ("id" = String, Path, description = "Parent tag ID"),
        GetTagChildrenQuery,
    ),
    responses(
        (status = 200, description = "Paginated tag children", body = PaginatedTagChildren),
    ),
    tag = "tags",
)]
pub async fn get_tag_children(
    db: Db,
    path: web::Path<String>,
    query: web::Query<GetTagChildrenQuery>,
) -> HttpResponse {
    let parent_id = path.into_inner();
    let min_count = query.min_count.unwrap_or(0);
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let core = db.0;
    blocking_ok(move || core.get_tag_children(&parent_id, min_count, limit, offset)).await
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct CreateTagRequest {
    /// Tag name
    pub name: String,
    /// Parent tag ID for hierarchy
    pub parent_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/tags",
    request_body = CreateTagRequest,
    responses(
        (status = 201, description = "Created tag", body = Tag),
        (status = 400, description = "Validation error", body = ApiErrorResponse),
    ),
    tag = "tags",
)]
pub async fn create_tag(
    db: Db,
    body: web::Json<CreateTagRequest>,
) -> HttpResponse {
    let req = body.into_inner();
    let core = db.0;
    match web::block(move || core.create_tag(&req.name, req.parent_id.as_deref())).await {
        Ok(Ok(tag)) => HttpResponse::Created().json(tag),
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct UpdateTagRequest {
    /// Updated tag name
    pub name: String,
    /// Updated parent tag ID
    pub parent_id: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/tags/{id}",
    params(
        ("id" = String, Path, description = "Tag ID"),
    ),
    request_body = UpdateTagRequest,
    responses(
        (status = 200, description = "Updated tag", body = Tag),
        (status = 404, description = "Tag not found", body = ApiErrorResponse),
    ),
    tag = "tags",
)]
pub async fn update_tag(
    db: Db,
    path: web::Path<String>,
    body: web::Json<UpdateTagRequest>,
) -> HttpResponse {
    let id = path.into_inner();
    let req = body.into_inner();
    let core = db.0;
    blocking_ok(move || core.update_tag(&id, &req.name, req.parent_id.as_deref())).await
}

#[utoipa::path(
    delete,
    path = "/api/tags/{id}",
    params(
        ("id" = String, Path, description = "Tag ID"),
        ("recursive" = Option<bool>, Query, description = "Delete child tags recursively"),
    ),
    responses(
        (status = 200, description = "Tag deleted"),
        (status = 404, description = "Tag not found", body = ApiErrorResponse),
    ),
    tag = "tags",
)]
pub async fn delete_tag(
    db: Db,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> HttpResponse {
    let id = path.into_inner();
    let recursive = query.get("recursive").map(|v| v == "true").unwrap_or(false);
    let core = db.0;
    blocking_ok(move || core.delete_tag(&id, recursive)).await
}
