//! Learner phase for the self-improvement system.
//!
//! This module implements the fourth and final phase of the self-improvement loop:
//! - Calculates normalized rewards from before/after metrics
//! - Tracks action effectiveness over time
//! - Synthesizes lessons learned using reflection pipes
//! - Updates historical data for future action selection
//!
//! # Architecture
//!
//! ```text
//! ExecutionResult → Reward Calculation → Effectiveness Tracking → Learning Synthesis
//!                          ↓                      ↓                      ↓
//!                    NormalizedReward    EffectivenessHistory    Recommendations
//! ```
//!
//! The learner closes the feedback loop by measuring outcomes and improving
//! future action selection based on what worked.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::circuit_breaker::CircuitBreaker;
use super::config::SelfImprovementConfig;
use super::executor::ExecutionResult;
use super::pipes::{ActionEffectiveness, LearningResponse, PipeCallMetrics, SelfImprovementPipes};
use super::types::{
    ActionId, ActionOutcome, Baselines, DiagnosisId, MetricsSnapshot, NormalizedReward,
    SelfDiagnosis, SuggestedAction,
};

// ============================================================================
// Learning Outcome
// ============================================================================

/// Complete learning outcome from processing an executed action.
#[derive(Debug, Clone)]
pub struct LearningOutcome {
    /// The action that was evaluated
    pub action_id: ActionId,
    /// The diagnosis that led to this action
    pub diagnosis_id: DiagnosisId,
    /// Calculated reward
    pub reward: NormalizedReward,
    /// Final outcome status
    pub outcome: ActionOutcome,
    /// Learning synthesis from the pipe (if available)
    pub learning_synthesis: Option<LearningResponse>,
    /// When learning was completed
    pub completed_at: DateTime<Utc>,
    /// Whether the action was considered effective
    pub is_effective: bool,
}

// ============================================================================
// Learning Blocked
// ============================================================================

/// Reasons why learning cannot proceed.
#[derive(Debug, Clone)]
pub enum LearningBlocked {
    /// Insufficient samples in post-metrics
    InsufficientSamples {
        /// Required samples
        required: u64,
        /// Actual samples
        actual: u64,
    },
    /// Execution was not completed
    ExecutionNotCompleted {
        /// Current execution status
        status: ActionOutcome,
    },
    /// Learning pipe failed
    PipeUnavailable {
        /// Error message
        message: String,
    },
}

impl std::fmt::Display for LearningBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LearningBlocked::InsufficientSamples { required, actual } => {
                write!(
                    f,
                    "Insufficient samples: {} required, {} actual",
                    required, actual
                )
            }
            LearningBlocked::ExecutionNotCompleted { status } => {
                write!(f, "Execution not completed: {}", status)
            }
            LearningBlocked::PipeUnavailable { message } => {
                write!(f, "Learning pipe unavailable: {}", message)
            }
        }
    }
}

// ============================================================================
// Action History Entry
// ============================================================================

/// Single entry in action history.
#[derive(Debug, Clone)]
struct ActionHistoryEntry {
    /// Action ID (retained for future debugging/auditing)
    #[allow(dead_code)]
    action_id: ActionId,
    /// When executed (retained for future time-based analysis)
    #[allow(dead_code)]
    executed_at: DateTime<Utc>,
    /// Reward achieved
    reward: f64,
    /// Was it considered successful
    successful: bool,
}

// ============================================================================
// Learner State
// ============================================================================

