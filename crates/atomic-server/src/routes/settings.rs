//! Settings routes

use crate::db_extractor::Db;
use crate::error::{blocking_ok, ApiErrorResponse};
use crate::state::AppState;
use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[utoipa::path(get, path = "/api/settings", responses((status = 200, description = "All settings as key-value map")), tag = "settings")]
pub async fn get_settings(db: Db) -> HttpResponse {
    let core = db.0;
    blocking_ok(move || core.get_settings()).await
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct SetSettingBody {
    /// Setting value
    pub value: String,
}

#[utoipa::path(put, path = "/api/settings/{key}", params(("key" = String, Path, description = "Setting key")), request_body = SetSettingBody, responses((status = 200, description = "Setting updated"), (status = 400, description = "Invalid setting", body = ApiErrorResponse)), tag = "settings")]
pub async fn set_setting(
    state: web::Data<AppState>,
    db: Db,
    path: web::Path<String>,
    body: web::Json<SetSettingBody>,
) -> HttpResponse {
    let key = path.into_inner();
    let value = body.into_inner().value;

    // Handle dimension-affecting settings via set_setting_with_reembed (avoids deadlock)
    let dimension_keys = ["provider", "embedding_model", "ollama_embedding_model", "openai_compat_embedding_model", "openai_compat_embedding_dimension"];
    if dimension_keys.contains(&key.as_str()) {
        let core = db.0;
        let manager = state.manager.clone();
        let active_id = state.manager.active_id().unwrap_or_default();
        let on_event = crate::event_bridge::embedding_event_callback(state.event_tx.clone());
        match web::block(move || {
            let result = core.set_setting_with_reembed(&key, &value, on_event);
            // If dimension changed, also recreate vector indexes on all other databases.
            // Best-effort: failures here must not override the already-successful result.
            if let Ok((true, _)) = &result {
                match core.get_settings() {
                    Ok(current_settings) => {
                        let config = atomic_core::providers::ProviderConfig::from_settings(&current_settings);
                        let new_dim = config.embedding_dimension();
                        if let Err(e) = manager.recreate_other_vector_indexes(new_dim, &active_id) {
                            tracing::error!("Failed to recreate vector indexes on other databases: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to get settings for dimension calc: {}", e);
                    }
                }
            }
            result
        }).await {
            Ok(Ok((changed, count))) => HttpResponse::Ok().json(serde_json::json!({
                "dimension_changed": changed,
                "pending_reembed_count": count,
            })),
            Ok(Err(e)) => crate::error::error_response(e),
            Err(e) => HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        let core = db.0;
        blocking_ok(move || core.set_setting(&key, &value)).await
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct TestOpenRouterBody {
    /// OpenRouter API key to test
    pub api_key: String,
}

#[utoipa::path(post, path = "/api/settings/test-openrouter", request_body = TestOpenRouterBody, responses((status = 200, description = "Connection successful"), (status = 400, description = "API error", body = ApiErrorResponse)), tag = "settings")]
pub async fn test_openrouter_connection(
    body: web::Json<TestOpenRouterBody>,
) -> HttpResponse {
    let client = reqwest::Client::new();
    let response = client
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", body.api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": "anthropic/claude-haiku-4.5",
            "messages": [{"role": "user", "content": "Hi"}],
            "max_tokens": 5
        }))
        .send()
        .await;

    match response {
        Ok(resp) if resp.status().is_success() => {
            HttpResponse::Ok().json(serde_json::json!({"success": true}))
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            HttpResponse::BadRequest().json(serde_json::json!({
                "error": format!("API error ({}): {}", status, body)
            }))
        }
        Err(e) => HttpResponse::BadGateway().json(serde_json::json!({
            "error": format!("Network error: {}", e)
        })),
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct TestOpenAICompatBody {
    /// Base URL of the OpenAI-compatible API
    pub base_url: String,
    /// Optional API key for authentication
    pub api_key: Option<String>,
}

#[utoipa::path(post, path = "/api/settings/test-openai-compat", request_body = TestOpenAICompatBody, responses((status = 200, description = "Connection successful"), (status = 400, description = "API error", body = ApiErrorResponse)), tag = "settings")]
pub async fn test_openai_compat_connection(
    body: web::Json<TestOpenAICompatBody>,
) -> HttpResponse {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Normalize URL the same way OpenAICompatProvider does
    let trimmed = body.base_url.trim_end_matches('/');
    let base_url = if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{}/v1", trimmed)
    };

    let mut req = client.get(format!("{}/models", base_url));

    if let Some(ref api_key) = body.api_key {
        if !api_key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        }
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            HttpResponse::Ok().json(serde_json::json!({"success": true}))
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            HttpResponse::BadRequest().json(serde_json::json!({
                "error": format!("API error ({}): {}", status, body)
            }))
        }
        Err(e) => HttpResponse::BadGateway().json(serde_json::json!({
            "error": format!("Connection failed: {}", e)
        })),
    }
}

#[utoipa::path(get, path = "/api/settings/embedding-models", responses((status = 200, description = "Curated OpenRouter embedding models with dimensions")), tag = "settings")]
pub async fn get_openrouter_embedding_models() -> HttpResponse {
    let models = atomic_core::providers::openrouter::models::get_embedding_models();
    HttpResponse::Ok().json(models)
}

#[utoipa::path(get, path = "/api/settings/models", responses((status = 200, description = "Available LLM models")), tag = "settings")]
pub async fn get_available_llm_models(db: Db) -> HttpResponse {
    use atomic_core::providers::models::fetch_and_return_capabilities;

    let core = &db.0;
    let (cached, is_stale) = match core.get_cached_capabilities() {
        Ok(Some(cache)) => {
            let stale = cache.is_stale();
            (Some(cache), stale)
        }
        Ok(None) => (None, true),
        Err(_) => (None, true),
    };

    if let Some(ref cache) = cached {
        if !is_stale {
            return HttpResponse::Ok().json(cache.get_models_with_structured_outputs());
        }
    }

    let client = reqwest::Client::new();
    match fetch_and_return_capabilities(&client).await {
        Ok(fresh_cache) => {
            let _ = core.save_capabilities_cache(&fresh_cache);
            HttpResponse::Ok().json(fresh_cache.get_models_with_structured_outputs())
        }
        Err(e) => {
            if let Some(cache) = cached {
                HttpResponse::Ok().json(cache.get_models_with_structured_outputs())
            } else {
                HttpResponse::BadGateway()
                    .json(serde_json::json!({"error": format!("Failed to fetch models: {}", e)}))
            }
        }
    }
}
