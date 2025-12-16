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
use crate::storage::SqliteStorage;

/// Application state shared across handlers
pub struct AppState {
    pub config: Config,
    pub storage: SqliteStorage,
    pub langbase: LangbaseClient,
    pub linear_mode: LinearMode,
    pub tree_mode: TreeMode,
    pub divergent_mode: DivergentMode,
    pub reflection_mode: ReflectionMode,
    pub backtracking_mode: BacktrackingMode,
    pub auto_mode: AutoMode,
    pub got_mode: GotMode,
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
        }
    }
}

/// Shared application state handle
pub type SharedState = Arc<AppState>;
