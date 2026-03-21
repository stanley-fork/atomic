//! Instance setup endpoint — allows claiming an unconfigured instance

use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

/// GET /api/setup/status — Check if the instance needs initial setup
pub async fn setup_status(state: web::Data<AppState>) -> HttpResponse {
    let registry = state.manager.registry().clone();
    match web::block(move || registry.list_api_tokens()).await {
        Ok(Ok(tokens)) => {
            let active = tokens.iter().filter(|t| !t.is_revoked).count();
            HttpResponse::Ok().json(serde_json::json!({
                "needs_setup": active == 0,
            }))
        }
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}

#[derive(Deserialize)]
pub struct ClaimBody {
    pub name: Option<String>,
}

/// POST /api/setup/claim — Create the first API token (only works when no tokens exist)
pub async fn claim_instance(
    state: web::Data<AppState>,
    body: web::Json<ClaimBody>,
) -> HttpResponse {
    let name = body.into_inner().name.unwrap_or_else(|| "default".to_string());
    let registry = state.manager.registry().clone();

    match web::block(move || {
        // Check that no active tokens exist
        let tokens = registry.list_api_tokens()?;
        let active = tokens.iter().filter(|t| !t.is_revoked).count();
        if active > 0 {
            return Ok(None);
        }
        let (info, raw) = registry.create_api_token(&name)?;
        Ok(Some((info, raw)))
    })
    .await
    {
        Ok(Ok(Some((info, raw_token)))) => HttpResponse::Created().json(serde_json::json!({
            "id": info.id,
            "name": info.name,
            "token": raw_token,
            "prefix": info.token_prefix,
            "created_at": info.created_at,
        })),
        Ok(Ok(None)) => HttpResponse::Conflict().json(serde_json::json!({
            "error": "Instance already claimed"
        })),
        Ok(Err(e)) => crate::error::error_response(e),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({"error": e.to_string()})),
    }
}
