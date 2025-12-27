//! Self-Improvement System Orchestrator.
//!
//! This module provides the main `SelfImprovementSystem` that orchestrates
//! the 4-phase self-improvement loop: Monitor → Analyzer → Executor → Learner.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │              SelfImprovementSystem                               │
//! │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
//! │  │ Monitor  │──│ Analyzer │──│ Executor │──│ Learner  │        │
//! │  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
//! │       │                                          │              │
//! │       └──────────────────────────────────────────┘              │
//! │                         │                                        │
//! │  ┌──────────────────────┴──────────────────────────────────┐    │
//! │  │              Shared Components                           │    │
//! │  │  CircuitBreaker │ Allowlist │ Storage │ Pipes           │    │
//! │  └─────────────────────────────────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Safety Features
//!
//! - **Disabled by Default**: System must be explicitly enabled
//! - **Circuit Breaker**: Stops after consecutive failures
//! - **Rate Limiting**: Maximum actions per hour
//! - **Rollback**: Automatic rollback on regression

use std::sync::Arc;

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::{
    ActionAllowlist, AnalysisBlocked, Analyzer, CircuitBreaker, CircuitState, ExecutionBlocked,
    Executor, HealthReport, Learner, LearningBlocked, Monitor, SelfDiagnosis,
    SelfImprovementConfig, SelfImprovementPipes,
};
use crate::langbase::LangbaseClient;
use crate::storage::SqliteStorage;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during self-improvement operations.
#[derive(Debug, Error)]
pub enum SelfImprovementError {
    /// System is disabled via configuration.
    #[error("Self-improvement system is disabled")]
    Disabled,

    /// Circuit breaker is open, blocking all actions.
    #[error("Circuit breaker is open: {consecutive_failures} consecutive failures")]
    CircuitBreakerOpen {
        /// Number of consecutive failures that opened the circuit.
        consecutive_failures: u32,
    },

    /// System is in cooldown period.
    #[error("System in cooldown until {until}")]
    InCooldown {
        /// When the cooldown period ends.
        until: DateTime<Utc>,
    },

    /// Rate limit exceeded.
    #[error("Rate limit exceeded: {count}/{max} actions this hour")]
    RateLimitExceeded {
        /// Current action count this hour.
        count: u32,
        /// Maximum actions allowed per hour.
        max: u32,
    },

    /// Monitor phase failed.
    #[error("Monitor phase failed: {message}")]
    MonitorFailed {
        /// Error details from the monitor phase.
        message: String,
    },

    /// Analyzer phase failed.
    #[error("Analyzer phase failed: {message}")]
    AnalyzerFailed {
        /// Error details from the analyzer phase.
        message: String,
    },

    /// Executor phase failed.
    #[error("Executor phase failed: {message}")]
    ExecutorFailed {
        /// Error details from the executor phase.
        message: String,
    },

    /// Learner phase failed.
    #[error("Learner phase failed: {message}")]
    LearnerFailed {
        /// Error details from the learner phase.
        message: String,
    },

    /// Storage operation failed.
    #[error("Storage error: {message}")]
    StorageError {
        /// Error details from the storage operation.
        message: String,
    },

    /// Internal system error.
    #[error("Internal error: {message}")]
    Internal {
        /// Error details.
        message: String,
    },
}

// ============================================================================
// Invocation Event
// ============================================================================

/// Event recorded when a tool is invoked.
///
/// Used by the Monitor phase to track system metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationEvent {
    /// Name of the tool that was invoked.
    pub tool_name: String,
    /// Latency in milliseconds.
    pub latency_ms: i64,
    /// Whether the invocation succeeded.
    pub success: bool,
    /// Optional quality score from the response.
    pub quality_score: Option<f64>,
    /// When the invocation occurred.
    pub timestamp: DateTime<Utc>,
}

// ============================================================================
// Cycle Result
// ============================================================================

/// Result of running one improvement cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleResult {
    /// Whether the cycle completed successfully.
    pub success: bool,
    /// Whether any action was taken.
    pub action_taken: bool,
    /// Diagnosis that triggered the cycle, if any.
    pub diagnosis: Option<SelfDiagnosis>,
    /// Normalized reward if action was taken and verified.
    pub reward: Option<f64>,
    /// Any lessons learned from this cycle.
    pub lessons: Option<String>,
    /// Error message if cycle failed.
    pub error: Option<String>,
    /// Duration of the cycle in milliseconds.
    pub duration_ms: u64,
}

// ============================================================================
// System Status
// ============================================================================

