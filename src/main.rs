use std::sync::Arc;

use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use mcp_langbase_reasoning::{
    config::Config,
    langbase::LangbaseClient,
    server::{AppState, McpServer},
    storage::SqliteStorage,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging
    init_logging(&config);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "MCP Langbase Reasoning Server starting..."
    );

    // Initialize storage
    let storage = match SqliteStorage::new(&config.database).await {
        Ok(s) => {
            info!(path = %config.database.path.display(), "Database initialized");
            s
        }
        Err(e) => {
            error!(error = %e, "Failed to initialize database");
            return Err(e.into());
        }
    };

    // Initialize Langbase client
    let langbase = match LangbaseClient::new(&config.langbase, config.request.clone()) {
        Ok(c) => {
            info!(base_url = %config.langbase.base_url, "Langbase client initialized");
            c
        }
        Err(e) => {
            error!(error = %e, "Failed to initialize Langbase client");
            return Err(e.into());
        }
    };

    // Ensure required pipes exist (create if needed)
    info!("Ensuring required Langbase pipes exist...");
    if let Err(e) = langbase.ensure_linear_pipe(&config.pipes.linear).await {
        error!(error = %e, "Failed to ensure linear pipe exists");
        return Err(e.into());
    }

    // Create application state
    let state = Arc::new(AppState::new(config, storage, langbase));

    // Start MCP server
    let server = McpServer::new(state);

    info!("Server ready, waiting for requests on stdin...");

    if let Err(e) = server.run().await {
        error!(error = %e, "Server error");
        return Err(e.into());
    }

    info!("Server shutdown complete");
    Ok(())
}

/// Initialize tracing/logging
fn init_logging(config: &Config) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.logging.level));

    match config.logging.format {
        mcp_langbase_reasoning::config::LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json().with_writer(std::io::stderr))
                .init();
        }
        mcp_langbase_reasoning::config::LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().with_writer(std::io::stderr))
                .init();
        }
    }
}
