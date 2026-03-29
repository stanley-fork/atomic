//! Background jobs — grace period cleanup and scheduled tasks

use crate::clients::fly::FlyClient;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

/// Spawn the grace period cleanup job.
/// Runs every hour and destroys instances whose subscriptions were canceled > 30 days ago.
pub fn spawn_cleanup_job(pool: PgPool, fly: Arc<FlyClient>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        interval.tick().await; // skip first immediate tick
        loop {
            interval.tick().await;
            if let Err(e) = run_cleanup(&pool, &fly).await {
                eprintln!("Cleanup job error: {e}");
            }
        }
    });
}

async fn run_cleanup(pool: &PgPool, fly: &FlyClient) -> Result<(), crate::error::CloudError> {
    let instances = crate::db::list_instances_for_teardown(pool, 30).await?;

    for instance in instances {
        eprintln!("Tearing down expired instance: {}", instance.subdomain);

        crate::db::update_instance_status(pool, instance.id, "destroying").await?;

        if let Some(ref machine_id) = instance.fly_machine_id {
            if let Err(e) = fly.destroy_machine(&instance.fly_app_name, machine_id).await {
                eprintln!("Failed to destroy machine for {}: {e}", instance.subdomain);
                continue;
            }
        }

        crate::db::update_instance_status(pool, instance.id, "destroyed").await?;
        eprintln!("Destroyed instance: {}", instance.subdomain);
    }

    Ok(())
}
