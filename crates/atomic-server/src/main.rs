//! Atomic Server — standalone HTTP server for the Atomic knowledge base
//!
//! Wraps atomic-core with a REST API + WebSocket events.
//! No Tauri dependency.

mod auth;
mod config;
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

    // Initialize atomic-core
    let core = atomic_core::AtomicCore::open_or_create(&cli.db_path)
        .expect("Failed to open database");

    match cli.command {
        // Token management subcommands (no server needed)
        Some(Command::Token { action }) => {
            run_token_command(&core, action);
            Ok(())
        }

        // Server mode (default)
        Some(Command::Serve { port, bind, public_url }) => {
            run_server(core, &cli.db_path, port, &bind, public_url).await
        }
        None => run_server(core, &cli.db_path, 8080, "127.0.0.1", None).await,
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
    core: atomic_core::AtomicCore,
    db_path: &str,
    port: u16,
    bind: &str,
    public_url: Option<String>,
) -> std::io::Result<()> {
    // Migrate legacy token if present
    match core.migrate_legacy_token() {
        Ok(true) => println!("  Migrated legacy auth token to new token system"),
        Ok(false) => {}
        Err(e) => eprintln!("  Warning: failed to migrate legacy token: {}", e),
    }

    // Ensure at least one token exists
    match core.ensure_default_token() {
        Ok(Some((_info, raw_token))) => {
            println!("  New API token created: {}", raw_token);
            println!("  Save this token — it won't be shown again.");
            println!();
        }
        Ok(None) => {
            // Tokens already exist
            let tokens = core.list_api_tokens().unwrap_or_default();
            let active = tokens.iter().filter(|t| !t.is_revoked).count();
            println!("  {} active API token(s) configured", active);
        }
        Err(e) => eprintln!("  Warning: failed to ensure default token: {}", e),
    }

    // Create broadcast channel for WebSocket events (buffer 256 events)
    let (event_tx, _) = tokio::sync::broadcast::channel(256);

    let app_state = web::Data::new(AppState {
        core: core.clone(),
        event_tx: event_tx.clone(),
        public_url: public_url.clone(),
    });

    // Create MCP service
    let mcp_core = core.clone();
    let mcp_tx = event_tx.clone();
    let mcp_service = StreamableHttpService::builder()
        .service_factory(Arc::new(move || {
            Ok(mcp::AtomicMcpServer::new(
                mcp_core.clone(),
                mcp_tx.clone(),
            ))
        }))
        .session_manager(Arc::new(LocalSessionManager::default()))
        .stateful_mode(false)
        .sse_keep_alive(Duration::from_secs(30))
        .build();

    println!("Atomic Server starting...");
    println!("  Database: {}", db_path);
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

    // Startup recovery: reset stuck atoms and process any pending work
    {
        let on_event = event_bridge::embedding_event_callback(app_state.event_tx.clone());

        match app_state.core.reset_stuck_processing() {
            Ok(count) if count > 0 => println!("  Reset {} atoms stuck in processing state", count),
            Ok(_) => {}
            Err(e) => eprintln!("  Warning: failed to reset stuck processing: {}", e),
        }

        match app_state.core.process_pending_embeddings(on_event.clone()) {
            Ok(count) if count > 0 => println!("  Processing {} pending embeddings in background", count),
            Ok(_) => {}
            Err(e) => eprintln!("  Warning: failed to start pending embeddings: {}", e),
        }

        match app_state.core.process_pending_tagging(on_event) {
            Ok(count) if count > 0 => println!("  Processing {} pending tagging operations in background", count),
            Ok(_) => {}
            Err(e) => eprintln!("  Warning: failed to start pending tagging: {}", e),
        }
    }

    let bind_owned = bind.to_string();

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
    .bind((bind_owned.as_str(), port))?
    .run()
    .await
}
