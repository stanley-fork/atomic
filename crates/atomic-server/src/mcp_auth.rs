//! MCP-specific Bearer auth middleware
//!
//! Similar to `auth::BearerAuth` but returns `WWW-Authenticate` header with
//! `resource_metadata` pointing to the OAuth discovery endpoint, as required
//! by the MCP spec for OAuth-capable servers.

use crate::state::AppState;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::web;
use actix_web::Error;
use actix_web::HttpResponse;
use futures::future::{ok, LocalBoxFuture, Ready};
use std::task::{Context, Poll};

/// Custom error type that produces a 401 with the MCP-required `WWW-Authenticate` header.
#[derive(Debug)]
struct McpUnauthorized {
    www_authenticate: String,
    message: &'static str,
}

impl std::fmt::Display for McpUnauthorized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl actix_web::ResponseError for McpUnauthorized {
    fn status_code(&self) -> actix_web::http::StatusCode {
        actix_web::http::StatusCode::UNAUTHORIZED
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::Unauthorized()
            .insert_header(("WWW-Authenticate", self.www_authenticate.clone()))
            .json(serde_json::json!({"error": self.message}))
    }
}

/// Middleware for the `/mcp` scope that verifies Bearer tokens and returns
/// MCP-compliant `WWW-Authenticate` headers on 401.
pub struct McpAuth {
    pub state: web::Data<AppState>,
}

impl<S, B> Transform<S, ServiceRequest> for McpAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = McpAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(McpAuthMiddleware {
            service,
            state: self.state.clone(),
        })
    }
}

pub struct McpAuthMiddleware<S> {
    service: S,
    state: web::Data<AppState>,
}

impl<S, B> Service<ServiceRequest> for McpAuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let state = self.state.clone();

        let raw_token = req
            .headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(String::from);

        let www_authenticate = {
            let public_url = state
                .public_url
                .as_deref()
                .unwrap_or("")
                .trim_end_matches('/');
            format!(
                "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
                public_url
            )
        };

        let raw_token = match raw_token {
            Some(t) => t,
            None => {
                return Box::pin(async move {
                    Err(McpUnauthorized {
                        www_authenticate,
                        message: "unauthorized",
                    }
                    .into())
                });
            }
        };

        // Verify token
        let token_info = match state.core.verify_api_token(&raw_token) {
            Ok(Some(info)) => info,
            _ => {
                return Box::pin(async move {
                    Err(McpUnauthorized {
                        www_authenticate,
                        message: "invalid_token",
                    }
                    .into())
                });
            }
        };

        // Fire-and-forget last_used_at update
        let token_id = token_info.id.clone();
        let state_for_update = state.clone();
        tokio::task::spawn_blocking(move || {
            let _ = state_for_update.core.update_token_last_used(&token_id);
        });

        let fut = self.service.call(req);
        Box::pin(async move { fut.await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, ServerEvent};
    use actix_web::test as actix_test;
    use actix_web::App;
    use tokio::sync::broadcast;

    async fn protected_endpoint() -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({"ok": true}))
    }

    fn test_state(public_url: Option<&str>) -> (web::Data<AppState>, String) {
        let temp = tempfile::NamedTempFile::new().unwrap();
        let core = atomic_core::AtomicCore::open_or_create(temp.path()).unwrap();
        let (_, raw_token) = core.create_api_token("test-token").unwrap();
        let (event_tx, _) = broadcast::channel::<ServerEvent>(16);
        let state = web::Data::new(AppState {
            core,
            event_tx,
            public_url: public_url.map(String::from),
        });
        std::mem::forget(temp);
        (state, raw_token)
    }

    #[actix_web::test]
    async fn test_valid_token_passes() {
        let (state, raw_token) = test_state(Some("https://atomic.example.com"));
        let app = actix_test::init_service(
            App::new().service(
                web::scope("/mcp")
                    .wrap(McpAuth { state: state.clone() })
                    .route("/ping", web::get().to(protected_endpoint)),
            ),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mcp/ping")
            .insert_header(("Authorization", format!("Bearer {}", raw_token)))
            .to_request();
        let resp = actix_test::call_service(&app, req).await;
        assert_eq!(resp.status(), 200);
    }

    #[actix_web::test]
    async fn test_missing_token_returns_401_with_www_authenticate() {
        let (state, _) = test_state(Some("https://atomic.example.com"));
        let app = actix_test::init_service(
            App::new().service(
                web::scope("/mcp")
                    .wrap(McpAuth { state: state.clone() })
                    .route("/ping", web::get().to(protected_endpoint)),
            ),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mcp/ping")
            .to_request();
        // Middleware returns Err (custom ResponseError), so use try_call_service
        let resp = actix_test::try_call_service(&app, req).await;
        assert!(resp.is_err(), "Expected error response for missing token");
    }

    #[actix_web::test]
    async fn test_invalid_token_returns_401() {
        let (state, _) = test_state(Some("https://atomic.example.com"));
        let app = actix_test::init_service(
            App::new().service(
                web::scope("/mcp")
                    .wrap(McpAuth { state: state.clone() })
                    .route("/ping", web::get().to(protected_endpoint)),
            ),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mcp/ping")
            .insert_header(("Authorization", "Bearer bad_token"))
            .to_request();
        let resp = actix_test::try_call_service(&app, req).await;
        assert!(resp.is_err(), "Expected error response for invalid token");
    }

    #[actix_web::test]
    async fn test_revoked_token_returns_401() {
        let (state, raw_token) = test_state(Some("https://atomic.example.com"));

        let tokens = state.core.list_api_tokens().unwrap();
        state.core.revoke_api_token(&tokens[0].id).unwrap();

        let app = actix_test::init_service(
            App::new().service(
                web::scope("/mcp")
                    .wrap(McpAuth { state: state.clone() })
                    .route("/ping", web::get().to(protected_endpoint)),
            ),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mcp/ping")
            .insert_header(("Authorization", format!("Bearer {}", raw_token)))
            .to_request();
        let resp = actix_test::try_call_service(&app, req).await;
        assert!(resp.is_err(), "Expected error response for revoked token");
    }

    #[actix_web::test]
    async fn test_no_public_url_still_rejects() {
        let (state, _) = test_state(None);
        let app = actix_test::init_service(
            App::new().service(
                web::scope("/mcp")
                    .wrap(McpAuth { state: state.clone() })
                    .route("/ping", web::get().to(protected_endpoint)),
            ),
        )
        .await;

        let req = actix_test::TestRequest::get()
            .uri("/mcp/ping")
            .to_request();
        let resp = actix_test::try_call_service(&app, req).await;
        assert!(resp.is_err(), "Expected error response without token");
    }
}
