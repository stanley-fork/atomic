//! Server configuration and CLI

use clap::{Parser, Subcommand};

/// Standalone HTTP server for the Atomic knowledge base
#[derive(Parser, Debug)]
#[command(name = "atomic-server", about = "Atomic knowledge base HTTP server")]
pub struct Cli {
    /// Path to the SQLite database file
    #[arg(long, default_value = "atomic.db", global = true)]
    pub db_path: String,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the HTTP server (default if no subcommand given)
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// Address to bind to
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,

        /// Public URL for OAuth discovery (e.g. https://atomic.example.com).
        /// Required for OAuth/MCP remote auth. Without this, OAuth endpoints return 404.
        /// Can also be set via PUBLIC_URL env var.
        #[arg(long, env = "PUBLIC_URL")]
        public_url: Option<String>,
    },

    /// Manage API tokens
    Token {
        #[command(subcommand)]
        action: TokenAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum TokenAction {
    /// Create a new API token
    Create {
        /// Human-readable name for the token
        #[arg(long)]
        name: String,
    },

    /// List all API tokens
    List,

    /// Revoke an API token by ID
    Revoke {
        /// Token ID to revoke
        id: String,
    },
}
