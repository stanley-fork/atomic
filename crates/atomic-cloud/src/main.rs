//! Atomic Cloud — management plane for Atomic managed hosting

mod auth;
mod clients;
mod config;
mod db;
mod error;
mod jobs;
mod models;
mod routes;
mod state;

use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use clap::Parser;
use config::{Cli, Command};
use std::sync::Arc;

async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "service": "atomic-cloud",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Serve {
            port,
            bind,
            database_url,
            stripe_secret_key,
            stripe_webhook_secret,
            stripe_price_id,
            fly_api_token,
            fly_app_name,
            fly_region,
            atomic_image,
            base_domain,
            admin_api_key,
            public_url,
            frontend_dir,
        }) => {
            run_server(
                port,
                &bind,
                &database_url,
                stripe_secret_key,
                stripe_webhook_secret,
                stripe_price_id,
                fly_api_token,
                fly_app_name,
                fly_region,
                atomic_image,
                base_domain,
                admin_api_key,
                public_url,
                frontend_dir,
            )
            .await
        }
        None => {
            eprintln!("Usage: atomic-cloud serve [OPTIONS]");
            eprintln!("Run `atomic-cloud serve --help` for details.");
            std::process::exit(1);
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_server(
    port: u16,
    bind: &str,
    database_url: &str,
    stripe_secret_key: String,
    stripe_webhook_secret: String,
    stripe_price_id: String,
    fly_api_token: String,
    fly_app_name: String,
    fly_region: String,
    atomic_image: String,
    base_domain: String,
    admin_api_key: String,
    public_url: String,
    frontend_dir: String,
) -> std::io::Result<()> {
    // Connect to PostgreSQL
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .expect("Failed to connect to PostgreSQL");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run database migrations");

    eprintln!("Atomic Cloud starting...");
    eprintln!("  Listening: http://{}:{}", bind, port);
    eprintln!("  Public URL: {}", public_url);
    eprintln!("  Fly app: {}", fly_app_name);
    eprintln!("  Base domain: {}", base_domain);

    let fly_client = Arc::new(clients::fly::FlyClient::new(fly_api_token));

    // Spawn background cleanup job
    jobs::spawn_cleanup_job(pool.clone(), Arc::clone(&fly_client));

    let app_state = web::Data::new(state::CloudState {
        db: pool,
        stripe: clients::stripe::StripeClient::new(stripe_secret_key),
        fly: fly_client,
        config: state::CloudConfig {
            stripe_price_id,
            stripe_webhook_secret,
            base_domain,
            atomic_image,
            fly_app_name,
            fly_region,
            admin_api_key,
            public_url,
        },
    });

    let bind_owned = bind.to_string();
    let frontend_dir_owned = frontend_dir.clone();

    HttpServer::new(move || {
        let cors = Cors::permissive();

        let mut app = App::new()
            .wrap(cors)
            .app_data(app_state.clone())
            // Health check (public)
            .route("/health", web::get().to(health))
            // Public routes (no auth)
            .configure(routes::configure_public_routes)
            // Instance management routes (management token auth)
            .service(
                web::scope("")
                    .wrap(auth::InstanceAuth)
                    .configure(routes::configure_instance_routes),
            )
            // Admin routes (admin API key auth)
            .service(
                web::scope("")
                    .wrap(auth::AdminAuth)
                    .configure(routes::configure_admin_routes),
            );

        // Serve frontend static files if the directory exists
        let frontend_path = std::path::Path::new(&frontend_dir_owned);
        if frontend_path.exists() {
            app = app.service(
                actix_files::Files::new("/", &frontend_dir_owned)
                    .index_file("index.html")
                    .default_handler(
                        actix_files::NamedFile::open(
                            frontend_path.join("index.html"),
                        )
                        .expect("Frontend index.html not found"),
                    ),
            );
        }

        app
    })
    .workers(4)
    .bind((bind_owned.as_str(), port))?
    .run()
    .await
}
