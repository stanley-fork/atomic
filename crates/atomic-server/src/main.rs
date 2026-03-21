//! Atomic Server — standalone HTTP server for the Atomic knowledge base
//!
//! Wraps atomic-core with a REST API + WebSocket events.
//! No Tauri dependency.

mod auth;
mod config;
mod db_extractor;
mod error;
mod event_bridge;
mod mcp;
mod mcp_auth;
mod routes;
mod state;
mod ws;

use actix_cors::Cors;
use actix_web::{middleware, web, App, HttpResponse, HttpServer, Responder};
use clap::Parser;
use config::{Cli, Command, TokenAction};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp_actix_web::transport::StreamableHttpService;
use state::AppState;
use std::sync::Arc;
use std::time::Duration;

async fn health() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let data_dir = cli.resolve_data_dir();

    match cli.command {
        // Token management subcommands (no server needed)
        Some(Command::Token { action }) => {
            let manager = atomic_core::DatabaseManager::new(&data_dir)
                .expect("Failed to open database manager");
            let core = manager.active_core()
                .expect("Failed to get active database");
            run_token_command(&core, action);
            Ok(())
        }

        // Server mode
        Some(Command::Serve { port, bind, public_url }) => {
            // Auto-detect public URL on Fly.io if not explicitly set
            let public_url = public_url.or_else(|| {
                std::env::var("FLY_APP_NAME").ok().map(|name| format!("https://{name}.fly.dev"))
            });
            let manager = atomic_core::DatabaseManager::new(&data_dir)
                .expect("Failed to open database manager");
            run_server(manager, &data_dir.display().to_string(), port, &bind, public_url).await
        }
        None => {
            let manager = atomic_core::DatabaseManager::new(&data_dir)
                .expect("Failed to open database manager");
            run_server(manager, &data_dir.display().to_string(), 8080, "127.0.0.1", None).await
        }
    }
}

