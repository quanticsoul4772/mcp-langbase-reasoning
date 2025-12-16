mod handlers;
mod mcp;

pub use handlers::*;
pub use mcp::*;

use std::sync::Arc;

use crate::config::Config;
use crate::langbase::LangbaseClient;
use crate::modes::LinearMode;
use crate::storage::SqliteStorage;

/// Application state shared across handlers
pub struct AppState {
    pub config: Config,
    pub storage: SqliteStorage,
    pub langbase: LangbaseClient,
    pub linear_mode: LinearMode,
}

impl AppState {
    /// Create new application state
    pub fn new(config: Config, storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        let linear_mode = LinearMode::new(storage.clone(), langbase.clone(), &config);

        Self {
            config,
            storage,
            langbase,
            linear_mode,
        }
    }
}

/// Shared application state handle
pub type SharedState = Arc<AppState>;
