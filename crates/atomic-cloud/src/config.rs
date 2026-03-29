//! Server configuration and CLI

use clap::Parser;

/// Management plane for Atomic managed hosting
#[derive(Parser, Debug)]
#[command(name = "atomic-cloud", about = "Atomic managed hosting control plane")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Command {
    /// Start the HTTP server (default if no subcommand given)
    Serve {
        /// Port to listen on
        #[arg(long, default_value_t = 8080, env = "PORT")]
        port: u16,

        /// Address to bind to
        #[arg(long, default_value = "0.0.0.0", env = "BIND")]
        bind: String,

        /// PostgreSQL connection string
        #[arg(long, env = "DATABASE_URL")]
        database_url: String,

        /// Stripe secret key
        #[arg(long, env = "STRIPE_SECRET_KEY")]
        stripe_secret_key: String,

        /// Stripe webhook signing secret
        #[arg(long, env = "STRIPE_WEBHOOK_SECRET")]
        stripe_webhook_secret: String,

        /// Stripe price ID for the managed hosting plan
        #[arg(long, env = "STRIPE_PRICE_ID")]
        stripe_price_id: String,

        /// Fly.io API token
        #[arg(long, env = "FLY_API_TOKEN")]
        fly_api_token: String,

        /// Fly.io app name for managed instances
        #[arg(long, default_value = "atomic-managed", env = "FLY_APP_NAME")]
        fly_app_name: String,

        /// Fly.io region for new machines
        #[arg(long, default_value = "iad", env = "FLY_REGION")]
        fly_region: String,

        /// Docker image for customer instances
        #[arg(long, default_value = "registry.fly.io/atomic:latest", env = "ATOMIC_IMAGE")]
        atomic_image: String,

        /// Base domain for customer subdomains
        #[arg(long, default_value = "atomic.so", env = "BASE_DOMAIN")]
        base_domain: String,

        /// Admin API key for management routes
        #[arg(long, env = "ADMIN_API_KEY")]
        admin_api_key: String,

        /// Public URL of this management plane (for Stripe redirect URLs)
        #[arg(long, env = "PUBLIC_URL")]
        public_url: String,

        /// Path to the frontend build directory
        #[arg(long, default_value = "./frontend/dist", env = "FRONTEND_DIR")]
        frontend_dir: String,
    },
}
