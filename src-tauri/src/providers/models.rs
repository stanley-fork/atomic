use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// Model information from OpenRouter API
#[derive(Debug, Clone, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub supported_parameters: Vec<String>,
}

/// Response from OpenRouter models API
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

/// Simplified model info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableModel {
    pub id: String,
    pub name: String,
}

/// Cached model capabilities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCapabilitiesCache {
    /// Map of model ID to supported parameters
    pub models: HashMap<String, Vec<String>>,
    /// Map of model ID to display name
    pub model_names: HashMap<String, String>,
    /// Timestamp when cache was last updated (Unix seconds)
    pub updated_at: i64,
}

impl ModelCapabilitiesCache {
    /// Check if cache is stale (older than 24 hours)
    pub fn is_stale(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - self.updated_at > 86400 // 24 hours
    }

    /// Get supported parameters for a model
    pub fn get_supported_params(&self, model_id: &str) -> Option<&Vec<String>> {
        self.models.get(model_id)
    }

    /// Check if a model supports a specific parameter
    pub fn supports_param(&self, model_id: &str, param: &str) -> bool {
        self.models
            .get(model_id)
            .map(|params| params.iter().any(|p| p == param))
            .unwrap_or(true) // Default to true if model not in cache
    }

    /// Get all models that support structured outputs (JSON schema validation)
    /// We filter for "structured_outputs" specifically since that capability is required
    /// for response_format with type "json_schema" and strict validation
    pub fn get_models_with_structured_outputs(&self) -> Vec<AvailableModel> {
        let mut models: Vec<AvailableModel> = self
            .models
            .iter()
            .filter(|(_, params)| {
                params.iter().any(|p| p == "structured_outputs")
            })
            .map(|(id, _)| AvailableModel {
                id: id.clone(),
                name: self.model_names.get(id).cloned().unwrap_or_else(|| id.clone()),
            })
            .collect();

        // Sort by name for consistent ordering
        models.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        models
    }
}

/// Fetch model capabilities from OpenRouter API
pub async fn fetch_model_capabilities(client: &Client) -> Result<ModelCapabilitiesCache, String> {
    let response = client
        .get(OPENROUTER_MODELS_URL)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch models: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Models API returned status: {}",
            response.status()
        ));
    }

    let models_response: ModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse models response: {}", e))?;

    let mut models = HashMap::new();
    let mut model_names = HashMap::new();
    for model in models_response.data {
        model_names.insert(model.id.clone(), model.name.clone());
        models.insert(model.id, model.supported_parameters);
    }

    Ok(ModelCapabilitiesCache {
        models,
        model_names,
        updated_at: chrono::Utc::now().timestamp(),
    })
}

/// Load cached capabilities from database
pub fn load_cached_capabilities(
    conn: &rusqlite::Connection,
) -> Result<Option<ModelCapabilitiesCache>, String> {
    let json: Result<String, _> = conn.query_row(
        "SELECT value FROM settings WHERE key = 'model_capabilities_cache'",
        [],
        |row| row.get(0),
    );

    match json {
        Ok(json_str) => {
            let cache: ModelCapabilitiesCache = serde_json::from_str(&json_str)
                .map_err(|e| format!("Failed to parse cached capabilities: {}", e))?;
            Ok(Some(cache))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(format!("Failed to load cached capabilities: {}", e)),
    }
}

/// Save capabilities cache to database
pub fn save_capabilities_cache(
    conn: &rusqlite::Connection,
    cache: &ModelCapabilitiesCache,
) -> Result<(), String> {
    let json = serde_json::to_string(cache)
        .map_err(|e| format!("Failed to serialize capabilities cache: {}", e))?;

    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('model_capabilities_cache', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        [&json],
    )
    .map_err(|e| format!("Failed to save capabilities cache: {}", e))?;

    Ok(())
}

/// Fetch fresh model capabilities from API (async, no DB access)
pub async fn fetch_and_return_capabilities(
    client: &Client,
) -> Result<ModelCapabilitiesCache, String> {
    fetch_model_capabilities(client).await
}

/// Get model capabilities, using cache if available and fresh
/// This is a sync function that checks cache. Call fetch_and_return_capabilities
/// separately if cache is stale, then save_capabilities_cache to persist.
pub fn get_cached_capabilities_sync(
    conn: &rusqlite::Connection,
) -> Result<Option<ModelCapabilitiesCache>, String> {
    if let Some(cache) = load_cached_capabilities(conn)? {
        if !cache.is_stale() {
            return Ok(Some(cache));
        }
        // Return the stale cache so caller can use it as fallback
        return Ok(Some(cache));
    }
    Ok(None)
}
