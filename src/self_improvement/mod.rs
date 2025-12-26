//! Self-Improvement System for the MCP Reasoning Server.
//!
//! This module implements an autonomous self-improvement loop that monitors
//! system health, diagnoses issues, executes safe changes, and learns from
//! outcomes.
//!
//! # Architecture
//!
//! The system follows a four-phase loop:
//!
//! ```text
//! ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
//! │ MONITOR  │───▶│ ANALYZER │───▶│ EXECUTOR │───▶│ LEARNER  │
//! │ Phase 1  │    │ Phase 2  │    │ Phase 3  │    │ Phase 4  │
//! └──────────┘    └──────────┘    └──────────┘    └──────────┘
//!      │                                               │
//!      └───────────────────◀───────────────────────────┘
//! ```
//!
//! ## Phase 1: Monitor
//! - Collects system metrics (error rate, latency, quality scores)
//! - Maintains baselines using hybrid EMA + rolling average
//! - Detects anomalies that exceed thresholds
//!
//! ## Phase 2: Analyzer
//! - Uses Langbase pipes to diagnose root causes
//! - Generates action recommendations
//! - Validates decisions for biases/fallacies
//!
//! ## Phase 3: Executor
//! - Validates actions against allowlist
//! - Executes changes safely
//! - Monitors for regressions
//! - Rolls back if necessary
//!
//! ## Phase 4: Learner
//! - Calculates normalized rewards
//! - Tracks action effectiveness
//! - Synthesizes lessons learned
//!
//! # Safety Features
//!
//! - **Circuit Breaker**: Stops self-improvement after consecutive failures
//! - **Action Allowlist**: Only bounded, safe actions can be executed
//! - **Rollback**: Automatic rollback on regression
//! - **Cooldown**: Minimum time between actions
//! - **Validation**: AI-assisted bias/fallacy detection
//!
//! # Usage
//!
//! ```rust,ignore
//! use mcp_langbase_reasoning::self_improvement::{
//!     SelfImprovementConfig,
//!     BaselineCalculator,
//!     CircuitBreaker,
//!     ActionAllowlist,
//! };
//!
//! // Create configuration from environment
//! let config = SelfImprovementConfig::from_env();
//!
//! // Initialize components
//! let calculator = BaselineCalculator::new(config.baseline.clone());
//! let circuit_breaker = CircuitBreaker::new(config.circuit_breaker.clone());
//! let allowlist = ActionAllowlist::default_allowlist();
//! ```

pub mod allowlist;
pub mod analyzer;
pub mod baseline;
pub mod circuit_breaker;
pub mod config;
pub mod executor;
pub mod learner;
pub mod monitor;
pub mod pipes;
pub mod storage;
pub mod system;
pub mod types;

// Re-export main types for convenience
pub use allowlist::{ActionAllowlist, AllowlistError, ParamBounds, ResourceBounds};
pub use analyzer::{AnalysisBlocked, AnalysisResult, Analyzer, AnalyzerStats};
pub use baseline::{BaselineCalculator, BaselineCollection, MetricBaseline, TriggerLevel};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerSummary, CircuitState};
pub use config::{
    AnalyzerConfig, BaselineConfig, CircuitBreakerConfig, ExecutorConfig, LearnerConfig,
    MonitorConfig, SelfImprovementConfig, SelfImprovementPipeConfig,
};
pub use executor::{ConfigState, ExecutionBlocked, ExecutionResult, Executor, ExecutorStats};
pub use learner::{Learner, LearnerStats, LearningBlocked, LearningOutcome};
pub use monitor::{AggregatedMetrics, Monitor, MonitorStats, RawMetrics};
pub use pipes::{
    ActionEffectiveness, ActionSelectionResponse, DiagnosisResponse, LearningResponse,
    PipeCallMetrics, PipeError, SelfImprovementPipes, ValidationResponse,
};
pub use storage::{ActionEffectivenessRecord, ActionRecord, SelfImprovementStorage};
pub use system::{CycleResult, InvocationEvent, SelfImprovementError, SelfImprovementSystem, SystemStatus};
pub use types::{
    ActionId, ActionOutcome, Baselines, ConfigScope, DiagnosisId, DiagnosisStatus, HealthReport,
    MetricsSnapshot, NormalizedReward, ParamValue, ResourceType, RewardBreakdown, RewardWeights,
    SelfDiagnosis, ServiceComponent, Severity, SuggestedAction, TriggerMetric,
};