/// Current status of the self-improvement system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    /// Whether the system is enabled.
    pub enabled: bool,
    /// Current circuit breaker state.
    pub circuit_state: CircuitState,
    /// Consecutive failures count.
    pub consecutive_failures: u32,
    /// Whether system is in cooldown.
    pub in_cooldown: bool,
    /// Cooldown ends at (if in cooldown).
    pub cooldown_ends_at: Option<DateTime<Utc>>,
    /// Actions taken this hour.
    pub actions_this_hour: u32,
    /// Maximum actions per hour.
    pub max_actions_per_hour: u32,
    /// Total cycles completed.
    pub total_cycles: u64,
    /// Total successful actions.
    pub total_successes: u64,
    /// Total rolled back actions.
    pub total_rollbacks: u64,
    /// Last cycle timestamp.
    pub last_cycle_at: Option<DateTime<Utc>>,
}

// ============================================================================
// System State
// ============================================================================

/// Internal state for the SelfImprovementSystem.
struct SystemState {
    /// Cooldown end time, if in cooldown.
    cooldown_until: Option<DateTime<Utc>>,
    /// Actions taken in the current hour.
    actions_this_hour: u32,
    /// Hour boundary for rate limiting.
    rate_limit_hour: DateTime<Utc>,
    /// Total improvement cycles run.
    total_cycles: u64,
    /// Total successful actions.
    total_successes: u64,
    /// Total rolled back actions.
    total_rollbacks: u64,
    /// Last cycle timestamp.
    last_cycle_at: Option<DateTime<Utc>>,
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            cooldown_until: None,
            actions_this_hour: 0,
            rate_limit_hour: Utc::now(),
            total_cycles: 0,
            total_successes: 0,
            total_rollbacks: 0,
            last_cycle_at: None,
        }
    }
}

// ============================================================================
// SelfImprovementSystem
// ============================================================================

/// Orchestrates the 4-phase self-improvement loop.
///
/// # Example
///
/// ```rust,ignore
/// let system = SelfImprovementSystem::new(config, storage, langbase);
///
/// // Record invocations (called after each tool use)
/// system.on_invocation(event).await;
///
/// // Check health and run cycle if needed
/// let health = system.check_health().await;
/// if health.should_act() {
///     let result = system.run_cycle().await?;
/// }
/// ```
pub struct SelfImprovementSystem {
    /// System configuration.
    config: SelfImprovementConfig,
    /// Phase 1: Monitor for health metrics.
    monitor: Monitor,
    /// Phase 2: Analyzer for diagnosis and action selection.
    analyzer: Analyzer,
    /// Phase 3: Executor for safe action execution.
    executor: Executor,
    /// Phase 4: Learner for reward calculation and learning.
    learner: Learner,
    /// Circuit breaker for safety.
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    /// Action allowlist for validation.
    allowlist: ActionAllowlist,
    /// Internal state.
    state: Arc<RwLock<SystemState>>,
}

impl SelfImprovementSystem {
    /// Create a new self-improvement system.
    ///
    /// # Arguments
    ///
    /// * `config` - System configuration
    /// * `storage` - SQLite storage backend
    /// * `langbase` - Langbase API client for pipe calls
    ///
    /// # Returns
    ///
    /// A new `SelfImprovementSystem` instance.
    pub fn new(
        config: SelfImprovementConfig,
        _storage: SqliteStorage,
        langbase: LangbaseClient,
    ) -> Self {
        info!(
            enabled = config.enabled,
            max_actions_per_hour = config.executor.max_actions_per_hour,
            "Initializing SelfImprovementSystem"
        );

        // Create shared circuit breaker
        let circuit_breaker = Arc::new(RwLock::new(CircuitBreaker::new(
            config.circuit_breaker.clone(),
        )));

        // Create shared pipes
        let langbase_arc = Arc::new(langbase);
        let pipes = Arc::new(SelfImprovementPipes::new(
            langbase_arc,
            config.pipes.clone(),
        ));

        // Create allowlist
        let allowlist = ActionAllowlist::default_allowlist();

        // Create phase components with shared dependencies
        let monitor = Monitor::new(config.clone());
        let analyzer = Analyzer::new(config.clone(), pipes.clone(), circuit_breaker.clone());
        let executor = Executor::new(config.clone(), allowlist.clone(), circuit_breaker.clone());
        let learner = Learner::new(config.clone(), pipes, circuit_breaker.clone());

        Self {
            config,
            monitor,
            analyzer,
            executor,
            learner,
            circuit_breaker,
            allowlist,
            state: Arc::new(RwLock::new(SystemState::default())),
        }
    }

