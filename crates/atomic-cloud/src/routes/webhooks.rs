//! Stripe webhook handler — processes billing events and triggers provisioning

use crate::error::CloudError;
use crate::state::CloudState;
use actix_web::{web, HttpRequest, HttpResponse};
use std::sync::Arc;
use uuid::Uuid;

/// POST /api/stripe/webhook
pub async fn handle_webhook(
    state: web::Data<CloudState>,
    req: HttpRequest,
    body: web::Bytes,
) -> HttpResponse {
    // Verify webhook signature
    let signature = match req.headers().get("Stripe-Signature") {
        Some(v) => match v.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return CloudError::BadRequest("Invalid signature header".into()).to_response(),
        },
        None => return CloudError::BadRequest("Missing Stripe-Signature header".into()).to_response(),
    };

    let event = match state.stripe.verify_webhook(
        &state.config.stripe_webhook_secret,
        &body,
        &signature,
    ) {
        Ok(e) => e,
        Err(e) => return e.to_response(),
    };

    let event_id = event["id"].as_str().unwrap_or_default();
    let event_type = event["type"].as_str().unwrap_or_default();

    // Idempotency check
    match crate::db::try_insert_event(&state.db, event_id, event_type, &event).await {
        Ok(false) => return HttpResponse::Ok().json(serde_json::json!({ "status": "already_processed" })),
        Err(e) => return e.to_response(),
        Ok(true) => {}
    }

    // Route to handler based on event type
    let result = match event_type {
        "checkout.session.completed" => handle_checkout_completed(&state, &event).await,
        "customer.subscription.updated" => handle_subscription_updated(&state, &event).await,
        "customer.subscription.deleted" => handle_subscription_deleted(&state, &event).await,
        "invoice.payment_failed" => handle_payment_failed(&state, &event).await,
        _ => Ok(()),
    };

    match result {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "status": "processed" })),
        Err(e) => {
            eprintln!("Webhook error for {event_type}: {e}");
            // Return 200 to Stripe even on error — we don't want retries for business logic failures
            // The event is already recorded for debugging
            HttpResponse::Ok().json(serde_json::json!({ "status": "error", "message": e.to_string() }))
        }
    }
}

