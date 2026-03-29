//! API error handling — maps CloudError to HTTP responses

use actix_web::HttpResponse;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Stripe error: {0}")]
    Stripe(String),

    #[error("Fly.io error: {0}")]
    Fly(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl CloudError {
    pub fn to_response(&self) -> HttpResponse {
        let status = match self {
            CloudError::NotFound(_) => actix_web::http::StatusCode::NOT_FOUND,
            CloudError::BadRequest(_) => actix_web::http::StatusCode::BAD_REQUEST,
            CloudError::Unauthorized(_) => actix_web::http::StatusCode::UNAUTHORIZED,
            CloudError::Conflict(_) => actix_web::http::StatusCode::CONFLICT,
            CloudError::Stripe(_) | CloudError::Fly(_) | CloudError::Internal(_) => {
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR
            }
            CloudError::Database(_) => actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        };

        HttpResponse::build(status).json(ApiErrorResponse {
            error: self.to_string(),
        })
    }
}

impl From<CloudError> for HttpResponse {
    fn from(e: CloudError) -> Self {
        e.to_response()
    }
}

/// Shorthand to convert Result<T, CloudError> to HttpResponse
pub fn ok_or_error<T: Serialize>(result: Result<T, CloudError>) -> HttpResponse {
    match result {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(e) => e.to_response(),
    }
}