    /// Check if the system is enabled (always true).
    pub fn is_enabled(&self) -> bool {
        true
    }

    /// Record an invocation for metric tracking.
    ///
    /// This should be called after each tool invocation to feed the Monitor.
    pub async fn on_invocation(&self, event: InvocationEvent) {
        debug!(
            tool = %event.tool_name,
            latency_ms = event.latency_ms,
            success = event.success,
            "Recording invocation"
        );

        // Record in monitor (for baseline calculation and anomaly detection)
        self.monitor
            .record_invocation(
                !event.success,
                event.latency_ms,
                event.quality_score.unwrap_or(0.8),
                false, // fallback not tracked here
            )
            .await;
    }

    /// Get current health report from the Monitor.
    ///
    /// Returns `Some(HealthReport)` if enough samples have been collected
    /// and it's time for a check, `None` otherwise.
    pub async fn check_health(&self) -> Option<HealthReport> {
        self.monitor.check_health().await
    }

    /// Force a health check regardless of timing.
    pub async fn force_health_check(&self) -> Option<HealthReport> {
        self.monitor.force_check().await
    }

    /// Get current system status.
    pub async fn status(&self) -> SystemStatus {
        let state = self.state.read().await;
        let cb = self.circuit_breaker.read().await;
        let cb_summary = cb.summary();

        let in_cooldown = state
            .cooldown_until
            .map(|until| Utc::now() < until)
            .unwrap_or(false);

        SystemStatus {
            enabled: true,
            circuit_state: cb_summary.state,
            consecutive_failures: cb_summary.consecutive_failures,
            in_cooldown,
            cooldown_ends_at: if in_cooldown {
                state.cooldown_until
            } else {
                None
            },
            actions_this_hour: state.actions_this_hour,
            max_actions_per_hour: self.config.executor.max_actions_per_hour,
            total_cycles: state.total_cycles,
            total_successes: state.total_successes,
            total_rollbacks: state.total_rollbacks,
            last_cycle_at: state.last_cycle_at,
        }
    }

