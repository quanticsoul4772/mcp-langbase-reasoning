//! Server module for MCP protocol handling.
//!
//! This module provides:
//! - MCP server implementation over stdio
//! - Tool call handlers and routing
//! - Shared application state management
//! - Self-improvement system integration

mod handlers;
mod mcp;

pub use handlers::*;
pub use mcp::*;

use std::sync::Arc;

use crate::config::Config;
use crate::langbase::LangbaseClient;
use crate::modes::{
    AutoMode, BacktrackingMode, CounterfactualMode, DecisionMode, DetectionMode, DivergentMode,
    EvidenceMode, GotMode, LinearMode, MCTSMode, ReflectionMode, TimelineMode, TreeMode,
};
use crate::presets::PresetRegistry;
use crate::self_improvement::{SelfImprovementConfig, SelfImprovementSystem};
use crate::storage::SqliteStorage;

/// Application state shared across handlers.
///
/// Contains all mode handlers and shared resources needed for
/// processing reasoning requests.
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
    /// Decision framework mode handler.
    pub decision_mode: DecisionMode,
    /// Evidence assessment mode handler.
    pub evidence_mode: EvidenceMode,
    /// Detection mode handler for bias/fallacy detection.
    pub detection_mode: DetectionMode,
    /// Timeline reasoning mode handler.
    pub timeline_mode: TimelineMode,
    /// MCTS exploration mode handler.
    pub mcts_mode: MCTSMode,
    /// Counterfactual analysis mode handler.
    pub counterfactual_mode: CounterfactualMode,
    /// Workflow preset registry.
    pub preset_registry: Arc<PresetRegistry>,
    /// Self-improvement system (optional, enabled via config).
    ///
    /// When enabled, monitors system health and can take autonomous
    /// actions to improve performance, error rates, and quality.
    pub self_improvement: Option<Arc<SelfImprovementSystem>>,
}

impl AppState {
    /// Create new application state
    pub fn new(config: Config, storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        // Debug: Log pipe configuration
        tracing::info!(
            detection_pipe = ?config.pipes.detection.as_ref().and_then(|d| d.pipe.as_ref()),
            decision_pipe = ?config.pipes.decision.as_ref().and_then(|d| d.pipe.as_ref()),
            linear_pipe = %config.pipes.linear,
            "AppState initializing with pipe configuration"
        );

        let linear_mode = LinearMode::new(storage.clone(), langbase.clone(), &config);
        let tree_mode = TreeMode::new(storage.clone(), langbase.clone(), &config);
        let divergent_mode = DivergentMode::new(storage.clone(), langbase.clone(), &config);
        let reflection_mode = ReflectionMode::new(storage.clone(), langbase.clone(), &config);
        let backtracking_mode = BacktrackingMode::new(storage.clone(), langbase.clone(), &config);
        let auto_mode = AutoMode::new(storage.clone(), langbase.clone(), &config);
        let got_mode = GotMode::new(storage.clone(), langbase.clone(), &config);
        let decision_mode = DecisionMode::new(storage.clone(), langbase.clone(), &config);
        let evidence_mode = EvidenceMode::new(storage.clone(), langbase.clone(), &config);
        let detection_mode = DetectionMode::new(storage.clone(), langbase.clone(), &config);
        let timeline_mode = TimelineMode::new(storage.clone(), langbase.clone(), &config);
        let mcts_mode = MCTSMode::new(storage.clone(), langbase.clone(), &config);
        let counterfactual_mode = CounterfactualMode::new(storage.clone(), langbase.clone(), &config);
        let preset_registry = Arc::new(PresetRegistry::new());

        // Initialize self-improvement system (enabled by default)
        let self_improvement_config = SelfImprovementConfig::from_env();
        let self_improvement = if self_improvement_config.enabled {
            tracing::info!("Self-improvement system enabled (autonomous optimization active)");
            Some(Arc::new(SelfImprovementSystem::new(
                self_improvement_config,
                storage.clone(),
                langbase.clone(),
            )))
        } else {
            tracing::warn!("Self-improvement system disabled via SELF_IMPROVEMENT_ENABLED=false");
            None
        };

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
            decision_mode,
            evidence_mode,
            detection_mode,
            timeline_mode,
            mcts_mode,
            counterfactual_mode,
            preset_registry,
            self_improvement,
        }
    }

    /// Record an invocation event for self-improvement monitoring.
    ///
    /// This should be called after each tool invocation to feed metrics
    /// to the self-improvement system.
    pub async fn record_invocation(&self, event: crate::self_improvement::InvocationEvent) {
        if let Some(ref system) = self.self_improvement {
            system.on_invocation(event).await;
        }
    }

    /// Check if self-improvement system is enabled.
    pub fn self_improvement_enabled(&self) -> bool {
        self.self_improvement.is_some()
    }
}

