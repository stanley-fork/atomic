//! Database queries for the management plane

use crate::error::CloudError;
use crate::models::{Customer, Event, Instance, Subscription};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

// -- Customers --

pub async fn upsert_customer(
    pool: &PgPool,
    stripe_customer_id: &str,
    email: &str,
) -> Result<Customer, CloudError> {
    sqlx::query_as::<_, Customer>(
        r#"
        INSERT INTO customers (id, stripe_customer_id, email)
        VALUES ($1, $2, $3)
        ON CONFLICT (stripe_customer_id) DO UPDATE SET
            email = EXCLUDED.email,
            updated_at = now()
        RETURNING *
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(stripe_customer_id)
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(CloudError::from)
}

pub async fn get_customer_by_stripe_id(
    pool: &PgPool,
    stripe_customer_id: &str,
) -> Result<Option<Customer>, CloudError> {
    sqlx::query_as::<_, Customer>(
        "SELECT * FROM customers WHERE stripe_customer_id = $1",
    )
    .bind(stripe_customer_id)
    .fetch_optional(pool)
    .await
    .map_err(CloudError::from)
}

pub async fn get_customer_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<Customer>, CloudError> {
    sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(CloudError::from)
}

// -- Subscriptions --

pub async fn upsert_subscription(
    pool: &PgPool,
    customer_id: Uuid,
    stripe_subscription_id: &str,
    status: &str,
    current_period_end: DateTime<Utc>,
    cancel_at: Option<DateTime<Utc>>,
) -> Result<Subscription, CloudError> {
    sqlx::query_as::<_, Subscription>(
        r#"
        INSERT INTO subscriptions (id, customer_id, stripe_subscription_id, status, current_period_end, cancel_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (stripe_subscription_id) DO UPDATE SET
            status = EXCLUDED.status,
            current_period_end = EXCLUDED.current_period_end,
            cancel_at = EXCLUDED.cancel_at,
            updated_at = now()
        RETURNING *
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(customer_id)
    .bind(stripe_subscription_id)
    .bind(status)
    .bind(current_period_end)
    .bind(cancel_at)
    .fetch_one(pool)
    .await
    .map_err(CloudError::from)
}

pub async fn get_subscription_by_stripe_id(
    pool: &PgPool,
    stripe_subscription_id: &str,
) -> Result<Option<Subscription>, CloudError> {
    sqlx::query_as::<_, Subscription>(
        "SELECT * FROM subscriptions WHERE stripe_subscription_id = $1",
    )
    .bind(stripe_subscription_id)
    .fetch_optional(pool)
    .await
    .map_err(CloudError::from)
}

pub async fn update_subscription_status(
    pool: &PgPool,
    stripe_subscription_id: &str,
    status: &str,
    cancel_at: Option<DateTime<Utc>>,
) -> Result<(), CloudError> {
    sqlx::query(
        r#"
        UPDATE subscriptions SET status = $1, cancel_at = $2, updated_at = now()
        WHERE stripe_subscription_id = $3
        "#,
    )
    .bind(status)
    .bind(cancel_at)
    .bind(stripe_subscription_id)
    .execute(pool)
    .await
    .map_err(CloudError::from)?;
    Ok(())
}

// -- Instances --

pub async fn create_instance(
    pool: &PgPool,
    customer_id: Uuid,
    subscription_id: Uuid,
    subdomain: &str,
    fly_app_name: &str,
    management_token: &str,
) -> Result<Instance, CloudError> {
    sqlx::query_as::<_, Instance>(
        r#"
        INSERT INTO instances (id, customer_id, subscription_id, subdomain, fly_app_name, status, management_token)
        VALUES ($1, $2, $3, $4, $5, 'provisioning', $6)
        RETURNING *
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(customer_id)
    .bind(subscription_id)
    .bind(subdomain)
    .bind(fly_app_name)
    .bind(management_token)
    .fetch_one(pool)
    .await
    .map_err(CloudError::from)
}

pub async fn get_instance_by_customer_id(
    pool: &PgPool,
    customer_id: Uuid,
) -> Result<Option<Instance>, CloudError> {
    sqlx::query_as::<_, Instance>(
        "SELECT * FROM instances WHERE customer_id = $1 AND status NOT IN ('destroyed', 'failed') ORDER BY created_at DESC LIMIT 1",
    )
    .bind(customer_id)
    .fetch_optional(pool)
    .await
    .map_err(CloudError::from)
}

pub async fn get_instance_by_id(
    pool: &PgPool,
    instance_id: Uuid,
) -> Result<Option<Instance>, CloudError> {
    sqlx::query_as::<_, Instance>("SELECT * FROM instances WHERE id = $1")
        .bind(instance_id)
        .fetch_optional(pool)
        .await
        .map_err(CloudError::from)
}

pub async fn get_instance_by_management_token(
    pool: &PgPool,
    token: &str,
) -> Result<Option<Instance>, CloudError> {
    sqlx::query_as::<_, Instance>("SELECT * FROM instances WHERE management_token = $1 AND status != 'destroyed'")
        .bind(token)
        .fetch_optional(pool)
        .await
        .map_err(CloudError::from)
}

pub async fn get_instance_by_subdomain(
    pool: &PgPool,
    subdomain: &str,
) -> Result<Option<Instance>, CloudError> {
    sqlx::query_as::<_, Instance>(
        "SELECT * FROM instances WHERE subdomain = $1 AND status NOT IN ('destroyed', 'failed')",
    )
    .bind(subdomain)
    .fetch_optional(pool)
        .await
        .map_err(CloudError::from)
}

pub async fn update_instance_status(
    pool: &PgPool,
    instance_id: Uuid,
    status: &str,
) -> Result<(), CloudError> {
    sqlx::query("UPDATE instances SET status = $1, updated_at = now() WHERE id = $2")
        .bind(status)
        .bind(instance_id)
        .execute(pool)
        .await
        .map_err(CloudError::from)?;
    Ok(())
}

pub async fn update_instance_fly_ids(
    pool: &PgPool,
    instance_id: Uuid,
    fly_machine_id: &str,
    fly_volume_id: &str,
) -> Result<(), CloudError> {
    sqlx::query(
        r#"
        UPDATE instances SET fly_machine_id = $1, fly_volume_id = $2, updated_at = now()
        WHERE id = $3
        "#,
    )
    .bind(fly_machine_id)
    .bind(fly_volume_id)
    .bind(instance_id)
    .execute(pool)
    .await
    .map_err(CloudError::from)?;
    Ok(())
}

pub async fn list_instances(pool: &PgPool) -> Result<Vec<Instance>, CloudError> {
    sqlx::query_as::<_, Instance>("SELECT * FROM instances ORDER BY created_at DESC")
        .fetch_all(pool)
        .await
        .map_err(CloudError::from)
}

pub async fn list_instances_for_teardown(
    pool: &PgPool,
    grace_period_days: i64,
) -> Result<Vec<Instance>, CloudError> {
    sqlx::query_as::<_, Instance>(
        r#"
        SELECT i.* FROM instances i
        JOIN subscriptions s ON i.subscription_id = s.id
        WHERE s.status = 'canceled'
        AND s.cancel_at IS NOT NULL
        AND s.cancel_at + make_interval(days => $1) < now()
        AND i.status != 'destroyed'
        "#,
    )
    .bind(grace_period_days as i32)
    .fetch_all(pool)
    .await
    .map_err(CloudError::from)
}

// -- Magic Links --

pub async fn create_magic_link(
    pool: &PgPool,
    email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), CloudError> {
    sqlx::query(
        "INSERT INTO magic_links (id, email, token, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(token)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(CloudError::from)?;
    Ok(())
}

/// Consume a magic link token. Returns the email if valid and unused.
pub async fn consume_magic_link(
    pool: &PgPool,
    token: &str,
) -> Result<Option<String>, CloudError> {
    let row = sqlx::query_as::<_, (String,)>(
        r#"
        UPDATE magic_links SET used = true
        WHERE token = $1 AND used = false AND expires_at > now()
        RETURNING email
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .map_err(CloudError::from)?;

    Ok(row.map(|r| r.0))
}

/// Check if a magic link was sent to this email within the last `seconds` seconds.
pub async fn has_recent_magic_link(
    pool: &PgPool,
    email: &str,
    seconds: i64,
) -> Result<bool, CloudError> {
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM magic_links WHERE email = $1 AND created_at > now() - make_interval(secs => $2)",
    )
    .bind(email)
    .bind(seconds as f64)
    .fetch_one(pool)
    .await
    .map_err(CloudError::from)?;

    Ok(row.0 > 0)
}

pub async fn get_customer_by_email(
    pool: &PgPool,
    email: &str,
) -> Result<Option<Customer>, CloudError> {
    sqlx::query_as::<_, Customer>("SELECT * FROM customers WHERE email = $1")
        .bind(email)
        .fetch_optional(pool)
        .await
        .map_err(CloudError::from)
}

// -- Events (idempotency) --

/// Try to insert a Stripe event for idempotency. Returns false if already processed.
pub async fn try_insert_event(
    pool: &PgPool,
    stripe_event_id: &str,
    event_type: &str,
    payload: &serde_json::Value,
) -> Result<bool, CloudError> {
    let result = sqlx::query(
        r#"
        INSERT INTO events (id, stripe_event_id, event_type, payload)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (stripe_event_id) DO NOTHING
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(stripe_event_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await
    .map_err(CloudError::from)?;

    Ok(result.rows_affected() > 0)
}
