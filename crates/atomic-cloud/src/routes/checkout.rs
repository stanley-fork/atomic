//! Checkout routes — subdomain validation and Stripe Checkout session creation

use crate::error::CloudError;
use crate::state::CloudState;
use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CheckoutRequest {
    pub email: String,
    pub subdomain: String,
}

#[derive(Deserialize)]
pub struct CheckSubdomainQuery {
    pub subdomain: String,
}

/// Validate a subdomain: lowercase alphanumeric + hyphens, 3-30 chars, not reserved
fn validate_subdomain(subdomain: &str) -> Result<(), CloudError> {
    if subdomain.len() < 3 || subdomain.len() > 30 {
        return Err(CloudError::BadRequest(
            "Subdomain must be 3-30 characters".into(),
        ));
    }

    if !subdomain
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(CloudError::BadRequest(
            "Subdomain must contain only lowercase letters, digits, and hyphens".into(),
        ));
    }

    if subdomain.starts_with('-') || subdomain.ends_with('-') {
        return Err(CloudError::BadRequest(
            "Subdomain cannot start or end with a hyphen".into(),
        ));
    }

    const RESERVED: &[&str] = &[
        "www", "api", "app", "admin", "mail", "smtp", "ftp", "ssh", "test", "staging", "dev",
        "status", "docs", "help", "support", "blog", "cloud", "manage",
    ];
    if RESERVED.contains(&subdomain) {
        return Err(CloudError::Conflict(
            "This subdomain is reserved".into(),
        ));
    }

    Ok(())
}

/// POST /api/checkout — create a Stripe Checkout session
pub async fn create_checkout(
    state: web::Data<CloudState>,
    body: web::Json<CheckoutRequest>,
) -> HttpResponse {
    // Validate subdomain format
    if let Err(e) = validate_subdomain(&body.subdomain) {
        return e.to_response();
    }

    // Check subdomain availability
    match crate::db::get_instance_by_subdomain(&state.db, &body.subdomain).await {
        Ok(Some(_)) => {
            return CloudError::Conflict("This subdomain is already taken".into()).to_response()
        }
        Err(e) => return e.to_response(),
        Ok(None) => {}
    }

    // Create Stripe Checkout session
    let success_url = format!(
        "{}/success?session_id={{CHECKOUT_SESSION_ID}}",
        state.config.public_url
    );
    let cancel_url = format!("{}/", state.config.public_url);

    match state
        .stripe
        .create_checkout_session(
            &state.config.stripe_price_id,
            &body.email,
            &body.subdomain,
            &success_url,
            &cancel_url,
        )
        .await
    {
        Ok(url) => HttpResponse::Ok().json(serde_json::json!({ "checkout_url": url })),
        Err(e) => e.to_response(),
    }
}

/// GET /api/checkout/check-subdomain — check if a subdomain is available
pub async fn check_subdomain(
    state: web::Data<CloudState>,
    query: web::Query<CheckSubdomainQuery>,
) -> HttpResponse {
    if let Err(e) = validate_subdomain(&query.subdomain) {
        return HttpResponse::Ok().json(serde_json::json!({
            "available": false,
            "reason": e.to_string()
        }));
    }

    match crate::db::get_instance_by_subdomain(&state.db, &query.subdomain).await {
        Ok(Some(_)) => HttpResponse::Ok().json(serde_json::json!({
            "available": false,
            "reason": "This subdomain is already taken"
        })),
        Ok(None) => HttpResponse::Ok().json(serde_json::json!({ "available": true })),
        Err(e) => e.to_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_subdomains() {
        assert!(validate_subdomain("kenny").is_ok());
        assert!(validate_subdomain("my-notes").is_ok());
        assert!(validate_subdomain("abc123").is_ok());
        assert!(validate_subdomain("a-b-c").is_ok());
    }

    #[test]
    fn test_invalid_subdomains() {
        assert!(validate_subdomain("ab").is_err()); // too short
        assert!(validate_subdomain("a".repeat(31).as_str()).is_err()); // too long
        assert!(validate_subdomain("My-Notes").is_err()); // uppercase
        assert!(validate_subdomain("my_notes").is_err()); // underscore
        assert!(validate_subdomain("-leading").is_err()); // leading hyphen
        assert!(validate_subdomain("trailing-").is_err()); // trailing hyphen
        assert!(validate_subdomain("my notes").is_err()); // space
    }

    #[test]
    fn test_reserved_subdomains() {
        assert!(validate_subdomain("www").is_err());
        assert!(validate_subdomain("admin").is_err());
        assert!(validate_subdomain("api").is_err());
    }
}
