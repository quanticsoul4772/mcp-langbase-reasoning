use std::sync::Arc;

use clap::{Parser, Subcommand};
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use mcp_langbase_reasoning::{
    config::Config,
    langbase::LangbaseClient,
    self_improvement::{execute_command, SelfImproveCommands},
    server::{AppState, McpServer},
    storage::{MetricsFilter, SqliteStorage, Storage},
};

/// MCP Langbase Reasoning Server
#[derive(Parser)]
#[command(name = "mcp-langbase-reasoning")]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Query pipe usage metrics
    Metrics {
        #[command(subcommand)]
        action: MetricsAction,
    },
    /// Self-improvement system commands
    SelfImprove {
        #[command(subcommand)]
        action: SelfImproveCommands,
    },
}

#[derive(Subcommand)]
enum MetricsAction {
    /// Show usage summary for all pipes
    Summary,
    /// Show metrics for a specific pipe
    Pipe {
        /// Name of the pipe to query
        name: String,
    },
    /// List recent invocations
    Invocations {
        /// Filter by pipe name
        #[arg(short, long)]
        pipe: Option<String>,
        /// Filter by session ID
        #[arg(short, long)]
        session: Option<String>,
        /// Maximum number of results
        #[arg(short, long, default_value = "20")]
        limit: u32,
        /// Show only successful invocations
        #[arg(long)]
        success_only: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            std::process::exit(1);
        }
    };

    match cli.command {
        Some(Commands::Metrics { action }) => {
            // Metrics commands don't need full server initialization
            run_metrics_command(&config, action).await
        }
        Some(Commands::SelfImprove { action }) => {
            // Self-improvement commands
            run_self_improve_command(&config, action).await
        }
        None => {
            // Default: run the MCP server
            run_server(config).await
        }
    }
}

/// Run metrics CLI commands
async fn run_metrics_command(config: &Config, action: MetricsAction) -> anyhow::Result<()> {
    // Initialize storage only (no langbase client needed for metrics)
    let storage = match SqliteStorage::new(&config.database).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    match action {
        MetricsAction::Summary => {
            let summaries = storage.get_pipe_usage_summary().await?;
            if summaries.is_empty() {
                println!("No pipe usage data found.");
                return Ok(());
            }

            println!("\n{:=<80}", "");
            println!("PIPE USAGE SUMMARY");
            println!("{:=<80}\n", "");

            for summary in summaries {
                println!("📊 Pipe: {}", summary.pipe_name);
                println!("   Total Calls:    {}", summary.total_calls);
                println!(
                    "   Success Rate:   {:.1}% ({} success / {} failed)",
                    summary.success_rate * 100.0,
                    summary.success_count,
                    summary.failure_count
                );
                println!("   Avg Latency:    {:.2}ms", summary.avg_latency_ms);
                if let (Some(min), Some(max)) = (summary.min_latency_ms, summary.max_latency_ms) {
                    println!("   Latency Range:  {}ms - {}ms", min, max);
                }
                println!(
                    "   First Call:     {}",
                    summary.first_call.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!(
                    "   Last Call:      {}",
                    summary.last_call.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!();
            }
        }

        MetricsAction::Pipe { name } => match storage.get_pipe_summary(&name).await? {
            Some(summary) => {
                println!("\n{:=<80}", "");
                println!("METRICS FOR PIPE: {}", summary.pipe_name);
                println!("{:=<80}\n", "");

                println!("Total Calls:    {}", summary.total_calls);
                println!(
                    "Success Rate:   {:.1}% ({} success / {} failed)",
                    summary.success_rate * 100.0,
                    summary.success_count,
                    summary.failure_count
                );
                println!("Avg Latency:    {:.2}ms", summary.avg_latency_ms);
                if let (Some(min), Some(max)) = (summary.min_latency_ms, summary.max_latency_ms) {
                    println!("Latency Range:  {}ms - {}ms", min, max);
                }
                println!(
                    "First Call:     {}",
                    summary.first_call.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!(
                    "Last Call:      {}",
                    summary.last_call.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!();
            }
            None => {
                println!("No data found for pipe: {}", name);
            }
        },

        MetricsAction::Invocations {
            pipe,
            session,
            limit,
            success_only,
        } => {
            let filter = MetricsFilter {
                pipe_name: pipe.clone(),
                session_id: session,
                limit: Some(limit),
                success_only: if success_only { Some(true) } else { None },
                ..Default::default()
            };

            let invocations = storage.get_invocations(filter).await?;

            if invocations.is_empty() {
                println!("No invocations found matching the criteria.");
                return Ok(());
            }

            println!("\n{:=<80}", "");
            println!(
                "RECENT INVOCATIONS{}",
                pipe.map(|p| format!(" (pipe: {})", p)).unwrap_or_default()
            );
            println!("{:=<80}\n", "");

            for inv in invocations {
                let status = if inv.success { "✓" } else { "✗" };
                let latency = inv
                    .latency_ms
                    .map(|l| format!("{}ms", l))
                    .unwrap_or_else(|| "-".to_string());
                let pipe_name = inv.pipe_name.as_deref().unwrap_or("-");
                let session_id = inv.session_id.as_deref().unwrap_or("-");

                println!(
                    "{} {} | {} | {} | session: {}",
                    status,
                    inv.created_at.format("%Y-%m-%d %H:%M:%S"),
                    pipe_name,
                    latency,
                    session_id
                );

                println!("    Tool: {}", inv.tool_name);

                if !inv.success {
                    if let Some(err) = &inv.error {
                        println!("    Error: {}", err);
                    }
                }
            }
            println!();
        }
    }

    Ok(())
}

/// Run self-improvement CLI commands
async fn run_self_improve_command(
    config: &Config,
    action: SelfImproveCommands,
) -> anyhow::Result<()> {
    // Initialize storage only (no langbase client needed for CLI commands)
    let storage = match SqliteStorage::new(&config.database).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    let result = execute_command(action, &storage).await;

    // Print the result message
    print!("{}", result.message);

    if result.exit_code != 0 {
        std::process::exit(result.exit_code);
    }

    Ok(())
}

/// Run the MCP server (default behavior)
async fn run_server(config: Config) -> anyhow::Result<()> {
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

    // Ensure all required pipes exist (create if needed via upsert)
    info!("Ensuring all required Langbase pipes exist...");
    if let Err(e) = langbase.ensure_all_pipes().await {
        error!(error = %e, "Failed to ensure pipes exist");
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
