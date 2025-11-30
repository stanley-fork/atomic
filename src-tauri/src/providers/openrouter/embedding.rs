use crate::providers::error::ProviderError;
use crate::providers::openrouter::OpenRouterProvider;
use crate::providers::traits::EmbeddingConfig;
use serde::{Deserialize, Serialize};

/// OpenRouter Embeddings API request
#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

/// OpenRouter Embeddings API response
#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// Generate embeddings for multiple texts via OpenRouter API
pub async fn embed_batch(
    provider: &OpenRouterProvider,
    texts: &[String],
    config: &EmbeddingConfig,
) -> Result<Vec<Vec<f32>>, ProviderError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let request = EmbeddingRequest {
        model: config.model.clone(),
        input: texts.to_vec(),
    };

    let response = provider
        .client()
        .post(format!("{}/embeddings", provider.base_url()))
        .header("Authorization", format!("Bearer {}", provider.api_key()))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "https://atomic.app")
        .header("X-Title", "Atomic")
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        if status == 429 {
            // Try to parse retry-after header
            return Err(ProviderError::RateLimited {
                retry_after_secs: None,
            });
        }

        return Err(ProviderError::Api {
            status,
            message: body,
        });
    }

    let embedding_response: EmbeddingResponse = response.json().await?;

    Ok(embedding_response
        .data
        .into_iter()
        .map(|d| d.embedding)
        .collect())
}