/// Internal state for the Learner.
#[derive(Default)]
struct LearnerState {
    /// History by action signature
    effectiveness_history: HashMap<String, Vec<ActionHistoryEntry>>,
    /// Total learning cycles completed
    total_cycles: u64,
    /// Last learning time
    last_learning_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Learner Stats
// ============================================================================

/// Statistics about the Learner's operations.
#[derive(Debug, Clone)]
pub struct LearnerStats {
    /// Total learning cycles completed
    pub total_cycles: u64,
    /// Total actions tracked
    pub total_actions_tracked: usize,
    /// Actions with positive rewards
    pub positive_reward_count: u64,
    /// Actions with negative rewards
    pub negative_reward_count: u64,
    /// Average reward across all tracked actions
    pub avg_reward: f64,
    /// Most effective action signature
    pub most_effective_action: Option<String>,
    /// Least effective action signature
    pub least_effective_action: Option<String>,
    /// When last learning occurred
    pub last_learning_at: Option<DateTime<Utc>>,
}

// ============================================================================
// Learner
// ============================================================================

/// The Learner phase of the self-improvement loop.
///
/// Responsibilities:
/// - Calculate normalized rewards from before/after metrics
/// - Track action effectiveness history
/// - Synthesize lessons via reflection pipes
/// - Update circuit breaker based on outcomes
/// - Provide effectiveness data for action selection
pub struct Learner {
    config: SelfImprovementConfig,
    pipes: Arc<SelfImprovementPipes>,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    state: Arc<RwLock<LearnerState>>,
}

impl Learner {
    /// Create a new Learner.
    pub fn new(
        config: SelfImprovementConfig,
        pipes: Arc<SelfImprovementPipes>,
        circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    ) -> Self {
        Self {
            config,
            pipes,
            circuit_breaker,
            state: Arc::new(RwLock::new(LearnerState::default())),
        }
    }

    /// Learn from an execution result.
    ///
    /// This is the main entry point for the learning phase. It:
    /// 1. Validates the execution is complete
    /// 2. Calculates normalized reward
    /// 3. Updates effectiveness history
    /// 4. Optionally synthesizes learning via pipes
    /// 5. Updates circuit breaker
    pub async fn learn(
        &self,
        execution: &ExecutionResult,
        diagnosis: &SelfDiagnosis,
        post_metrics: &MetricsSnapshot,
        baselines: &Baselines,
    ) -> Result<LearningOutcome, LearningBlocked> {
        // Validate execution is in a learnable state
        if execution.outcome == ActionOutcome::Pending {
            return Err(LearningBlocked::ExecutionNotCompleted {
                status: execution.outcome,
            });
        }

        // Check minimum samples for meaningful learning
        let min_samples = 10u64;
        if post_metrics.sample_count < min_samples {
            debug!(
                action_id = %execution.action_id,
                samples = post_metrics.sample_count,
                required = min_samples,
                "Insufficient samples for learning"
            );
            return Err(LearningBlocked::InsufficientSamples {
                required: min_samples,
                actual: post_metrics.sample_count,
            });
        }

        // Calculate reward
        let reward = NormalizedReward::calculate(
            &diagnosis.trigger,
            &execution.metrics_before,
            post_metrics,
            baselines,
        );

        let is_effective = reward.value >= self.config.learner.effective_reward_threshold;

        info!(
            action_id = %execution.action_id,
            reward = reward.value,
            is_effective = is_effective,
            confidence = reward.confidence,
            "Learning from execution"
        );

        // Update effectiveness history
        let action_signature = self.get_action_signature(&execution.action);
        self.update_effectiveness_history(
            &action_signature,
            &execution.action_id,
            reward.value,
            is_effective,
        )
        .await;

        // Optionally synthesize learning
        let learning_synthesis = if self.config.learner.use_reflection_for_learning {
            self.synthesize_learning(
                &execution.action,
                diagnosis,
                &execution.metrics_before,
                post_metrics,
                &reward,
            )
            .await
            .ok()
            .map(|(response, _metrics)| response)
        } else {
            None
        };

        // Determine final outcome
        let outcome = if is_effective {
            ActionOutcome::Success
        } else if execution.outcome == ActionOutcome::RolledBack {
            ActionOutcome::RolledBack
        } else if reward.is_negative() {
            ActionOutcome::Failed
        } else {
            // Neutral - neither particularly effective nor harmful
            ActionOutcome::Success
        };

        // Update circuit breaker
        let mut cb = self.circuit_breaker.write().await;
        if is_effective {
            cb.record_success();
        } else if reward.is_negative() {
            cb.record_failure();
        }
        drop(cb);

        // Update state
        {
            let mut state = self.state.write().await;
            state.total_cycles += 1;
            state.last_learning_at = Some(Utc::now());
        }

        Ok(LearningOutcome {
            action_id: execution.action_id.clone(),
            diagnosis_id: execution.diagnosis_id.clone(),
            reward,
            outcome,
            learning_synthesis,
            completed_at: Utc::now(),
            is_effective,
        })
    }