fn run_token_command(core: &atomic_core::AtomicCore, action: TokenAction) {
    match action {
        TokenAction::Create { name } => {
            match core.create_api_token(&name) {
                Ok((info, raw_token)) => {
                    println!("Token created:");
                    println!("  ID:     {}", info.id);
                    println!("  Name:   {}", info.name);
                    println!("  Token:  {}", raw_token);
                    println!();
                    println!("Save this token — it won't be shown again.");
                }
                Err(e) => {
                    eprintln!("Failed to create token: {}", e);
                    std::process::exit(1);
                }
            }
        }
        TokenAction::List => {
            match core.list_api_tokens() {
                Ok(tokens) => {
                    if tokens.is_empty() {
                        println!("No API tokens found.");
                    } else {
                        println!(
                            "{:<38} {:<24} {:<12} {:<22} {:<22} {}",
                            "ID", "NAME", "PREFIX", "CREATED", "LAST USED", "STATUS"
                        );
                        for t in &tokens {
                            let status = if t.is_revoked { "REVOKED" } else { "active" };
                            let last_used = t.last_used_at.as_deref().unwrap_or("never");
                            // Truncate timestamps to 19 chars (drop timezone)
                            let created = &t.created_at[..t.created_at.len().min(19)];
                            let last_used = &last_used[..last_used.len().min(19)];
                            println!(
                                "{:<38} {:<24} {:<12} {:<22} {:<22} {}",
                                t.id, t.name, t.token_prefix, created, last_used, status
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to list tokens: {}", e);
                    std::process::exit(1);
                }
            }
        }
        TokenAction::Revoke { id } => {
            match core.revoke_api_token(&id) {
                Ok(()) => println!("Token {} revoked.", id),
                Err(e) => {
                    eprintln!("Failed to revoke token: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

async fn run_server(
    manager: atomic_core::DatabaseManager,
    data_dir: &str,
    port: u16,
    bind: &str,
    public_url: Option<String>,
) -> std::io::Result<()> {
    let manager = Arc::new(manager);

    // Get active core for startup tasks
    let core = manager.active_core().expect("Failed to get active database");

    // Migrate legacy token if present
    match core.migrate_legacy_token() {
        Ok(true) => println!("  Migrated legacy auth token to new token system"),
        Ok(false) => {}
        Err(e) => eprintln!("  Warning: failed to migrate legacy token: {}", e),
    }

    // Check token status
    match core.list_api_tokens() {
        Ok(tokens) => {
            let active = tokens.iter().filter(|t| !t.is_revoked).count();
            if active == 0 {
                println!("  No API tokens configured — open the web UI to claim this instance");
                println!("  Or create one with: atomic-server token create --name default");
            } else {
                println!("  {} active API token(s) configured", active);
            }
        }
        Err(e) => eprintln!("  Warning: failed to check tokens: {}", e),
    }

    // Create broadcast channel for WebSocket events (buffer 256 events)
    let (event_tx, _) = tokio::sync::broadcast::channel(256);

    let app_state = web::Data::new(AppState {
        manager: Arc::clone(&manager),
        event_tx: event_tx.clone(),
        public_url: public_url.clone(),
    });

    // Create MCP service
    let mcp_manager = Arc::clone(&manager);
    let mcp_tx = event_tx.clone();
    let mcp_service = StreamableHttpService::builder()
        .service_factory(Arc::new(move || {
            let core = mcp_manager.active_core()
                .expect("Failed to get active database for MCP");
            Ok(mcp::AtomicMcpServer::new(
                core,
                mcp_tx.clone(),
            ))
        }))
        .session_manager(Arc::new(LocalSessionManager::default()))
        .stateful_mode(true)
        .sse_keep_alive(Duration::from_secs(30))
        .build();

    println!("Atomic Server starting...");
    println!("  Data dir: {}", data_dir);
    println!("  Listening: http://{}:{}", bind, port);
    if let Some(ref url) = public_url {
        println!("  Public URL: {}", url);
    }
    println!();
    println!("  Health: http://{}:{}/health", bind, port);
    println!("  MCP: http://{}:{}/mcp", bind, port);
    println!(
        "  WebSocket: ws://{}:{}/ws?token=<token>",
        bind, port
    );

    // Startup recovery: reset stuck atoms and process any pending work for ALL databases
    {
        let (databases, _active_id) = manager.list_databases().unwrap_or_default();
        for db_info in &databases {
            let db_core = match manager.get_core(&db_info.id) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("  Warning: failed to load database '{}': {}", db_info.name, e);
                    continue;
                }
            };
            let on_event = event_bridge::embedding_event_callback(app_state.event_tx.clone());

            match db_core.reset_stuck_processing() {
                Ok(count) if count > 0 => println!("  [{}] Reset {} atoms stuck in processing state", db_info.name, count),
                Ok(_) => {}
                Err(e) => eprintln!("  Warning: [{}] failed to reset stuck processing: {}", db_info.name, e),
            }

            match db_core.process_pending_embeddings(on_event.clone()) {
                Ok(count) if count > 0 => println!("  [{}] Processing {} pending embeddings in background", db_info.name, count),
                Ok(_) => {}
                Err(e) => eprintln!("  Warning: [{}] failed to start pending embeddings: {}", db_info.name, e),
            }

            match db_core.process_pending_tagging(on_event) {
                Ok(count) if count > 0 => println!("  [{}] Processing {} pending tagging operations in background", db_info.name, count),
                Ok(_) => {}
                Err(e) => eprintln!("  Warning: [{}] failed to start pending tagging: {}", db_info.name, e),
            }
        }
    }

    // Spawn feed polling scheduler (ticks every 60 seconds, polls all databases)
    {
        let poll_manager = Arc::clone(&manager);
        let poll_tx = event_tx.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.tick().await; // first tick fires immediately — skip it
            loop {
                interval.tick().await;
                let databases = match poll_manager.list_databases() {
                    Ok((dbs, _)) => dbs,
                    Err(_) => continue,
                };
                for db_info in &databases {
                    let db_core = match poll_manager.get_core(&db_info.id) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    let on_ingest = event_bridge::ingestion_event_callback(poll_tx.clone());
                    let on_embed = event_bridge::embedding_event_callback(poll_tx.clone());
                    let results = db_core.poll_due_feeds(on_ingest, on_embed).await;
                    for r in &results {
                        if r.new_items > 0 {
                            eprintln!(
                                "[{}] Feed {}: {} new, {} skipped, {} errors",
                                db_info.name, r.feed_id, r.new_items, r.skipped, r.errors
                            );
                        }
                    }
                }
            }
        });
    }

    let bind_owned = bind.to_string();
    let shutdown_manager = Arc::clone(&manager);

    HttpServer::new(move || {
        let cors = Cors::permissive();

        App::new()
            .wrap(cors)
            .wrap(middleware::Compress::default())
            .app_data(app_state.clone())
            // Public routes (no auth)
            .route("/health", web::get().to(health))
            .route("/ws", web::get().to(ws::ws_handler))
            // OAuth discovery (public, no auth)
            .route(
                "/.well-known/oauth-authorization-server",
                web::get().to(routes::oauth::metadata),
            )
            .route(
                "/.well-known/oauth-protected-resource",
                web::get().to(routes::oauth::resource_metadata),
            )
            .route(
                "/.well-known/oauth-protected-resource/mcp",
                web::get().to(routes::oauth::resource_metadata),
            )
            // Instance setup (public, no auth — guarded by zero-token check)
            .route("/api/setup/status", web::get().to(routes::setup::setup_status))
            .route("/api/setup/claim", web::post().to(routes::setup::claim_instance))
            // OAuth flow (public, no auth)
            .route("/oauth/register", web::post().to(routes::oauth::register))
            .route(
                "/oauth/authorize",
                web::get().to(routes::oauth::authorize_page),
            )
            .route(
                "/oauth/authorize",
                web::post().to(routes::oauth::authorize_approve),
            )
            .route("/oauth/token", web::post().to(routes::oauth::token))
            // MCP endpoint with MCP-aware auth
            .service(
                web::scope("/mcp")
                    .wrap(mcp_auth::McpAuth {
                        state: app_state.clone(),
                    })
                    .service(mcp_service.clone().scope()),
            )
            // Authenticated API routes
            .service(
                web::scope("/api")
                    .wrap(auth::BearerAuth {
                        state: app_state.clone(),
                    })
                    .configure(routes::configure_routes),
            )
    })
    .workers(4)
    .bind((bind_owned.as_str(), port))?
    .run()
    .await?;

    // Graceful shutdown: update query planner statistics
    println!("Shutting down — running PRAGMA optimize...");
    shutdown_manager.optimize_all();

    Ok(())
}
