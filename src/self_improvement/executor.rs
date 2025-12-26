//! Executor phase for the self-improvement system.
//!
//! This module implements the third phase of the self-improvement loop:
//! - Validates actions against the allowlist
//! - Executes configuration changes safely
//! - Monitors for regressions after execution
//! - Rolls back changes if needed
//!
//! # Architecture
//!
//! ```text
//! SelfDiagnosis → Validation → Execution → Stabilization → Verification → Outcome
//!                    ↓                                          ↓
//!                 Reject                                    Rollback
//! ```
//!
//! The executor ensures all changes are safe, bounded, and reversible.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::allowlist::ActionAllowlist;
use super::circuit_breaker::CircuitBreaker;
use super::config::SelfImprovementConfig;
use super::types::{
    ActionId, ActionOutcome, DiagnosisId, DiagnosisStatus, MetricsSnapshot, NormalizedReward,
    ParamValue, ResourceType, SelfDiagnosis, SuggestedAction,
};

// ============================================================================
// Execution Result
// ============================================================================

/// Result of executing an action.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Unique ID for this execution
    pub action_id: ActionId,
    /// The diagnosis that triggered this action
    pub diagnosis_id: DiagnosisId,
    /// The action that was executed
    pub action: SuggestedAction,
    /// Configuration state before execution
    pub pre_state: ConfigState,
    /// Configuration state after execution
    pub post_state: Option<ConfigState>,
    /// Metrics before execution
    pub metrics_before: MetricsSnapshot,
    /// Metrics after stabilization (if verified)
    pub metrics_after: Option<MetricsSnapshot>,
    /// Execution outcome
    pub outcome: ActionOutcome,
    /// Rollback reason if rolled back
    pub rollback_reason: Option<String>,
    /// Calculated reward (if verified)
    pub reward: Option<NormalizedReward>,
    /// Time of execution
    pub executed_at: DateTime<Utc>,
    /// Time of verification
    pub verified_at: Option<DateTime<Utc>>,
}

/// Reason why execution was blocked.
#[derive(Debug, Clone)]
pub enum ExecutionBlocked {
    /// Circuit breaker is open
    CircuitOpen {
        /// Seconds until recovery
        remaining_secs: u64,
    },
    /// Action not allowed by allowlist
    NotAllowed {
        /// Reason for rejection
        reason: String,
    },
    /// Cooldown period active
    CooldownActive {
        /// Seconds until cooldown ends
        remaining_secs: u64,
    },
    /// Rate limit exceeded
    RateLimitExceeded {
        /// Actions executed this hour
        count: u32,
        /// Maximum allowed
        max: u32,
    },
    /// Awaiting manual approval
    AwaitingApproval {
        /// Diagnosis ID awaiting approval
        diagnosis_id: DiagnosisId,
    },
    /// Action is NoOp (nothing to execute)
    NoOpAction {
        /// Reason from the NoOp
        reason: String,
    },
}

// ============================================================================
// Configuration State
// ============================================================================

/// Snapshot of configuration state for before/after comparison.
#[derive(Debug, Clone, Default)]
pub struct ConfigState {
    /// Current parameter values
    pub params: HashMap<String, ParamValue>,
    /// Current feature states
    pub features: HashMap<String, bool>,
    /// Current resource limits
    pub resources: HashMap<ResourceType, u32>,
    /// Timestamp of snapshot
    pub timestamp: DateTime<Utc>,
}

impl ConfigState {
    /// Create a new empty config state.
    pub fn new() -> Self {
        Self {
            timestamp: Utc::now(),
            ..Default::default()
        }
    }

    /// Create config state from allowlist defaults.
    pub fn from_allowlist(allowlist: &ActionAllowlist) -> Self {
        let mut params = HashMap::new();
        for (key, bounds) in &allowlist.adjustable_params {
            params.insert(key.clone(), bounds.current_value.clone());
        }

        let mut features = HashMap::new();
        for feature in &allowlist.toggleable_features {
            features.insert(feature.clone(), true); // Default to enabled
        }

        let mut resources = HashMap::new();
        for (resource, bounds) in &allowlist.scalable_resources {
            resources.insert(*resource, bounds.min + (bounds.max - bounds.min) / 2);
        }

        Self {
            params,
            features,
            resources,
            timestamp: Utc::now(),
        }
    }
}

