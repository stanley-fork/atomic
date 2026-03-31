//! Instance control routes — start, stop, restart, status

use crate::error::CloudError;
use crate::state::CloudState;
use actix_web::{web, HttpRequest, HttpResponse};

/// Extract the instance from the management token in the Authorization header
async fn resolve_instance(
    state: &web::Data<CloudState>,
    req: &HttpRequest,
) -> Result<crate::models::Instance, CloudError> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or_else(|| CloudError::Unauthorized("Missing management token".into()))?;

    crate::db::get_instance_by_management_token(&state.db, token)
        .await?
        .ok_or_else(|| CloudError::Unauthorized("Invalid management token".into()))
}

/// GET /api/instance/status
pub async fn get_status(state: web::Data<CloudState>, req: HttpRequest) -> HttpResponse {
    let instance = match resolve_instance(&state, &req).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    // Get live status from Fly if we have a machine ID
    let fly_state = if let Some(ref machine_id) = instance.fly_machine_id {
        match state
            .fly
            .get_machine(&instance.fly_app_name, machine_id)
            .await
        {
            Ok(m) => Some(m.state),
            Err(_) => None,
        }
    } else {
        None
    };

    let subdomain_url = format!("https://{}.fly.dev", instance.fly_app_name);
    let mcp_url = format!("{}/mcp", subdomain_url);

    HttpResponse::Ok().json(serde_json::json!({
        "id": instance.id,
        "subdomain": instance.subdomain,
        "status": instance.status,
        "fly_state": fly_state,
        "subdomain_url": subdomain_url,
        "mcp_url": mcp_url,
        "created_at": instance.created_at,
    }))
}

/// POST /api/instance/start
pub async fn start(state: web::Data<CloudState>, req: HttpRequest) -> HttpResponse {
    let instance = match resolve_instance(&state, &req).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    let machine_id = match &instance.fly_machine_id {
        Some(id) => id.clone(),
        None => {
            return CloudError::BadRequest("Instance not yet provisioned".into()).to_response()
        }
    };

    match state
        .fly
        .start_machine(&instance.fly_app_name, &machine_id)
        .await
    {
        Ok(()) => {
            let _ =
                crate::db::update_instance_status(&state.db, instance.id, "running").await;
            HttpResponse::Ok().json(serde_json::json!({ "status": "starting" }))
        }
        Err(e) => e.to_response(),
    }
}

/// POST /api/instance/stop — "break glass" emergency stop
pub async fn stop(state: web::Data<CloudState>, req: HttpRequest) -> HttpResponse {
    let instance = match resolve_instance(&state, &req).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    let machine_id = match &instance.fly_machine_id {
        Some(id) => id.clone(),
        None => {
            return CloudError::BadRequest("Instance not yet provisioned".into()).to_response()
        }
    };

    match state
        .fly
        .stop_machine(&instance.fly_app_name, &machine_id)
        .await
    {
        Ok(()) => {
            let _ =
                crate::db::update_instance_status(&state.db, instance.id, "stopped").await;
            HttpResponse::Ok().json(serde_json::json!({ "status": "stopped" }))
        }
        Err(e) => e.to_response(),
    }
}

/// POST /api/instance/restart
pub async fn restart(state: web::Data<CloudState>, req: HttpRequest) -> HttpResponse {
    let instance = match resolve_instance(&state, &req).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    let machine_id = match &instance.fly_machine_id {
        Some(id) => id.clone(),
        None => {
            return CloudError::BadRequest("Instance not yet provisioned".into()).to_response()
        }
    };

    // Stop then start
    if let Err(e) = state
        .fly
        .stop_machine(&instance.fly_app_name, &machine_id)
        .await
    {
        return e.to_response();
    }

    match state
        .fly
        .start_machine(&instance.fly_app_name, &machine_id)
        .await
    {
        Ok(()) => {
            let _ =
                crate::db::update_instance_status(&state.db, instance.id, "running").await;
            HttpResponse::Ok().json(serde_json::json!({ "status": "restarting" }))
        }
        Err(e) => e.to_response(),
    }
}

/// POST /api/instance/portal — get Stripe Customer Portal URL
pub async fn billing_portal(state: web::Data<CloudState>, req: HttpRequest) -> HttpResponse {
    let instance = match resolve_instance(&state, &req).await {
        Ok(i) => i,
        Err(e) => return e.to_response(),
    };

    // Look up the customer's Stripe ID
    let customer = match crate::db::get_customer_by_id(&state.db, instance.customer_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return CloudError::NotFound("Customer not found".into()).to_response(),
        Err(e) => return e.to_response(),
    };

    let return_url = format!("{}/dashboard", state.config.public_url);
    match state
        .stripe
        .create_portal_session(&customer.stripe_customer_id, &return_url)
        .await
    {
        Ok(url) => HttpResponse::Ok().json(serde_json::json!({ "portal_url": url })),
        Err(e) => e.to_response(),
    }
}