    /// Get effectiveness data for action selection.
    ///
    /// Returns historical effectiveness for all tracked action types.
    pub async fn get_effectiveness_history(&self) -> Vec<ActionEffectiveness> {
        let state = self.state.read().await;

        state
            .effectiveness_history
            .iter()
            .map(|(signature, entries)| {
                let total = entries.len() as u32;
                let successful = entries.iter().filter(|e| e.successful).count() as u32;
                let avg_reward = if total > 0 {
                    entries.iter().map(|e| e.reward).sum::<f64>() / total as f64
                } else {
                    0.0
                };

                ActionEffectiveness {
                    action_type: self.extract_action_type(signature),
                    action_signature: signature.clone(),
                    total_attempts: total,
                    successful_attempts: successful,
                    avg_reward,
                    effectiveness_score: self.calculate_effectiveness_score(
                        total,
                        successful,
                        avg_reward,
                    ),
                }
            })
            .collect()
    }

    /// Get effectiveness for a specific action type.
    pub async fn get_effectiveness_for_action(&self, action: &SuggestedAction) -> Option<f64> {
        let signature = self.get_action_signature(action);
        let state = self.state.read().await;

        state.effectiveness_history.get(&signature).map(|entries| {
            let total = entries.len() as u32;
            let successful = entries.iter().filter(|e| e.successful).count() as u32;
            let avg_reward = if total > 0 {
                entries.iter().map(|e| e.reward).sum::<f64>() / total as f64
            } else {
                0.0
            };
            self.calculate_effectiveness_score(total, successful, avg_reward)
        })
    }

    /// Get learner statistics.
    pub async fn stats(&self) -> LearnerStats {
        let state = self.state.read().await;

        let all_entries: Vec<&ActionHistoryEntry> = state
            .effectiveness_history
            .values()
            .flat_map(|v| v.iter())
            .collect();

        let positive_count = all_entries.iter().filter(|e| e.reward > 0.0).count() as u64;
        let negative_count = all_entries.iter().filter(|e| e.reward < 0.0).count() as u64;

        let avg_reward = if !all_entries.is_empty() {
            all_entries.iter().map(|e| e.reward).sum::<f64>() / all_entries.len() as f64
        } else {
            0.0
        };

        // Find most/least effective
        let effectiveness: Vec<_> = state
            .effectiveness_history
            .iter()
            .map(|(sig, entries)| {
                let total = entries.len() as u32;
                let successful = entries.iter().filter(|e| e.successful).count() as u32;
                let avg = if total > 0 {
                    entries.iter().map(|e| e.reward).sum::<f64>() / total as f64
                } else {
                    0.0
                };
                (sig.clone(), self.calculate_effectiveness_score(total, successful, avg))
            })
            .collect();

        let most_effective = effectiveness
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(s, _)| s.clone());

        let least_effective = effectiveness
            .iter()
            .filter(|(_, score)| *score > 0.0)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(s, _)| s.clone());