// ============================================================================
// Cooldown Tracker
// ============================================================================

/// Tracks cooldown periods between actions.
#[derive(Debug, Clone)]
struct CooldownTracker {
    /// End time of current cooldown
    ends_at: Option<DateTime<Utc>>,
    /// Reason for cooldown
    reason: Option<String>,
}

impl CooldownTracker {
    fn new() -> Self {
        Self {
            ends_at: None,
            reason: None,
        }
    }

    fn start(&mut self, duration: Duration, reason: &str) {
        let ends = Utc::now() + chrono::Duration::seconds(duration.as_secs() as i64);
        self.ends_at = Some(ends);
        self.reason = Some(reason.to_string());
    }

    fn is_active(&self) -> bool {
        self.ends_at.map(|e| Utc::now() < e).unwrap_or(false)
    }

    fn remaining_secs(&self) -> u64 {
        self.ends_at
            .map(|e| (e - Utc::now()).num_seconds().max(0) as u64)
            .unwrap_or(0)
    }

    fn clear(&mut self) {
        self.ends_at = None;
        self.reason = None;
    }
}

// ============================================================================
// Rate Limiter
// ============================================================================

/// Tracks action rate for limiting.
#[derive(Debug, Clone)]
struct RateLimiter {
    /// Action timestamps in the current hour
    actions: Vec<DateTime<Utc>>,
    /// Maximum actions per hour
    max_per_hour: u32,
}

impl RateLimiter {
    fn new(max_per_hour: u32) -> Self {
        Self {
            actions: Vec::new(),
            max_per_hour,
        }
    }

    fn prune_old(&mut self) {
        let one_hour_ago = Utc::now() - chrono::Duration::hours(1);
        self.actions.retain(|t| *t > one_hour_ago);
    }

    fn can_execute(&mut self) -> bool {
        self.prune_old();
        (self.actions.len() as u32) < self.max_per_hour
    }

    fn record(&mut self) {
        self.prune_old();
        self.actions.push(Utc::now());
    }

    fn count(&mut self) -> u32 {
        self.prune_old();
        self.actions.len() as u32
    }
}

// ============================================================================
// Executor State
// ============================================================================

/// Internal state for the executor.
#[derive(Debug)]
struct ExecutorState {
    /// Current configuration state
    config_state: ConfigState,
    /// Cooldown tracker
    cooldown: CooldownTracker,
    /// Rate limiter
    rate_limiter: RateLimiter,
    /// Pending execution awaiting verification
    pending_verification: Option<ExecutionResult>,
    /// Execution history
    history: Vec<ExecutionResult>,
    /// Total executions
    total_executions: u64,
    /// Total rollbacks
    total_rollbacks: u64,
}

// ============================================================================
// Executor
// ============================================================================

/// Executor phase for safe action execution.
///
/// The executor takes validated diagnoses from the Analyzer phase and
/// executes the recommended actions safely, with rollback capability.
///
/// # Example
///
/// ```rust,ignore
/// use mcp_langbase_reasoning::self_improvement::{Executor, SelfImprovementConfig};
///
/// let config = SelfImprovementConfig::from_env();
/// let allowlist = ActionAllowlist::default_allowlist();
/// let executor = Executor::new(config, allowlist, circuit_breaker);
///
/// // Execute an action
/// match executor.execute(&diagnosis).await {
///     Ok(result) => {
///         // Wait for stabilization and verify
///         tokio::time::sleep(config.executor.stabilization_period()).await;
///         executor.verify_and_complete(metrics_after).await;
///     }
///     Err(blocked) => {
///         // Handle blocked execution
///     }
/// }
/// ```
pub struct Executor {
    config: SelfImprovementConfig,
    allowlist: ActionAllowlist,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    state: Arc<RwLock<ExecutorState>>,
}