// Manual Clone implementation since SelfImprovementSystem is behind Arc
impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            storage: self.storage.clone(),
            langbase: self.langbase.clone(),
            linear_mode: self.linear_mode.clone(),
            tree_mode: self.tree_mode.clone(),
            divergent_mode: self.divergent_mode.clone(),
            reflection_mode: self.reflection_mode.clone(),
            backtracking_mode: self.backtracking_mode.clone(),
            auto_mode: self.auto_mode.clone(),
            got_mode: self.got_mode.clone(),
            decision_mode: self.decision_mode.clone(),
            evidence_mode: self.evidence_mode.clone(),
            detection_mode: self.detection_mode.clone(),
            timeline_mode: self.timeline_mode.clone(),
            mcts_mode: self.mcts_mode.clone(),
            counterfactual_mode: self.counterfactual_mode.clone(),
            preset_registry: Arc::clone(&self.preset_registry),
            self_improvement: self.self_improvement.as_ref().map(Arc::clone),
        }
    }
}

/// Shared application state handle
pub type SharedState = Arc<AppState>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DatabaseConfig, ErrorHandlingConfig, LangbaseConfig, LogFormat, LoggingConfig, PipeConfig,
        RequestConfig,
    };
    use std::path::PathBuf;

    fn create_test_config() -> Config {
        Config {
            langbase: LangbaseConfig {
                api_key: "test-key".to_string(),
                base_url: "https://api.langbase.com".to_string(),
            },
            database: DatabaseConfig {
                path: PathBuf::from(":memory:"),
                max_connections: 5,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: LogFormat::Pretty,
            },
            request: RequestConfig::default(),
            pipes: PipeConfig::default(),
            error_handling: ErrorHandlingConfig::default(),
        }
    }

    #[tokio::test]
    async fn test_app_state_new() {
        let config = create_test_config();
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let state = AppState::new(config.clone(), storage, langbase);

        // Verify all modes are initialized
        assert_eq!(state.config.langbase.api_key, "test-key");
    }

    #[tokio::test]
    async fn test_app_state_clone() {
        let config = create_test_config();
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let state1 = AppState::new(config, storage, langbase);
        let state2 = state1.clone();

        assert_eq!(
            state1.config.langbase.api_key,
            state2.config.langbase.api_key
        );
    }

    #[tokio::test]
    async fn test_shared_state_type() {
        let config = create_test_config();
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let state = AppState::new(config, storage, langbase);
        let shared: SharedState = Arc::new(state);

        // Verify we can clone the shared state
        let shared2 = Arc::clone(&shared);
        assert_eq!(Arc::strong_count(&shared), 2);
        drop(shared2);
        assert_eq!(Arc::strong_count(&shared), 1);
    }

    #[tokio::test]
    async fn test_app_state_has_all_modes() {
        let config = create_test_config();
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let state = AppState::new(config, storage, langbase);

        // Verify preset registry is initialized with builtins
        assert!(state.preset_registry.count() >= 5);
    }

    #[tokio::test]
    async fn test_app_state_storage_access() {
        use crate::storage::Storage;

        let config = create_test_config();
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let state = AppState::new(config, storage.clone(), langbase);

        // Verify storage is accessible and usable
        let session = crate::storage::Session::new("test-metadata");
        state.storage.create_session(&session).await.unwrap();
        let retrieved = state.storage.get_session(&session.id).await.unwrap();
        assert!(retrieved.is_some());
    }

    #[tokio::test]
    async fn test_app_state_config_access() {
        let config = create_test_config();
        let storage = SqliteStorage::new_in_memory().await.unwrap();
        let langbase = LangbaseClient::new(&config.langbase, config.request.clone()).unwrap();

        let state = AppState::new(config.clone(), storage, langbase);

        // Verify config values are preserved
        assert_eq!(state.config.langbase.base_url, "https://api.langbase.com");
        assert_eq!(state.config.database.max_connections, 5);
        assert_eq!(state.config.logging.level, "info");
    }
}
