//! API error handling — maps AtomicCoreError to HTTP responses

use actix_web::HttpResponse;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorResponse {
    pub error: String,
}

/// Convert AtomicCoreError to an appropriate HTTP response
pub fn error_response(e: atomic_core::AtomicCoreError) -> HttpResponse {
    match &e {
        atomic_core::AtomicCoreError::NotFound(_) => {
            HttpResponse::NotFound().json(ApiErrorResponse {
                error: e.to_string(),
            })
        }
        atomic_core::AtomicCoreError::Validation(_) => {
            HttpResponse::BadRequest().json(ApiErrorResponse {
                error: e.to_string(),
            })
        }
        atomic_core::AtomicCoreError::Configuration(_) => {
            HttpResponse::BadRequest().json(ApiErrorResponse {
                error: e.to_string(),
            })
        }
        atomic_core::AtomicCoreError::Conflict(_) => {
            HttpResponse::Conflict().json(ApiErrorResponse {
                error: e.to_string(),
            })
        }
        _ => HttpResponse::InternalServerError().json(ApiErrorResponse {
            error: e.to_string(),
        }),
    }
}

/// Shorthand to convert Result<T, AtomicCoreError> to HttpResponse
pub fn ok_or_error<T: serde::Serialize>(
    result: Result<T, atomic_core::AtomicCoreError>,
) -> HttpResponse {
    match result {
        Ok(data) => HttpResponse::Ok().json(data),
        Err(e) => error_response(e),
    }
}

/// Get the HTTP status code for an AtomicCoreError
pub fn status_code_for(e: &atomic_core::AtomicCoreError) -> actix_web::http::StatusCode {
    use actix_web::http::StatusCode;
    match e {
        atomic_core::AtomicCoreError::NotFound(_) => StatusCode::NOT_FOUND,
        atomic_core::AtomicCoreError::Validation(_) => StatusCode::BAD_REQUEST,
        atomic_core::AtomicCoreError::Configuration(_) => StatusCode::BAD_REQUEST,
        atomic_core::AtomicCoreError::Conflict(_) => StatusCode::CONFLICT,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::http::StatusCode;
    use atomic_core::AtomicCoreError;

    #[test]
    fn test_not_found_maps_to_404() {
        assert_eq!(
            status_code_for(&AtomicCoreError::NotFound("atom xyz".into())),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_validation_maps_to_400() {
        assert_eq!(
            status_code_for(&AtomicCoreError::Validation("bad input".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn test_configuration_maps_to_400() {
        assert_eq!(
            status_code_for(&AtomicCoreError::Configuration("missing key".into())),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn test_database_error_maps_to_500() {
        assert_eq!(
            status_code_for(&AtomicCoreError::DatabaseOperation("db broke".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_lock_error_maps_to_500() {
        assert_eq!(
            status_code_for(&AtomicCoreError::Lock("poisoned".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_ok_or_error_success() {
        let resp = ok_or_error(Ok::<_, AtomicCoreError>(serde_json::json!({"key": "val"})));
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_ok_or_error_failure() {
        let resp = ok_or_error(Err::<serde_json::Value, _>(AtomicCoreError::NotFound(
            "nope".into(),
        )));
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