impl Executor {
    /// Create a new executor.
    pub fn new(
        config: SelfImprovementConfig,
        allowlist: ActionAllowlist,
        circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    ) -> Self {
        let initial_state = ConfigState::from_allowlist(&allowlist);

        Self {
            state: Arc::new(RwLock::new(ExecutorState {
                config_state: initial_state,
                cooldown: CooldownTracker::new(),
                rate_limiter: RateLimiter::new(config.executor.max_actions_per_hour),
                pending_verification: None,
                history: Vec::new(),
                total_executions: 0,
                total_rollbacks: 0,
            })),
            config,
            allowlist,
            circuit_breaker,
        }
    }

    /// Execute an action from a diagnosis.
    ///
    /// This validates the action, applies the change, and starts the
    /// stabilization period. Call `verify_and_complete` after stabilization.
    pub async fn execute(
        &self,
        diagnosis: &SelfDiagnosis,
        current_metrics: &MetricsSnapshot,
    ) -> Result<ExecutionResult, ExecutionBlocked> {
        let start = Instant::now();

        // Check for NoOp action
        if let SuggestedAction::NoOp { reason, .. } = &diagnosis.suggested_action {
            return Err(ExecutionBlocked::NoOpAction {
                reason: reason.clone(),
            });
        }

        // Check circuit breaker
        {
            let mut cb = self.circuit_breaker.write().await;
            if !cb.can_execute() {
                let remaining = cb
                    .time_until_recovery()
                    .map(|d| d.num_seconds().max(0) as u64)
                    .unwrap_or(3600);
                return Err(ExecutionBlocked::CircuitOpen {
                    remaining_secs: remaining,
                });
            }
        }

        // Check cooldown
        {
            let state = self.state.read().await;
            if state.cooldown.is_active() {
                return Err(ExecutionBlocked::CooldownActive {
                    remaining_secs: state.cooldown.remaining_secs(),
                });
            }
        }

        // Check rate limit
        {
            let mut state = self.state.write().await;
            if !state.rate_limiter.can_execute() {
                return Err(ExecutionBlocked::RateLimitExceeded {
                    count: state.rate_limiter.count(),
                    max: self.config.executor.max_actions_per_hour,
                });
            }
        }

        // Check approval requirement
        if self.config.executor.require_approval
            && diagnosis.status != DiagnosisStatus::AwaitingApproval
        {
            return Err(ExecutionBlocked::AwaitingApproval {
                diagnosis_id: diagnosis.id.clone(),
            });
        }

        // Validate action against allowlist
        if let Err(e) = self.allowlist.validate(&diagnosis.suggested_action) {
            return Err(ExecutionBlocked::NotAllowed {
                reason: e.to_string(),
            });
        }

        // Capture pre-execution state
        let pre_state = {
            let state = self.state.read().await;
            state.config_state.clone()
        };

        // Execute the action
        let post_state = self.apply_action(&diagnosis.suggested_action).await;

        let execution = ExecutionResult {
            action_id: ActionId::new(),
            diagnosis_id: diagnosis.id.clone(),
            action: diagnosis.suggested_action.clone(),
            pre_state,
            post_state: Some(post_state),
            metrics_before: current_metrics.clone(),
            metrics_after: None,
            outcome: ActionOutcome::Pending,
            rollback_reason: None,
            reward: None,
            executed_at: Utc::now(),
            verified_at: None,
        };

        // Record execution
        {
            let mut state = self.state.write().await;
            state.rate_limiter.record();
            state.pending_verification = Some(execution.clone());
            state.total_executions += 1;
        }

        let elapsed = start.elapsed();
        info!(
            action_id = %execution.action_id,
            action_type = execution.action.action_type(),
            elapsed_us = elapsed.as_micros(),
            "Executed action"
        );

        Ok(execution)
    }

