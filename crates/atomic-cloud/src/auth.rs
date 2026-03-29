//! Authentication middleware for the management plane
//!
//! Two middleware implementations:
//! - AdminAuth: validates against a shared admin API key
//! - InstanceAuth: validates against per-instance management tokens

use crate::state::CloudState;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::error::ErrorUnauthorized;
use actix_web::{web, Error};
use futures::future::{ok, LocalBoxFuture, Ready};
use std::task::{Context, Poll};

// -- Admin Auth --

/// Middleware that requires a valid admin API key via Bearer token
pub struct AdminAuth;

impl<S, B> Transform<S, ServiceRequest> for AdminAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = AdminAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AdminAuthMiddleware { service })
    }
}

pub struct AdminAuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AdminAuthMiddleware<S>
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
        let state = req
            .app_data::<web::Data<CloudState>>()
            .expect("CloudState not configured");

        let token = extract_bearer_token(&req);
        let valid = match token {
            Some(t) => t == state.config.admin_api_key,
            None => false,
        };

        if !valid {
            return Box::pin(async { Err(ErrorUnauthorized("Invalid or missing admin API key")) });
        }

        let fut = self.service.call(req);
        Box::pin(async move { fut.await })
    }
}

// -- Instance Auth --

/// Middleware that validates per-instance management tokens.
/// On success, inserts the Instance into request extensions.
pub struct InstanceAuth;

impl<S, B> Transform<S, ServiceRequest> for InstanceAuth
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Transform = InstanceAuthMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(InstanceAuthMiddleware { service })
    }
}

pub struct InstanceAuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for InstanceAuthMiddleware<S>
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
        let state = req
            .app_data::<web::Data<CloudState>>()
            .expect("CloudState not configured");

        let token = match extract_bearer_token(&req) {
            Some(t) => t,
            None => {
                return Box::pin(async {
                    Err(ErrorUnauthorized("Invalid or missing management token"))
                });
            }
        };

        let pool = state.db.clone();
        let fut = self.service.call(req);

        Box::pin(async move {
            let instance = crate::db::get_instance_by_management_token(&pool, &token).await;
            match instance {
                Ok(Some(_)) => fut.await,
                _ => Err(ErrorUnauthorized("Invalid or missing management token")),
            }
        })
    }
}

fn extract_bearer_token(req: &ServiceRequest) -> Option<String> {
    req.headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(String::from)
}