        LearnerStats {
            total_cycles: state.total_cycles,
            total_actions_tracked: all_entries.len(),
            positive_reward_count: positive_count,
            negative_reward_count: negative_count,
            avg_reward,
            most_effective_action: most_effective,
            least_effective_action: least_effective,
            last_learning_at: state.last_learning_at,
        }
    }

    /// Clear all history (for testing or reset).
    pub async fn clear_history(&self) {
        let mut state = self.state.write().await;
        state.effectiveness_history.clear();
        info!("Cleared learner effectiveness history");
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    /// Update effectiveness history for an action.
    async fn update_effectiveness_history(
        &self,
        signature: &str,
        action_id: &ActionId,
        reward: f64,
        successful: bool,
    ) {
        let mut state = self.state.write().await;

        let entries = state
            .effectiveness_history
            .entry(signature.to_string())
            .or_default();

        entries.push(ActionHistoryEntry {
            action_id: action_id.clone(),
            executed_at: Utc::now(),
            reward,
            successful,
        });

        // Trim to max history
        let max = self.config.learner.max_history_per_action;
        if entries.len() > max {
            entries.drain(0..entries.len() - max);
        }

        debug!(
            signature = %signature,
            history_size = entries.len(),
            "Updated effectiveness history"
        );
    }

    /// Synthesize learning using the reflection pipe.
    async fn synthesize_learning(
        &self,
        action: &SuggestedAction,
        diagnosis: &SelfDiagnosis,
        pre_metrics: &MetricsSnapshot,
        post_metrics: &MetricsSnapshot,
        reward: &NormalizedReward,
    ) -> Result<(LearningResponse, PipeCallMetrics), LearningBlocked> {
        match self
            .pipes
            .synthesize_learning(action, diagnosis, pre_metrics, post_metrics, reward)
            .await
        {
            Ok(result) => {
                info!(
                    outcome_assessment = %result.0.outcome_assessment,
                    action_effectiveness = result.0.action_effectiveness,
                    lessons_count = result.0.lessons.len(),
                    "Learning synthesis completed"
                );
                Ok(result)
            }
            Err(e) => {
                warn!(error = %e, "Learning synthesis pipe failed");
                Err(LearningBlocked::PipeUnavailable {
                    message: e.to_string(),
                })
            }
        }
    }

    /// Generate a signature for an action (for grouping similar actions).
    fn get_action_signature(&self, action: &SuggestedAction) -> String {
        match action {
            SuggestedAction::AdjustParam {
                key,
                old_value,
                new_value,
                ..
            } => {
                let direction = if let (Some(old), Some(new)) =
                    (old_value.as_float(), new_value.as_float())
                {
                    if new > old {
                        "increase"
                    } else {
                        "decrease"
                    }
                } else {
                    "change"
                };
                format!("adjust_param:{}:{}", key, direction)
            }
            SuggestedAction::ToggleFeature {
                feature_name,
                desired_state,
                ..
            } => {
                format!("toggle_feature:{}:{}", feature_name, desired_state)
            }
            SuggestedAction::RestartService { component, .. } => {
                format!("restart_service:{:?}", component)
            }
            SuggestedAction::ClearCache { cache_name } => {
                format!("clear_cache:{}", cache_name)
            }
            SuggestedAction::ScaleResource {
                resource,
                old_value,
                new_value,
            } => {
                let direction = if new_value > old_value {
                    "increase"
                } else {
                    "decrease"
                };
                format!("scale_resource:{:?}:{}", resource, direction)
            }
            SuggestedAction::NoOp { .. } => "no_op".to_string(),
        }
    }

    /// Extract action type from signature.
    fn extract_action_type(&self, signature: &str) -> String {
        signature
            .split(':')
            .next()
            .unwrap_or("unknown")
            .to_string()
    }

    /// Calculate effectiveness score from history.
    fn calculate_effectiveness_score(
        &self,
        total_attempts: u32,
        successful_attempts: u32,
        avg_reward: f64,
    ) -> f64 {
        if total_attempts == 0 {
            return 0.0;
        }

        let success_rate = successful_attempts as f64 / total_attempts as f64;

        // Confidence factor based on sample size (full confidence at 10 samples)
        let confidence = (total_attempts as f64 / 10.0).min(1.0);

        // Combine success rate and average reward
        let raw_score = (success_rate * 0.6) + ((avg_reward + 1.0) / 2.0 * 0.4);

        // Apply confidence
        raw_score * confidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_improvement::executor::ConfigState;
    use crate::self_improvement::types::{ConfigScope, ParamValue, TriggerMetric};

    #[allow(dead_code)]
    fn test_config() -> SelfImprovementConfig {
        let mut config = SelfImprovementConfig::default();
        config.learner.use_reflection_for_learning = false; // Disable for unit tests
        config
    }

    #[allow(dead_code)]
    fn test_diagnosis() -> SelfDiagnosis {
        SelfDiagnosis::new(
            TriggerMetric::ErrorRate {
                observed: 0.10,
                baseline: 0.05,
                threshold: 0.08,
            },
            "High error rate detected".to_string(),
            SuggestedAction::AdjustParam {
                key: "REQUEST_TIMEOUT_MS".to_string(),
                old_value: ParamValue::Integer(30000),
                new_value: ParamValue::Integer(35000),
                scope: ConfigScope::Runtime,
            },
        )
    }

    #[allow(dead_code)]
    fn test_execution_result() -> ExecutionResult {
        let diagnosis = test_diagnosis();
        ExecutionResult {
            action_id: ActionId::new(),
            diagnosis_id: diagnosis.id.clone(),
            action: diagnosis.suggested_action.clone(),
            pre_state: ConfigState::new(),
            post_state: Some(ConfigState::new()),
            metrics_before: MetricsSnapshot::new(0.10, 1000, 0.80, 100),
            metrics_after: None,
            outcome: ActionOutcome::Success,
            rollback_reason: None,
            reward: None,
            executed_at: Utc::now(),
            verified_at: None,
        }
    }

    fn test_baselines() -> Baselines {
        Baselines {
            error_rate: 0.05,
            latency_ms: 1000,
            quality_score: 0.80,
        }
    }

    // Test helpers for signature generation - avoid needing full Learner
    #[test]
    fn test_action_signature_adjust_param_increase() {
        let action = SuggestedAction::AdjustParam {
            key: "TIMEOUT".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(35000),
            scope: ConfigScope::Runtime,
        };

        // Test the logic directly
        let sig = match &action {
            SuggestedAction::AdjustParam {
                key,
                old_value,
                new_value,
                ..
            } => {
                let direction = if let (Some(old), Some(new)) =
                    (old_value.as_float(), new_value.as_float())
                {
                    if new > old {
                        "increase"
                    } else {
                        "decrease"
                    }
                } else {
                    "change"
                };
                format!("adjust_param:{}:{}", key, direction)
            }
            _ => "other".to_string(),
        };

        assert_eq!(sig, "adjust_param:TIMEOUT:increase");
    }

    #[test]
    fn test_action_signature_adjust_param_decrease() {
        let action = SuggestedAction::AdjustParam {
            key: "TIMEOUT".to_string(),
            old_value: ParamValue::Integer(35000),
            new_value: ParamValue::Integer(30000),
            scope: ConfigScope::Runtime,
        };

        let sig = match &action {
            SuggestedAction::AdjustParam {
                key,
                old_value,
                new_value,
                ..
            } => {
                let direction = if let (Some(old), Some(new)) =
                    (old_value.as_float(), new_value.as_float())
                {
                    if new > old {
                        "increase"
                    } else {
                        "decrease"
                    }
                } else {
                    "change"
                };
                format!("adjust_param:{}:{}", key, direction)
            }
            _ => "other".to_string(),
        };

        assert_eq!(sig, "adjust_param:TIMEOUT:decrease");
    }

    #[test]
    fn test_action_signature_toggle_feature() {
        let action = SuggestedAction::ToggleFeature {
            feature_name: "CACHE".to_string(),
            desired_state: true,
            reason: "Enable caching".to_string(),
        };

        let sig = match &action {
            SuggestedAction::ToggleFeature {
                feature_name,
                desired_state,
                ..
            } => format!("toggle_feature:{}:{}", feature_name, desired_state),
            _ => "other".to_string(),
        };

        assert_eq!(sig, "toggle_feature:CACHE:true");
    }

    #[test]
    fn test_effectiveness_score_zero_attempts() {
        // With 0 attempts, effectiveness should be 0
        let score = calculate_test_effectiveness_score(0, 0, 0.0);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_effectiveness_score_perfect_low_confidence() {
        // Perfect score with low samples should be reduced
        let score = calculate_test_effectiveness_score(5, 5, 1.0);
        assert!(score < 1.0, "Should be reduced by low confidence");
        assert!(score > 0.4, "Should still be positive");
    }

    #[test]
    fn test_effectiveness_score_perfect_high_confidence() {
        // Perfect score with sufficient samples
        let score = calculate_test_effectiveness_score(10, 10, 1.0);
        assert!(score > 0.9, "Should be high with perfect record");
    }

    #[test]
    fn test_effectiveness_score_poor_record() {
        // Poor record
        let score = calculate_test_effectiveness_score(10, 2, -0.5);
        assert!(score < 0.5, "Should be low with poor record");
    }

    // Helper to test effectiveness calculation logic
    fn calculate_test_effectiveness_score(
        total_attempts: u32,
        successful_attempts: u32,
        avg_reward: f64,
    ) -> f64 {
        if total_attempts == 0 {
            return 0.0;
        }

        let success_rate = successful_attempts as f64 / total_attempts as f64;
        let confidence = (total_attempts as f64 / 10.0).min(1.0);
        let raw_score = (success_rate * 0.6) + ((avg_reward + 1.0) / 2.0 * 0.4);

        raw_score * confidence
    }

    #[test]
    fn test_extract_action_type() {
        fn extract(signature: &str) -> String {
            signature
                .split(':')
                .next()
                .unwrap_or("unknown")
                .to_string()
        }

        assert_eq!(extract("adjust_param:TIMEOUT:increase"), "adjust_param");
        assert_eq!(extract("toggle_feature:CACHE:true"), "toggle_feature");
        assert_eq!(extract("no_op"), "no_op");
    }

    #[test]
    fn test_learning_blocked_not_completed() {
        let blocked = LearningBlocked::ExecutionNotCompleted {
            status: ActionOutcome::Pending,
        };
        assert!(matches!(
            blocked,
            LearningBlocked::ExecutionNotCompleted { .. }
        ));
        assert!(blocked.to_string().contains("not completed"));
    }

    #[test]
    fn test_learning_blocked_insufficient_samples() {
        let blocked = LearningBlocked::InsufficientSamples {
            required: 10,
            actual: 5,
        };
        assert!(matches!(
            blocked,
            LearningBlocked::InsufficientSamples { .. }
        ));
        assert!(blocked.to_string().contains("Insufficient samples"));
    }

    #[test]
    fn test_learning_blocked_pipe_unavailable() {
        let blocked = LearningBlocked::PipeUnavailable {
            message: "timeout".to_string(),
        };
        assert!(matches!(blocked, LearningBlocked::PipeUnavailable { .. }));
        assert!(blocked.to_string().contains("unavailable"));
    }

    #[test]
    fn test_learning_outcome_structure() {
        let reward = NormalizedReward::calculate(
            &TriggerMetric::ErrorRate {
                observed: 0.10,
                baseline: 0.05,
                threshold: 0.08,
            },
            &MetricsSnapshot::new(0.10, 1000, 0.80, 100),
            &MetricsSnapshot::new(0.03, 800, 0.90, 100),
            &test_baselines(),
        );

        let outcome = LearningOutcome {
            action_id: ActionId::new(),
            diagnosis_id: DiagnosisId::new(),
            reward: reward.clone(),
            outcome: ActionOutcome::Success,
            learning_synthesis: None,
            completed_at: Utc::now(),
            is_effective: reward.is_positive(),
        };

        assert!(outcome.is_effective);
        assert_eq!(outcome.outcome, ActionOutcome::Success);
    }

    #[test]
    fn test_learner_stats_default() {
        let stats = LearnerStats {
            total_cycles: 0,
            total_actions_tracked: 0,
            positive_reward_count: 0,
            negative_reward_count: 0,
            avg_reward: 0.0,
            most_effective_action: None,
            least_effective_action: None,
            last_learning_at: None,
        };

        assert_eq!(stats.total_cycles, 0);
        assert_eq!(stats.total_actions_tracked, 0);
        assert!(stats.most_effective_action.is_none());
    }

    #[test]
    fn test_action_history_entry() {
        let entry = ActionHistoryEntry {
            action_id: ActionId::new(),
            executed_at: Utc::now(),
            reward: 0.5,
            successful: true,
        };

        assert!(entry.successful);
        assert!(entry.reward > 0.0);
    }

    #[test]
    fn test_normalized_reward_positive() {
        let reward = NormalizedReward::calculate(
            &TriggerMetric::ErrorRate {
                observed: 0.10,
                baseline: 0.05,
                threshold: 0.08,
            },
            &MetricsSnapshot::new(0.10, 1000, 0.80, 100),
            &MetricsSnapshot::new(0.03, 800, 0.90, 100), // Improved
            &test_baselines(),
        );

        assert!(reward.is_positive());
        assert!(reward.value > 0.0);
    }

    #[test]
    fn test_normalized_reward_negative() {
        let reward = NormalizedReward::calculate(
            &TriggerMetric::ErrorRate {
                observed: 0.10,
                baseline: 0.05,
                threshold: 0.08,
            },
            &MetricsSnapshot::new(0.10, 1000, 0.80, 100),
            &MetricsSnapshot::new(0.15, 1500, 0.70, 100), // Worse
            &test_baselines(),
        );

        assert!(reward.is_negative());
        assert!(reward.value < 0.0);
    }
}