    /// Verify the pending action and complete or rollback.
    ///
    /// This should be called after the stabilization period to compare
    /// metrics and decide whether to keep or rollback the change.
    pub async fn verify_and_complete(
        &self,
        metrics_after: &MetricsSnapshot,
        baselines: &super::types::Baselines,
    ) -> Option<ExecutionResult> {
        let mut state = self.state.write().await;

        let mut execution = state.pending_verification.take()?;

        // Calculate reward
        let reward = NormalizedReward::calculate(
            &execution
                .action
                .to_trigger_metric()
                .unwrap_or_else(|| super::types::TriggerMetric::ErrorRate {
                    observed: 0.0,
                    baseline: 0.0,
                    threshold: 0.0,
                }),
            &execution.metrics_before,
            metrics_after,
            baselines,
        );

        execution.metrics_after = Some(metrics_after.clone());
        execution.reward = Some(reward.clone());
        execution.verified_at = Some(Utc::now());

        // Decide outcome
        if reward.is_negative() && self.config.executor.rollback_on_regression {
            // Rollback
            warn!(
                action_id = %execution.action_id,
                reward = reward.value,
                "Negative reward, rolling back"
            );

            self.rollback_action(&execution.action, &execution.pre_state)
                .await;

            execution.outcome = ActionOutcome::RolledBack;
            execution.rollback_reason = Some(format!("Negative reward: {:.3}", reward.value));

            state.total_rollbacks += 1;

            // Record failure with circuit breaker
            drop(state);
            {
                let mut cb = self.circuit_breaker.write().await;
                cb.record_failure();
            }
        } else {
            // Success
            execution.outcome = ActionOutcome::Success;

            // Record success with circuit breaker
            drop(state);
            {
                let mut cb = self.circuit_breaker.write().await;
                cb.record_success();
            }

            // Start cooldown
            let mut state = self.state.write().await;
            state
                .cooldown
                .start(self.config.executor.cooldown_duration(), "Action completed");
        }

        // Store in history
        {
            let mut state = self.state.write().await;
            state.history.push(execution.clone());

            // Limit history size
            if state.history.len() > 100 {
                state.history.remove(0);
            }
        }

        info!(
            action_id = %execution.action_id,
            outcome = ?execution.outcome,
            reward = execution.reward.as_ref().map(|r| r.value),
            "Verification complete"
        );

        Some(execution)
    }

    /// Force rollback of the pending action.
    pub async fn force_rollback(&self, reason: &str) -> Option<ExecutionResult> {
        let mut state = self.state.write().await;

        let mut execution = state.pending_verification.take()?;

        self.rollback_action(&execution.action, &execution.pre_state)
            .await;

        execution.outcome = ActionOutcome::RolledBack;
        execution.rollback_reason = Some(reason.to_string());
        execution.verified_at = Some(Utc::now());

        state.total_rollbacks += 1;
        state.history.push(execution.clone());

        warn!(
            action_id = %execution.action_id,
            reason = reason,
            "Forced rollback"
        );

        Some(execution)
    }

    /// Get pending verification if any.
    pub async fn pending_verification(&self) -> Option<ExecutionResult> {
        self.state.read().await.pending_verification.clone()
    }

    /// Check if there's a pending verification.
    pub async fn has_pending(&self) -> bool {
        self.state.read().await.pending_verification.is_some()
    }

    /// Get execution history.
    pub async fn history(&self) -> Vec<ExecutionResult> {
        self.state.read().await.history.clone()
    }

    /// Get executor statistics.
    pub async fn stats(&self) -> ExecutorStats {
        let state = self.state.read().await;
        ExecutorStats {
            total_executions: state.total_executions,
            total_rollbacks: state.total_rollbacks,
            cooldown_active: state.cooldown.is_active(),
            cooldown_remaining_secs: state.cooldown.remaining_secs(),
            actions_this_hour: state.rate_limiter.actions.len() as u32,
            has_pending: state.pending_verification.is_some(),
        }
    }

    /// Clear cooldown (for testing or manual override).
    pub async fn clear_cooldown(&self) {
        self.state.write().await.cooldown.clear();
    }

