//! Server module for MCP protocol handling.
//!
//! This module provides:
//! - MCP server implementation over stdio
//! - Tool call handlers and routing
//! - Shared application state management

mod handlers;
mod mcp;

pub use handlers::*;
pub use mcp::*;

use std::sync::Arc;

use crate::config::Config;
use crate::langbase::LangbaseClient;
use crate::modes::{
    AutoMode, BacktrackingMode, DivergentMode, GotMode, LinearMode, ReflectionMode, TreeMode,
};
use crate::presets::PresetRegistry;
use crate::storage::SqliteStorage;

/// Application state shared across handlers.
///
/// Contains all mode handlers and shared resources needed for
/// processing reasoning requests.
#[derive(Clone)]
pub struct AppState {
    /// Application configuration.
    pub config: Config,
    /// SQLite storage backend.
    pub storage: SqliteStorage,
    /// Langbase API client.
    pub langbase: LangbaseClient,
    /// Linear reasoning mode handler.
    pub linear_mode: LinearMode,
    /// Tree reasoning mode handler.
    pub tree_mode: TreeMode,
    /// Divergent reasoning mode handler.
    pub divergent_mode: DivergentMode,
    /// Reflection reasoning mode handler.
    pub reflection_mode: ReflectionMode,
    /// Backtracking mode handler.
    pub backtracking_mode: BacktrackingMode,
    /// Auto mode router handler.
    pub auto_mode: AutoMode,
    /// Graph-of-Thoughts mode handler.
    pub got_mode: GotMode,
    /// Workflow preset registry.
    pub preset_registry: Arc<PresetRegistry>,
}

impl AppState {
    /// Create new application state
    pub fn new(config: Config, storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        let linear_mode = LinearMode::new(storage.clone(), langbase.clone(), &config);
        let tree_mode = TreeMode::new(storage.clone(), langbase.clone(), &config);
        let divergent_mode = DivergentMode::new(storage.clone(), langbase.clone(), &config);
        let reflection_mode = ReflectionMode::new(storage.clone(), langbase.clone(), &config);
        let backtracking_mode = BacktrackingMode::new(storage.clone(), langbase.clone(), &config);
        let auto_mode = AutoMode::new(storage.clone(), langbase.clone(), &config);
        let got_mode = GotMode::new(storage.clone(), langbase.clone(), &config);
        let preset_registry = Arc::new(PresetRegistry::new());

        Self {
            config,
            storage,
            langbase,
            linear_mode,
            tree_mode,
            divergent_mode,
            reflection_mode,
            backtracking_mode,
            auto_mode,
            got_mode,
            preset_registry,
        }
    }
}

/// Shared application state handle
pub type SharedState = Arc<AppState>;
