//! Stripe API client — checkout sessions and webhook verification

use crate::error::CloudError;
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub struct StripeClient {
    secret_key: String,
    http: reqwest::Client,
}

impl StripeClient {
    pub fn new(secret_key: String) -> Self {
        Self {
            secret_key,
            http: reqwest::Client::new(),
        }
    }

    /// Create a Stripe Checkout Session and return the URL
    pub async fn create_checkout_session(
        &self,
        price_id: &str,
        customer_email: &str,
        subdomain: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<String, CloudError> {
        let params = [
            ("mode", "subscription"),
            ("payment_method_types[0]", "card"),
            ("line_items[0][price]", price_id),
            ("line_items[0][quantity]", "1"),
            ("customer_email", customer_email),
            ("metadata[subdomain]", subdomain),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
        ];

        let resp = self
            .http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, None::<&str>)
            .form(&params)
            .send()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Stripe(format!("Checkout session failed: {body}")));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))?;

        json["url"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| CloudError::Stripe("No URL in checkout session response".into()))
    }

    /// Retrieve a Checkout Session by ID
    pub async fn retrieve_checkout_session(
        &self,
        session_id: &str,
    ) -> Result<serde_json::Value, CloudError> {
        let url = format!("https://api.stripe.com/v1/checkout/sessions/{}", session_id);

        let resp = self
            .http
            .get(&url)
            .basic_auth(&self.secret_key, None::<&str>)
            .send()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Stripe(format!("Retrieve session failed: {body}")));
        }

        resp.json()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))
    }

    /// Create a Stripe Customer Portal session and return the URL
    pub async fn create_portal_session(
        &self,
        stripe_customer_id: &str,
        return_url: &str,
    ) -> Result<String, CloudError> {
        let params = [
            ("customer", stripe_customer_id),
            ("return_url", return_url),
        ];

        let resp = self
            .http
            .post("https://api.stripe.com/v1/billing_portal/sessions")
            .basic_auth(&self.secret_key, None::<&str>)
            .form(&params)
            .send()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Stripe(format!("Portal session failed: {body}")));
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))?;

        json["url"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| CloudError::Stripe("No URL in portal session response".into()))
    }

    /// Cancel a Stripe subscription immediately (triggers a prorated refund)
    pub async fn cancel_subscription(
        &self,
        subscription_id: &str,
    ) -> Result<(), CloudError> {
        let url = format!(
            "https://api.stripe.com/v1/subscriptions/{}",
            subscription_id
        );

        let resp = self
            .http
            .delete(&url)
            .basic_auth(&self.secret_key, None::<&str>)
            .send()
            .await
            .map_err(|e| CloudError::Stripe(e.to_string()))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(CloudError::Stripe(format!("Cancel subscription failed: {body}")));
        }

        Ok(())
    }

    /// Verify a Stripe webhook signature and parse the event payload.
    ///
    /// See: https://docs.stripe.com/webhooks/signatures
    pub fn verify_webhook(
        &self,
        webhook_secret: &str,
        payload: &[u8],
        signature_header: &str,
    ) -> Result<serde_json::Value, CloudError> {
        // Parse the Stripe-Signature header
        let mut timestamp = None;
        let mut signatures = Vec::new();

        for part in signature_header.split(',') {
            let mut kv = part.splitn(2, '=');
            match (kv.next(), kv.next()) {
                (Some("t"), Some(t)) => timestamp = Some(t.to_string()),
                (Some("v1"), Some(sig)) => signatures.push(sig.to_string()),
                _ => {}
            }
        }

        let timestamp =
            timestamp.ok_or_else(|| CloudError::BadRequest("Missing webhook timestamp".into()))?;

        if signatures.is_empty() {
            return Err(CloudError::BadRequest(
                "Missing webhook signature".into(),
            ));
        }

        // Reject events older than 5 minutes (Stripe recommendation)
        const TOLERANCE_SECS: i64 = 300;
        let ts: i64 = timestamp
            .parse()
            .map_err(|_| CloudError::BadRequest("Invalid webhook timestamp".into()))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        if (now - ts).abs() > TOLERANCE_SECS {
            return Err(CloudError::BadRequest(
                "Webhook timestamp too old".into(),
            ));
        }

        // Compute expected signature
        let signed_payload = format!("{}.{}", timestamp, String::from_utf8_lossy(payload));
        let mut mac = Hmac::<Sha256>::new_from_slice(webhook_secret.as_bytes())
            .map_err(|e| CloudError::Internal(e.to_string()))?;
        mac.update(signed_payload.as_bytes());
        let expected = hex::encode(mac.finalize().into_bytes());

        // Check if any provided signature matches
        if !signatures.iter().any(|sig| sig == &expected) {
            return Err(CloudError::BadRequest(
                "Invalid webhook signature".into(),
            ));
        }

        // Parse the JSON payload
        serde_json::from_slice(payload)
            .map_err(|e| CloudError::BadRequest(format!("Invalid webhook payload: {e}")))
    }
}
