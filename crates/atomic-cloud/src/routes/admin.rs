//! Admin routes — instance listing, stats, rollout triggers

use crate::state::CloudState;
use actix_web::{web, HttpResponse};

/// GET /api/admin/instances — list all instances
pub async fn list_instances(state: web::Data<CloudState>) -> HttpResponse {
    match crate::db::list_instances(&state.db).await {
        Ok(instances) => HttpResponse::Ok().json(instances),
        Err(e) => e.to_response(),
    }
}

/// GET /api/admin/stats — basic stats
pub async fn stats(state: web::Data<CloudState>) -> HttpResponse {
    let instances = match crate::db::list_instances(&state.db).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    let total = instances.len();
    let running = instances.iter().filter(|i| i.status == "running").count();
    let stopped = instances.iter().filter(|i| i.status == "stopped").count();
    let provisioning = instances.iter().filter(|i| i.status == "provisioning").count();

    HttpResponse::Ok().json(serde_json::json!({
        "total_instances": total,
        "running": running,
        "stopped": stopped,
        "provisioning": provisioning,
        "mrr_estimate": total as f64 * 8.0,
    }))
}

/// POST /api/admin/rollout — trigger an image update across all running instances
pub async fn trigger_rollout(state: web::Data<CloudState>) -> HttpResponse {
    let instances = match crate::db::list_instances(&state.db).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    let running: Vec<_> = instances
        .into_iter()
        .filter(|i| i.status == "running" && i.fly_machine_id.is_some())
        .collect();

    let count = running.len();
    let fly = std::sync::Arc::clone(&state.fly);
    let image = state.config.atomic_image.clone();

    tokio::spawn(async move {
        for instance in running {
            let machine_id = instance.fly_machine_id.as_ref().unwrap();
            eprintln!("Rolling out {} to {}", image, instance.subdomain);

            if let Err(e) = fly
                .update_machine_image(&instance.fly_app_name, machine_id, &image)
                .await
            {
                eprintln!("Rollout failed for {}: {}", instance.subdomain, e);
                continue;
            }

            // Brief pause between updates to avoid thundering herd
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
        eprintln!("Rollout complete");
    });

    HttpResponse::Ok().json(serde_json::json!({
        "status": "rollout_started",
        "instance_count": count,
    }))
}
