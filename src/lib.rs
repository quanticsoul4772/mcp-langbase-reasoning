//! # MCP Langbase Reasoning Server
//!
//! A Model Context Protocol (MCP) server that provides structured reasoning capabilities
//! by delegating to Langbase Pipes for LLM-powered cognitive processing.
//!
//! ## Features
//!
//! - **Linear Reasoning**: Sequential step-by-step thought processing
//! - **Tree Reasoning**: Branching exploration with multiple reasoning paths
//! - **Divergent Reasoning**: Creative exploration with multiple perspectives
//! - **Reflection**: Meta-cognitive analysis and quality improvement
//! - **Backtracking**: Checkpoint-based state restoration for exploration
//! - **Auto Routing**: Intelligent mode selection based on content analysis
//! - **Graph-of-Thoughts (GoT)**: Advanced graph-based reasoning with scoring and pruning
//! - **Bias & Fallacy Detection**: Cognitive bias and logical fallacy identification
//! - **Workflow Presets**: Composable multi-step reasoning workflows
//!
//! ## Architecture
//!
//! ```text
//! MCP Client → MCP Server (Rust) → Langbase Pipes (HTTP)
//!                    ↓
//!              SQLite (State)
//! ```
//!
//! ## Example
//!
//! ```ignore
//! use std::sync::Arc;
//! use mcp_langbase_reasoning::{Config, AppState, McpServer};
//! use mcp_langbase_reasoning::langbase::LangbaseClient;
//! use mcp_langbase_reasoning::storage::SqliteStorage;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = Config::from_env()?;
//!     let storage = SqliteStorage::new(&config.database_path).await?;
//!     let langbase = LangbaseClient::new(&config)?;
//!     let state = Arc::new(AppState::new(config, storage, langbase));
//!     let server = McpServer::new(state);
//!     server.run().await?;
//!     Ok(())
//! }
//! ```

#![warn(missing_docs)]

/// Configuration management for the MCP server.
pub mod config;
/// Error types and result aliases for the application.
pub mod error;
/// Langbase API client and types for pipe communication.
pub mod langbase;
/// Reasoning mode implementations (linear, tree, divergent, etc.).
pub mod modes;
/// Workflow preset system for composable reasoning workflows.
pub mod presets;
/// System prompts for Langbase pipes.
pub mod prompts;
/// MCP server implementation and request handling.
pub mod server;
/// SQLite storage layer for persistence.
pub mod storage;

pub use config::Config;
pub use error::{AppError, AppResult};
pub use server::{AppState, McpServer, SharedState};