    /// Get current config state.
    pub async fn config_state(&self) -> ConfigState {
        self.state.read().await.config_state.clone()
    }

    // ========================================================================
    // Internal: Action Application
    // ========================================================================

    async fn apply_action(&self, action: &SuggestedAction) -> ConfigState {
        let mut state = self.state.write().await;

        match action {
            SuggestedAction::AdjustParam {
                key, new_value, ..
            } => {
                debug!(key = %key, new_value = %new_value, "Adjusting parameter");
                state.config_state.params.insert(key.clone(), new_value.clone());
            }

            SuggestedAction::ToggleFeature {
                feature_name,
                desired_state,
                ..
            } => {
                debug!(feature = %feature_name, state = desired_state, "Toggling feature");
                state
                    .config_state
                    .features
                    .insert(feature_name.clone(), *desired_state);
            }

            SuggestedAction::ScaleResource {
                resource,
                new_value,
                ..
            } => {
                debug!(resource = ?resource, new_value = new_value, "Scaling resource");
                state
                    .config_state
                    .resources
                    .insert(*resource, *new_value);
            }

            SuggestedAction::ClearCache { cache_name } => {
                debug!(cache = %cache_name, "Clearing cache (simulated)");
                // In production, this would actually clear the cache
            }

            SuggestedAction::RestartService { component, graceful } => {
                debug!(component = ?component, graceful = graceful, "Restarting service (simulated)");
                // In production, this would trigger a graceful restart
            }

            SuggestedAction::NoOp { .. } => {
                // Nothing to do
            }
        }

        state.config_state.timestamp = Utc::now();
        state.config_state.clone()
    }

    async fn rollback_action(&self, action: &SuggestedAction, pre_state: &ConfigState) {
        let mut state = self.state.write().await;

        match action {
            SuggestedAction::AdjustParam { key, .. } => {
                if let Some(old_value) = pre_state.params.get(key) {
                    debug!(key = %key, "Rolling back parameter");
                    state.config_state.params.insert(key.clone(), old_value.clone());
                }
            }

            SuggestedAction::ToggleFeature { feature_name, .. } => {
                if let Some(&old_state) = pre_state.features.get(feature_name) {
                    debug!(feature = %feature_name, "Rolling back feature toggle");
                    state.config_state.features.insert(feature_name.clone(), old_state);
                }
            }

            SuggestedAction::ScaleResource { resource, .. } => {
                if let Some(&old_value) = pre_state.resources.get(resource) {
                    debug!(resource = ?resource, "Rolling back resource scale");
                    state.config_state.resources.insert(*resource, old_value);
                }
            }

            SuggestedAction::ClearCache { .. } | SuggestedAction::RestartService { .. } => {
                // These are not reversible
                warn!("Cannot rollback non-reversible action");
            }

            SuggestedAction::NoOp { .. } => {
                // Nothing to rollback
            }
        }

        state.config_state.timestamp = Utc::now();
    }
}

// ============================================================================
// Helper Trait
// ============================================================================

trait ToTriggerMetric {
    fn to_trigger_metric(&self) -> Option<super::types::TriggerMetric>;
}