async fn handle_checkout_completed(
    state: &web::Data<CloudState>,
    event: &serde_json::Value,
) -> Result<(), CloudError> {
    let session = &event["data"]["object"];
    let stripe_customer_id = session["customer"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing customer in checkout session".into()))?;
    let email = session["customer_email"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing email in checkout session".into()))?
        .to_lowercase();
    let stripe_subscription_id = session["subscription"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing subscription in checkout session".into()))?;
    let subdomain = session["metadata"]["subdomain"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing subdomain in checkout metadata".into()))?;

    // Upsert customer
    let customer = crate::db::upsert_customer(&state.db, stripe_customer_id, &email).await?;

    // Create subscription record
    let subscription = crate::db::upsert_subscription(
        &state.db,
        customer.id,
        stripe_subscription_id,
        "active",
        chrono::Utc::now() + chrono::Duration::days(30), // approximate; will be updated by subscription.updated
        None,
    )
    .await?;

    // Generate management token
    let management_token = Uuid::new_v4().to_string();

    // Each customer gets their own Fly app: atomic-{subdomain}
    let fly_app_name = format!("atomic-{subdomain}");

    // Create instance record — if subdomain or customer conflict, cancel and refund
    let instance = match crate::db::create_instance(
        &state.db,
        customer.id,
        subscription.id,
        subdomain,
        &fly_app_name,
        &management_token,
    )
    .await
    {
        Ok(i) => i,
        Err(_) => {
            eprintln!("Instance conflict for {subdomain}, canceling subscription {stripe_subscription_id}");
            let _ = state.stripe.cancel_subscription(stripe_subscription_id).await;
            return Err(CloudError::Conflict(
                "Subdomain or account conflict — subscription canceled and refunded".into(),
            ));
        }
    };

    // Spawn async provisioning task
    let fly = Arc::clone(&state.fly);
    let db = state.db.clone();
    let image = state.config.atomic_image.clone();
    let region = state.config.fly_region.clone();
    let fly_org = state.config.fly_org.clone();
    let instance_id = instance.id;
    let subdomain = subdomain.to_string();

    tokio::spawn(async move {
        if let Err(e) = provision_instance(
            &fly, &db, &fly_app_name, &fly_org, &image, &region, instance_id, &subdomain,
        )
        .await
        {
            eprintln!("Provisioning failed for {subdomain}: {e}");
            // Clean up the Fly app to avoid blocking re-subscription
            if let Err(cleanup_err) = fly.delete_app(&fly_app_name).await {
                eprintln!("Failed to clean up Fly app {fly_app_name}: {cleanup_err}");
            }
            let _ = crate::db::update_instance_status(&db, instance_id, "failed").await;
        }
    });

    Ok(())
}

async fn provision_instance(
    fly: &crate::clients::fly::FlyClient,
    db: &sqlx::PgPool,
    app_name: &str,
    org_slug: &str,
    image: &str,
    region: &str,
    instance_id: uuid::Uuid,
    subdomain: &str,
) -> Result<(), CloudError> {
    // Step 1: Create a dedicated Fly app for this customer
    fly.create_app(app_name, org_slug).await?;
    eprintln!("Created Fly app: {app_name}");

    // Step 2: Allocate IP addresses
    fly.allocate_ips(app_name).await?;
    eprintln!("Allocated IPs for {app_name}");

    // Step 3: Create volume
    let volume_name = format!("{}_data", subdomain.replace('-', "_"));
    let volume = fly
        .create_volume(app_name, &volume_name, 3, region)
        .await?;

    // Step 4: Create machine
    let machine = fly
        .create_machine(app_name, subdomain, image, &volume.id, region)
        .await?;

    // Update instance with Fly IDs
    crate::db::update_instance_fly_ids(db, instance_id, &machine.id, &volume.id).await?;
    crate::db::update_instance_status(db, instance_id, "running").await?;

    eprintln!("Provisioned {subdomain}: app={app_name}, machine={}, volume={}", machine.id, volume.id);
    Ok(())
}

async fn handle_subscription_updated(
    state: &web::Data<CloudState>,
    event: &serde_json::Value,
) -> Result<(), CloudError> {
    let subscription = &event["data"]["object"];
    let stripe_subscription_id = subscription["id"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing subscription id".into()))?;
    let status = subscription["status"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing subscription status".into()))?;

    let cancel_at = subscription["cancel_at"]
        .as_i64()
        .map(|ts| chrono::DateTime::from_timestamp(ts, 0))
        .flatten();

    crate::db::update_subscription_status(&state.db, stripe_subscription_id, status, cancel_at)
        .await?;

    Ok(())
}

async fn handle_subscription_deleted(
    state: &web::Data<CloudState>,
    event: &serde_json::Value,
) -> Result<(), CloudError> {
    let subscription = &event["data"]["object"];
    let stripe_subscription_id = subscription["id"]
        .as_str()
        .ok_or_else(|| CloudError::BadRequest("Missing subscription id".into()))?;

    crate::db::update_subscription_status(
        &state.db,
        stripe_subscription_id,
        "canceled",
        Some(chrono::Utc::now()),
    )
    .await?;

    Ok(())
}

async fn handle_payment_failed(
    state: &web::Data<CloudState>,
    event: &serde_json::Value,
) -> Result<(), CloudError> {
    let invoice = &event["data"]["object"];
    let stripe_subscription_id = match invoice["subscription"].as_str() {
        Some(id) => id,
        None => return Ok(()), // Not all invoices have subscriptions
    };

    crate::db::update_subscription_status(&state.db, stripe_subscription_id, "past_due", None)
        .await?;

    Ok(())
}
