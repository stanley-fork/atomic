//! OpenAI-compatible embedding implementation

use crate::providers::error::ProviderError;
use crate::providers::openai_compat::OpenAICompatProvider;
use crate::providers::traits::EmbeddingConfig;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// Generate embeddings via an OpenAI-compatible API
pub async fn embed_batch(
    provider: &OpenAICompatProvider,
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

    let mut req = provider
        .client()
        .post(format!("{}/embeddings", provider.base_url()))
        .header("Content-Type", "application/json");

    if let Some(api_key) = provider.api_key() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }

    let response = req.json(&request).send().await?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();

        if status == 429 {
            return Err(ProviderError::RateLimited {
                retry_after_secs: None,
            });
        }

        return Err(ProviderError::Api {
            status,
            message: body,
        });
    }

    let body = response.text().await?;

    let embedding_response: EmbeddingResponse = serde_json::from_str(&body)
        .map_err(|e| {
            eprintln!("OpenAI-compat embedding parse error: {e}");
            eprintln!("Response body (first 500 chars): {}", &body[..body.len().min(500)]);
            ProviderError::ParseError(format!("Failed to parse embedding response: {e}"))
        })?;

    Ok(embedding_response
        .data
        .into_iter()
        .map(|d| d.embedding)
        .collect())
}