impl ToTriggerMetric for SuggestedAction {
    fn to_trigger_metric(&self) -> Option<super::types::TriggerMetric> {
        // Create a placeholder trigger based on action type
        match self {
            SuggestedAction::AdjustParam { key, .. } => {
                if key.contains("TIMEOUT") || key.contains("RETRY") {
                    Some(super::types::TriggerMetric::ErrorRate {
                        observed: 0.0,
                        baseline: 0.0,
                        threshold: 0.0,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

// ============================================================================
// Executor Stats
// ============================================================================

/// Executor statistics for monitoring.
#[derive(Debug, Clone)]
pub struct ExecutorStats {
    /// Total actions executed
    pub total_executions: u64,
    /// Total rollbacks performed
    pub total_rollbacks: u64,
    /// Whether cooldown is active
    pub cooldown_active: bool,
    /// Seconds remaining in cooldown
    pub cooldown_remaining_secs: u64,
    /// Actions executed in the current hour
    pub actions_this_hour: u32,
    /// Whether there's a pending verification
    pub has_pending: bool,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_improvement::CircuitBreakerConfig;

    fn test_config() -> SelfImprovementConfig {
        let mut config = SelfImprovementConfig::default();
        config.executor.max_actions_per_hour = 10;
        config.executor.cooldown_duration_secs = 1; // Short for tests
        config.executor.require_approval = false;
        config.executor.rollback_on_regression = true;
        config
    }

    fn test_diagnosis(action: SuggestedAction) -> SelfDiagnosis {
        SelfDiagnosis {
            id: DiagnosisId::new(),
            created_at: Utc::now(),
            trigger: super::super::types::TriggerMetric::ErrorRate {
                observed: 0.10,
                baseline: 0.05,
                threshold: 0.08,
            },
            severity: super::super::types::Severity::Warning,
            description: "Test diagnosis".to_string(),
            suspected_cause: None,
            suggested_action: action,
            action_rationale: None,
            status: DiagnosisStatus::Pending,
        }
    }

    #[test]
    fn test_config_state_from_allowlist() {
        let allowlist = ActionAllowlist::default_allowlist();
        let state = ConfigState::from_allowlist(&allowlist);

        assert!(state.params.contains_key("REQUEST_TIMEOUT_MS"));
        assert!(!state.features.is_empty());
        assert!(!state.resources.is_empty());
    }

    #[test]
    fn test_cooldown_tracker() {
        let mut tracker = CooldownTracker::new();
        assert!(!tracker.is_active());

        tracker.start(Duration::from_secs(60), "test");
        assert!(tracker.is_active());
        assert!(tracker.remaining_secs() <= 60);

        tracker.clear();
        assert!(!tracker.is_active());
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(3);
        assert!(limiter.can_execute());
        assert_eq!(limiter.count(), 0);

        limiter.record();
        limiter.record();
        assert!(limiter.can_execute());
        assert_eq!(limiter.count(), 2);

        limiter.record();
        assert!(!limiter.can_execute());
        assert_eq!(limiter.count(), 3);
    }

    #[test]
    fn test_execution_blocked_variants() {
        let blocked = ExecutionBlocked::CircuitOpen { remaining_secs: 100 };
        assert!(matches!(blocked, ExecutionBlocked::CircuitOpen { .. }));

        let blocked = ExecutionBlocked::CooldownActive { remaining_secs: 50 };
        assert!(matches!(blocked, ExecutionBlocked::CooldownActive { .. }));

        let blocked = ExecutionBlocked::NoOpAction {
            reason: "test".to_string(),
        };
        assert!(matches!(blocked, ExecutionBlocked::NoOpAction { .. }));
    }

    #[tokio::test]
    async fn test_executor_blocks_noop() {
        let config = test_config();
        let allowlist = ActionAllowlist::default_allowlist();
        let cb = Arc::new(RwLock::new(CircuitBreaker::new(
            CircuitBreakerConfig::default(),
        )));
        let executor = Executor::new(config, allowlist, cb);

        let diagnosis = test_diagnosis(SuggestedAction::NoOp {
            reason: "test".to_string(),
            revisit_after: Duration::from_secs(60),
        });

        let metrics = MetricsSnapshot::new(0.05, 1000, 0.9, 100);
        let result = executor.execute(&diagnosis, &metrics).await;

        assert!(matches!(result, Err(ExecutionBlocked::NoOpAction { .. })));
    }

    #[tokio::test]
    async fn test_executor_stats() {
        let config = test_config();
        let allowlist = ActionAllowlist::default_allowlist();
        let cb = Arc::new(RwLock::new(CircuitBreaker::new(
            CircuitBreakerConfig::default(),
        )));
        let executor = Executor::new(config, allowlist, cb);

        let stats = executor.stats().await;
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.total_rollbacks, 0);
        assert!(!stats.cooldown_active);
        assert!(!stats.has_pending);
    }
}
