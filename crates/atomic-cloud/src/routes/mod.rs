pub mod admin;
pub mod checkout;
pub mod instances;
pub mod webhooks;

use actix_web::web;

pub fn configure_public_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/checkout", web::post().to(checkout::create_checkout))
        .route(
            "/api/checkout/check-subdomain",
            web::get().to(checkout::check_subdomain),
        )
        .route("/api/stripe/webhook", web::post().to(webhooks::handle_webhook));
}

pub fn configure_instance_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/instance/status", web::get().to(instances::get_status))
        .route("/api/instance/start", web::post().to(instances::start))
        .route("/api/instance/stop", web::post().to(instances::stop))
        .route("/api/instance/restart", web::post().to(instances::restart))
        .route("/api/instance/portal", web::post().to(instances::billing_portal));
}

pub fn configure_admin_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/admin/instances", web::get().to(admin::list_instances))
        .route("/api/admin/stats", web::get().to(admin::stats))
        .route(
            "/api/admin/rollout",
            web::post().to(admin::trigger_rollout),
        );
}
