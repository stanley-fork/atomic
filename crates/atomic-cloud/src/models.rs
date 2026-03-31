//! Database models for the management plane

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Customer {
    pub id: Uuid,
    pub stripe_customer_id: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Subscription {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub stripe_subscription_id: String,
    pub status: String,
    pub current_period_end: DateTime<Utc>,
    pub cancel_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Instance {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub subscription_id: Option<Uuid>,
    pub subdomain: String,
    pub fly_machine_id: Option<String>,
    pub fly_volume_id: Option<String>,
    pub fly_app_name: String,
    pub status: String,
    pub server_version: Option<String>,
    pub management_token: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Event {
    pub id: Uuid,
    pub stripe_event_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub processed_at: DateTime<Utc>,
}

/// Instance status values
pub mod status {
    pub const PENDING: &str = "pending";
    pub const PROVISIONING: &str = "provisioning";
    pub const RUNNING: &str = "running";
    pub const STOPPED: &str = "stopped";
    pub const DESTROYING: &str = "destroying";
    pub const DESTROYED: &str = "destroyed";
}

/// Subscription status values
pub mod subscription_status {
    pub const ACTIVE: &str = "active";
    pub const PAST_DUE: &str = "past_due";
    pub const CANCELED: &str = "canceled";
    pub const UNPAID: &str = "unpaid";
}