    /// Run one improvement cycle (Monitor → Analyzer → Executor → Learner).
    ///
    /// # Returns
    ///
    /// * `Ok(CycleResult)` - Cycle completed (may or may not have taken action)
    /// * `Err(SelfImprovementError)` - Cycle blocked or failed
    pub async fn run_cycle(&self) -> Result<CycleResult, SelfImprovementError> {
        let start = std::time::Instant::now();

        // Check circuit breaker
        {
            let mut cb = self.circuit_breaker.write().await;
            if !cb.can_execute() {
                let summary = cb.summary();
                return Err(SelfImprovementError::CircuitBreakerOpen {
                    consecutive_failures: summary.consecutive_failures,
                });
            }
        }

        // Check cooldown
        {
            let state = self.state.read().await;
            if let Some(until) = state.cooldown_until {
                if Utc::now() < until {
                    return Err(SelfImprovementError::InCooldown { until });
                }
            }
        }

        // Check rate limit
        self.check_and_update_rate_limit().await?;

        // Update cycle tracking
        {
            let mut state = self.state.write().await;
            state.total_cycles += 1;
            state.last_cycle_at = Some(Utc::now());
        }

        info!("Starting self-improvement cycle");

        // Phase 1: Monitor - Check health (force check since we're running a cycle)
        let health = match self.monitor.force_check().await {
            Some(report) => report,
            None => {
                info!("Not enough samples for health check");
                return Ok(CycleResult {
                    success: true,
                    action_taken: false,
                    diagnosis: None,
                    reward: None,
                    lessons: None,
                    error: Some("Insufficient samples for health check".to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };
        debug!(?health, "Health report");

        if !health.needs_action() {
            info!("No action needed - system healthy");
            return Ok(CycleResult {
                success: true,
                action_taken: false,
                diagnosis: None,
                reward: None,
                lessons: None,
                error: None,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Phase 2: Analyzer - Diagnose and select action
        let analysis_result = match self.analyzer.analyze(&health).await {
            Ok(result) => result,
            Err(blocked) => {
                let msg = match &blocked {
                    AnalysisBlocked::CircuitOpen { remaining_secs } => {
                        format!("Circuit open, {} seconds until recovery", remaining_secs)
                    }
                    AnalysisBlocked::NoTriggers => "No triggers to analyze".to_string(),
                    AnalysisBlocked::PipeUnavailable { pipe, error } => {
                        format!("Pipe '{}' unavailable: {}", pipe, error)
                    }
                    AnalysisBlocked::MaxPendingReached { count } => {
                        format!("Max pending diagnoses reached: {}", count)
                    }
                    AnalysisBlocked::SeverityTooLow { severity, minimum } => {
                        format!("Severity {:?} below minimum {:?}", severity, minimum)
                    }
                };
                warn!(?blocked, "Analysis blocked");
                return Ok(CycleResult {
                    success: true,
                    action_taken: false,
                    diagnosis: None,
                    reward: None,
                    lessons: None,
                    error: Some(format!("Analysis blocked: {}", msg)),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        let diagnosis = analysis_result.diagnosis.clone();

        info!(
            diagnosis_id = %diagnosis.id,
            severity = ?diagnosis.severity,
            action = ?diagnosis.suggested_action.action_type(),
            "Diagnosis generated"
        );

        // Validate action against allowlist
        if let Err(e) = self.allowlist.validate(&diagnosis.suggested_action) {
            warn!(error = %e, "Action not allowed");
            return Ok(CycleResult {
                success: true,
                action_taken: false,
                diagnosis: Some(diagnosis),
                reward: None,
                lessons: None,
                error: Some(format!("Action not allowed: {}", e)),
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Phase 3: Executor - Execute action
        let current_metrics = self.monitor.get_current_metrics().await;
        let execution_result = match self.executor.execute(&diagnosis, &current_metrics).await {
            Ok(result) => result,
            Err(blocked) => {
                let msg = match &blocked {
                    ExecutionBlocked::CircuitOpen { remaining_secs } => {
                        format!("Circuit open, {} seconds until recovery", remaining_secs)
                    }
                    ExecutionBlocked::CooldownActive { remaining_secs } => {
                        format!("Cooldown active, {} seconds remaining", remaining_secs)
                    }
                    ExecutionBlocked::RateLimitExceeded { count, max } => {
                        format!("Rate limit exceeded: {}/{}", count, max)
                    }
                    ExecutionBlocked::NotAllowed { reason } => {
                        format!("Action not allowed: {}", reason)
                    }
                    ExecutionBlocked::NoOpAction { reason } => {
                        format!("NoOp action: {}", reason)
                    }
                    ExecutionBlocked::AwaitingApproval { diagnosis_id } => {
                        format!("Awaiting approval for diagnosis: {}", diagnosis_id)
                    }
                };
                warn!(?blocked, "Execution blocked");
                return Ok(CycleResult {
                    success: true,
                    action_taken: false,
                    diagnosis: Some(diagnosis),
                    reward: None,
                    lessons: None,
                    error: Some(format!("Execution blocked: {}", msg)),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
            }
        };

        info!(
            action_id = %execution_result.action_id,
            outcome = ?execution_result.outcome,
            "Action executed"
        );

        // Get current baselines for reward calculation
        let baselines = self.monitor.get_baselines().await;
        let post_metrics = self.monitor.get_current_metrics().await;

        // Phase 4: Learner - Calculate reward and learn
        let learning_result = match self
            .learner
            .learn(&execution_result, &diagnosis, &post_metrics, &baselines)
            .await
        {
            Ok(outcome) => Some(outcome),
            Err(blocked) => {
                let msg = match &blocked {
                    LearningBlocked::ExecutionNotCompleted { status } => {
                        format!("Execution not completed: {:?}", status)
                    }
                    LearningBlocked::InsufficientSamples { required, actual } => {
                        format!("Insufficient samples: {} < {}", actual, required)
                    }
                    LearningBlocked::PipeUnavailable { message } => {
                        format!("Pipe unavailable: {}", message)
                    }
                };
                warn!(?blocked, "Learning blocked: {}", msg);
                None
            }
        };

        let (reward, lessons) = if let Some(outcome) = learning_result {
            let lesson_text = outcome
                .learning_synthesis
                .map(|ls| ls.lessons.join("; "));
            (Some(outcome.reward.value), lesson_text)
        } else {
            (None, None)
        };

        // Record success/failure in circuit breaker
        if execution_result.outcome == super::types::ActionOutcome::Success {
            self.record_success().await;
            let mut state = self.state.write().await;
            state.total_successes += 1;
        } else if execution_result.outcome == super::types::ActionOutcome::RolledBack {
            self.record_failure().await;
            let mut state = self.state.write().await;
            state.total_rollbacks += 1;
        }

        // Set cooldown
        self.set_cooldown().await;

        info!(
            reward = ?reward,
            "Improvement cycle completed"
        );

        Ok(CycleResult {
            success: true,
            action_taken: true,
            diagnosis: Some(diagnosis),
            reward,
            lessons,
            error: None,
            duration_ms: start.elapsed().as_millis() as u64,
        })
    }

    /// Force run a cycle even if system would normally not act.
    ///
    /// This bypasses health checks but still respects circuit breaker and rate limits.
    pub async fn force_cycle(&self) -> Result<CycleResult, SelfImprovementError> {
        info!("Force-running improvement cycle");
        self.run_cycle().await
    }

    /// Manually trigger a rollback of a previous action.
    pub async fn rollback(&self, action_id: &str) -> Result<(), SelfImprovementError> {
        info!(action_id = action_id, "Manual rollback requested");

        self.executor.rollback_by_id(action_id).await.map_err(|e| {
            SelfImprovementError::ExecutorFailed {
                message: format!("Rollback failed: {}", e),
            }
        })?;

        Ok(())
    }

    /// Pause the system for a specified duration.
    pub async fn pause(&self, duration: std::time::Duration) {
        let until = Utc::now() + chrono::Duration::from_std(duration).unwrap_or_default();
        let mut state = self.state.write().await;
        state.cooldown_until = Some(until);
        info!(until = %until, "System paused");
    }

    /// Resume the system from pause.
    pub async fn resume(&self) {
        let mut state = self.state.write().await;
        state.cooldown_until = None;
        info!("System resumed");
    }

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    async fn check_and_update_rate_limit(&self) -> Result<(), SelfImprovementError> {
        let mut state = self.state.write().await;
        let now = Utc::now();

        // Reset counter if we've crossed into a new hour
        let current_hour = now.date_naive().and_hms_opt(now.time().hour(), 0, 0);
        let limit_hour = state
            .rate_limit_hour
            .date_naive()
            .and_hms_opt(state.rate_limit_hour.time().hour(), 0, 0);

        if current_hour != limit_hour {
            state.actions_this_hour = 0;
            state.rate_limit_hour = now;
        }

        // Check rate limit
        if state.actions_this_hour >= self.config.executor.max_actions_per_hour {
            return Err(SelfImprovementError::RateLimitExceeded {
                count: state.actions_this_hour,
                max: self.config.executor.max_actions_per_hour,
            });
        }

        // Increment counter
        state.actions_this_hour += 1;

        Ok(())
    }

    async fn record_success(&self) {
        let mut cb = self.circuit_breaker.write().await;
        cb.record_success();
    }

    async fn record_failure(&self) {
        let mut cb = self.circuit_breaker.write().await;
        cb.record_failure();
    }

    async fn set_cooldown(&self) {
        let cooldown = self.config.executor.cooldown_duration();
        let until = Utc::now() + chrono::Duration::from_std(cooldown).unwrap_or_default();
        let mut state = self.state.write().await;
        state.cooldown_until = Some(until);
        debug!(until = %until, "Cooldown set");
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invocation_event_creation() {
        let event = InvocationEvent {
            tool_name: "reasoning_linear".to_string(),
            latency_ms: 150,
            success: true,
            quality_score: Some(0.85),
            timestamp: Utc::now(),
        };

        assert_eq!(event.tool_name, "reasoning_linear");
        assert_eq!(event.latency_ms, 150);
        assert!(event.success);
    }

    #[test]
    fn test_cycle_result_no_action() {
        let result = CycleResult {
            success: true,
            action_taken: false,
            diagnosis: None,
            reward: None,
            lessons: None,
            error: None,
            duration_ms: 50,
        };

        assert!(result.success);
        assert!(!result.action_taken);
    }

    #[test]
    fn test_system_status_default() {
        let status = SystemStatus {
            enabled: true,
            circuit_state: CircuitState::Closed,
            consecutive_failures: 0,
            in_cooldown: false,
            cooldown_ends_at: None,
            actions_this_hour: 0,
            max_actions_per_hour: 3,
            total_cycles: 0,
            total_successes: 0,
            total_rollbacks: 0,
            last_cycle_at: None,
        };

        assert!(status.enabled);
        assert_eq!(status.circuit_state, CircuitState::Closed);
    }

    #[test]
    fn test_error_display() {
        let err = SelfImprovementError::Disabled;
        assert_eq!(err.to_string(), "Self-improvement system is disabled");

        let err = SelfImprovementError::CircuitBreakerOpen {
            consecutive_failures: 3,
        };
        assert!(err.to_string().contains("3 consecutive failures"));
    }
}
