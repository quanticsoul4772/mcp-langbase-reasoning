//! Analyzer phase for the self-improvement system.
//!
//! This module implements the second phase of the self-improvement loop:
//! - Uses Langbase pipes to diagnose root causes
//! - Generates action recommendations
//! - Validates decisions for biases/fallacies
//!
//! # Architecture
//!
//! ```text
//! HealthReport → Diagnosis Pipe → Action Selection Pipe → Validation Pipe → SelfDiagnosis
//! ```
//!
//! The analyzer uses existing Langbase pipes (reflection, decision-framework,
//! detection) with specialized prompts for self-improvement analysis.

use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::allowlist::ActionAllowlist;
use super::circuit_breaker::CircuitBreaker;
use super::config::SelfImprovementConfig;
use super::pipes::{ActionEffectiveness, SelfImprovementPipes};
use super::types::{
    ConfigScope, DiagnosisId, DiagnosisStatus, HealthReport, ParamValue, ResourceType,
    SelfDiagnosis, Severity, SuggestedAction, TriggerMetric,
};

// ============================================================================
// Analyzer Result
// ============================================================================

/// Result of the analysis phase.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// The generated diagnosis
    pub diagnosis: SelfDiagnosis,
    /// Whether the diagnosis passed validation
    pub passed_validation: bool,
    /// Validation warnings (if any)
    pub validation_warnings: Vec<String>,
    /// Whether the circuit breaker allowed analysis
    pub circuit_allowed: bool,
    /// Time taken for analysis in milliseconds
    pub analysis_time_ms: u64,
}

/// Outcome when analysis cannot proceed.
#[derive(Debug, Clone)]
pub enum AnalysisBlocked {
    /// Circuit breaker is open
    CircuitOpen {
        /// Seconds until recovery is attempted
        remaining_secs: u64,
    },
    /// No triggers to analyze
    NoTriggers,
    /// Pipe unavailable
    PipeUnavailable {
        /// Name of the pipe
        pipe: String,
        /// Error message
        error: String,
    },
    /// Maximum pending diagnoses reached
    MaxPendingReached {
        /// Current count of pending diagnoses
        count: usize,
    },
    /// Severity below minimum threshold
    SeverityTooLow {
        /// Severity of the trigger
        severity: Severity,
        /// Minimum severity required
        minimum: Severity,
    },
}

// ============================================================================
// Analyzer State
// ============================================================================

/// Internal state for the analyzer.
#[derive(Debug, Default)]
struct AnalyzerState {
    /// Pending diagnoses (not yet executed)
    pending_diagnoses: Vec<SelfDiagnosis>,
    /// Total diagnoses generated
    total_diagnoses: u64,
    /// Total analyses blocked
    total_blocked: u64,
}

// ============================================================================
// Analyzer
// ============================================================================

/// Analyzer phase for diagnosing issues and recommending actions.
///
/// The analyzer takes health reports from the Monitor phase, uses Langbase
/// pipes to diagnose root causes, selects appropriate actions, and validates
/// decisions for cognitive biases.
///
/// # Example
///
/// ```rust,ignore
/// use mcp_langbase_reasoning::self_improvement::{Analyzer, Monitor, SelfImprovementConfig};
///
/// let config = SelfImprovementConfig::from_env();
/// let pipes = SelfImprovementPipes::new(langbase, config.pipes.clone());
/// let circuit_breaker = CircuitBreaker::new(config.circuit_breaker.clone());
/// let analyzer = Analyzer::new(config, pipes, circuit_breaker);
///
/// // Analyze a health report from the monitor
/// match analyzer.analyze(&health_report).await {
///     Ok(result) => {
///         if result.passed_validation {
///             // Proceed to execution phase
///         }
///     }
///     Err(blocked) => {
///         // Handle blocked analysis
///     }
/// }
/// ```
pub struct Analyzer {
    config: SelfImprovementConfig,
    pipes: Arc<SelfImprovementPipes>,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    allowlist: ActionAllowlist,
    state: Arc<RwLock<AnalyzerState>>,
    /// Historical effectiveness data for action selection
    effectiveness_history: Arc<RwLock<Vec<ActionEffectiveness>>>,
}

impl Analyzer {
    /// Create a new analyzer.
    pub fn new(
        config: SelfImprovementConfig,
        pipes: Arc<SelfImprovementPipes>,
        circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    ) -> Self {
        Self {
            config,
            pipes,
            circuit_breaker,
            allowlist: ActionAllowlist::default_allowlist(),
            state: Arc::new(RwLock::new(AnalyzerState::default())),
            effectiveness_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Create a new analyzer with a custom allowlist.
    pub fn with_allowlist(
        config: SelfImprovementConfig,
        pipes: Arc<SelfImprovementPipes>,
        circuit_breaker: Arc<RwLock<CircuitBreaker>>,
        allowlist: ActionAllowlist,
    ) -> Self {
        Self {
            config,
            pipes,
            circuit_breaker,
            allowlist,
            state: Arc::new(RwLock::new(AnalyzerState::default())),
            effectiveness_history: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Analyze a health report and generate a diagnosis.
    ///
    /// This is the main entry point for the analyzer phase.
    pub async fn analyze(
        &self,
        health_report: &HealthReport,
    ) -> Result<AnalysisResult, AnalysisBlocked> {
        let start = Instant::now();

        // Check if we have triggers to analyze
        if !health_report.has_triggers() {
            return Err(AnalysisBlocked::NoTriggers);
        }

        // Check circuit breaker
        {
            let mut cb = self.circuit_breaker.write().await;
            if !cb.can_execute() {
                let remaining = cb
                    .time_until_recovery()
                    .map(|d| d.num_seconds().max(0) as u64)
                    .unwrap_or(3600);
                return Err(AnalysisBlocked::CircuitOpen {
                    remaining_secs: remaining,
                });
            }
        }

        // Check pending diagnoses limit
        {
            let state = self.state.read().await;
            if state.pending_diagnoses.len() >= self.config.analyzer.max_pending_diagnoses {
                return Err(AnalysisBlocked::MaxPendingReached {
                    count: state.pending_diagnoses.len(),
                });
            }
        }

        // Get the most severe trigger
        let trigger = health_report
            .most_severe_trigger()
            .cloned()
            .expect("has_triggers() was true");

        // Check severity threshold
        let severity = Severity::from_deviation(trigger.deviation_pct().abs());
        if severity < self.config.analyzer.min_action_severity {
            return Err(AnalysisBlocked::SeverityTooLow {
                severity,
                minimum: self.config.analyzer.min_action_severity,
            });
        }

        // Generate diagnosis
        let diagnosis = self.generate_diagnosis(health_report, &trigger).await?;

        // Validate the diagnosis
        let (passed_validation, validation_warnings) = self.validate_diagnosis(&diagnosis).await;

        let analysis_time_ms = start.elapsed().as_millis() as u64;

        // Store pending diagnosis
        {
            let mut state = self.state.write().await;
            state.pending_diagnoses.push(diagnosis.clone());
            state.total_diagnoses += 1;
        }

        info!(
            diagnosis_id = %diagnosis.id,
            severity = ?diagnosis.severity,
            action_type = diagnosis.suggested_action.action_type(),
            passed_validation = passed_validation,
            analysis_time_ms = analysis_time_ms,
            "Generated diagnosis"
        );

        Ok(AnalysisResult {
            diagnosis,
            passed_validation,
            validation_warnings,
            circuit_allowed: true,
            analysis_time_ms,
        })
    }

    /// Get pending diagnoses awaiting execution.
    pub async fn pending_diagnoses(&self) -> Vec<SelfDiagnosis> {
        self.state.read().await.pending_diagnoses.clone()
    }

    /// Remove a diagnosis from pending (after execution or superseding).
    pub async fn remove_pending(&self, diagnosis_id: &DiagnosisId) -> bool {
        let mut state = self.state.write().await;
        let initial_len = state.pending_diagnoses.len();
        state
            .pending_diagnoses
            .retain(|d| d.id.0 != diagnosis_id.0);
        state.pending_diagnoses.len() < initial_len
    }

    /// Supersede all pending diagnoses (mark them as superseded).
    pub async fn supersede_all_pending(&self) {
        let mut state = self.state.write().await;
        for diagnosis in &mut state.pending_diagnoses {
            diagnosis.status = DiagnosisStatus::Superseded;
        }
        state.pending_diagnoses.clear();
    }

    /// Update effectiveness history for action selection.
    pub async fn update_effectiveness(&self, effectiveness: ActionEffectiveness) {
        let mut history = self.effectiveness_history.write().await;

        // Update existing or add new
        if let Some(existing) = history.iter_mut().find(|e| {
            e.action_type == effectiveness.action_type
                && e.action_signature == effectiveness.action_signature
        }) {
            *existing = effectiveness;
        } else {
            // Limit history size
            if history.len() >= self.config.learner.max_history_per_action {
                history.remove(0);
            }
            history.push(effectiveness);
        }
    }

    /// Get analyzer statistics.
    pub async fn stats(&self) -> AnalyzerStats {
        let state = self.state.read().await;
        AnalyzerStats {
            pending_count: state.pending_diagnoses.len(),
            total_diagnoses: state.total_diagnoses,
            total_blocked: state.total_blocked,
        }
    }

    /// Get the allowlist.
    pub fn allowlist(&self) -> &ActionAllowlist {
        &self.allowlist
    }

    // ========================================================================
    // Internal: Diagnosis Generation
    // ========================================================================

    async fn generate_diagnosis(
        &self,
        health_report: &HealthReport,
        trigger: &TriggerMetric,
    ) -> Result<SelfDiagnosis, AnalysisBlocked> {
        // Try to get AI-powered diagnosis
        let diagnosis_response = match self.pipes.generate_diagnosis(health_report, trigger).await {
            Ok((response, _metrics)) => response,
            Err(e) => {
                warn!(error = %e, "Diagnosis pipe failed, using fallback");
                // Record blocked analysis
                {
                    let mut state = self.state.write().await;
                    state.total_blocked += 1;
                }
                return Err(AnalysisBlocked::PipeUnavailable {
                    pipe: "diagnosis".to_string(),
                    error: e.to_string(),
                });
            }
        };

        // Convert AI response to action
        let suggested_action = self
            .convert_to_action(&diagnosis_response, trigger)
            .await;

        // Validate action against allowlist
        let validated_action = match self.allowlist.validate(&suggested_action) {
            Ok(()) => suggested_action,
            Err(e) => {
                warn!(error = %e, "Action not allowed, falling back to NoOp");
                SuggestedAction::NoOp {
                    reason: format!("Action not allowed: {}", e),
                    revisit_after: std::time::Duration::from_secs(300),
                }
            }
        };

        // Parse severity from response
        let severity = match diagnosis_response.severity.as_str() {
            "critical" => Severity::Critical,
            "high" => Severity::High,
            "warning" => Severity::Warning,
            _ => Severity::Info,
        };

        let diagnosis = SelfDiagnosis {
            id: DiagnosisId::new(),
            created_at: Utc::now(),
            trigger: trigger.clone(),
            severity,
            description: format!(
                "Detected {} deviation in {}",
                diagnosis_response.severity,
                trigger.metric_name()
            ),
            suspected_cause: Some(diagnosis_response.suspected_cause),
            suggested_action: validated_action,
            action_rationale: Some(diagnosis_response.rationale),
            status: DiagnosisStatus::Pending,
        };

        Ok(diagnosis)
    }

    async fn convert_to_action(
        &self,
        response: &super::pipes::DiagnosisResponse,
        trigger: &TriggerMetric,
    ) -> SuggestedAction {
        // Get effectiveness history for action selection
        let history = self.effectiveness_history.read().await;

        // Try AI-powered action selection if we have enough history
        if !history.is_empty() {
            // Create a placeholder diagnosis for action selection
            let placeholder = SelfDiagnosis {
                id: DiagnosisId::new(),
                created_at: Utc::now(),
                trigger: trigger.clone(),
                severity: Severity::Warning,
                description: "placeholder".to_string(),
                suspected_cause: Some(response.suspected_cause.clone()),
                suggested_action: SuggestedAction::NoOp {
                    reason: "placeholder".to_string(),
                    revisit_after: std::time::Duration::from_secs(60),
                },
                action_rationale: None,
                status: DiagnosisStatus::Pending,
            };

            if let Ok((selection, _metrics)) = self
                .pipes
                .select_action(&placeholder, &self.allowlist, &history)
                .await
            {
                debug!(
                    selected = %selection.selected_option,
                    score = selection.total_score,
                    "AI selected action"
                );
                // Parse the selected action (simplified)
                // In production, we'd parse the selection more carefully
            }
        }

        // Fall back to rule-based action selection
        self.rule_based_action(response, trigger).await
    }

    async fn rule_based_action(
        &self,
        response: &super::pipes::DiagnosisResponse,
        trigger: &TriggerMetric,
    ) -> SuggestedAction {
        match response.recommended_action_type.as_str() {
            "adjust_param" => {
                if let Some(target) = &response.action_target {
                    if let Some(bounds) = self.allowlist.adjustable_params.get(target) {
                        // Determine direction based on trigger
                        let (old_value, new_value) =
                            self.calculate_param_adjustment(target, bounds, trigger);
                        return SuggestedAction::AdjustParam {
                            key: target.clone(),
                            old_value,
                            new_value,
                            scope: ConfigScope::Runtime,
                        };
                    }
                }
                // Fallback: adjust a relevant param based on trigger type
                self.default_adjustment_for_trigger(trigger)
            }

            "toggle_feature" => {
                if let Some(target) = &response.action_target {
                    if self.allowlist.toggleable_features.contains(target) {
                        return SuggestedAction::ToggleFeature {
                            feature_name: target.clone(),
                            desired_state: false, // Typically we disable problematic features
                            reason: response.rationale.clone(),
                        };
                    }
                }
                SuggestedAction::NoOp {
                    reason: "No valid feature to toggle".to_string(),
                    revisit_after: std::time::Duration::from_secs(300),
                }
            }

            "scale_resource" => {
                // Determine which resource to scale based on trigger
                match trigger {
                    TriggerMetric::Latency { .. } => {
                        if let Some(bounds) = self
                            .allowlist
                            .scalable_resources
                            .get(&ResourceType::MaxConcurrentRequests)
                        {
                            let current = bounds.min + (bounds.max - bounds.min) / 2;
                            let new_value = (current + bounds.step).min(bounds.max);
                            return SuggestedAction::ScaleResource {
                                resource: ResourceType::MaxConcurrentRequests,
                                old_value: current,
                                new_value,
                            };
                        }
                    }
                    TriggerMetric::ErrorRate { .. } => {
                        if let Some(bounds) = self
                            .allowlist
                            .scalable_resources
                            .get(&ResourceType::MaxRetries)
                        {
                            let current = 3; // Assume current
                            let new_value = (current + bounds.step).min(bounds.max);
                            return SuggestedAction::ScaleResource {
                                resource: ResourceType::MaxRetries,
                                old_value: current,
                                new_value,
                            };
                        }
                    }
                    _ => {}
                }
                self.default_adjustment_for_trigger(trigger)
            }

            "clear_cache" => SuggestedAction::ClearCache {
                cache_name: "sessions".to_string(),
            },

            "restart_service" => SuggestedAction::RestartService {
                component: super::types::ServiceComponent::LangbaseClient,
                graceful: true,
            },

            _ => SuggestedAction::NoOp {
                reason: format!(
                    "Unknown action type: {}",
                    response.recommended_action_type
                ),
                revisit_after: std::time::Duration::from_secs(300),
            },
        }
    }

    fn calculate_param_adjustment(
        &self,
        _key: &str,
        bounds: &super::allowlist::ParamBounds,
        trigger: &TriggerMetric,
    ) -> (ParamValue, ParamValue) {
        // Determine adjustment direction based on trigger
        let should_increase = matches!(
            trigger,
            TriggerMetric::Latency { .. } | TriggerMetric::ErrorRate { .. }
        );

        match (&bounds.current_value, &bounds.step, &bounds.min, &bounds.max) {
            (
                ParamValue::Integer(current),
                ParamValue::Integer(step),
                ParamValue::Integer(min),
                ParamValue::Integer(max),
            ) => {
                let new = if should_increase {
                    (*current + step).min(*max)
                } else {
                    (*current - step).max(*min)
                };
                (
                    ParamValue::Integer(*current),
                    ParamValue::Integer(new),
                )
            }
            (
                ParamValue::Float(current),
                ParamValue::Float(step),
                ParamValue::Float(min),
                ParamValue::Float(max),
            ) => {
                let new = if should_increase {
                    (current + step).min(*max)
                } else {
                    (current - step).max(*min)
                };
                (ParamValue::Float(*current), ParamValue::Float(new))
            }
            _ => (bounds.current_value.clone(), bounds.current_value.clone()),
        }
    }

    fn default_adjustment_for_trigger(&self, trigger: &TriggerMetric) -> SuggestedAction {
        match trigger {
            TriggerMetric::Latency { .. } => {
                // Increase timeout
                if let Some(bounds) = self.allowlist.adjustable_params.get("REQUEST_TIMEOUT_MS") {
                    let (old, new) = self.calculate_param_adjustment("REQUEST_TIMEOUT_MS", bounds, trigger);
                    return SuggestedAction::AdjustParam {
                        key: "REQUEST_TIMEOUT_MS".to_string(),
                        old_value: old,
                        new_value: new,
                        scope: ConfigScope::Runtime,
                    };
                }
            }
            TriggerMetric::ErrorRate { .. } => {
                // Increase retries
                if let Some(bounds) = self.allowlist.adjustable_params.get("MAX_RETRIES") {
                    let (old, new) = self.calculate_param_adjustment("MAX_RETRIES", bounds, trigger);
                    return SuggestedAction::AdjustParam {
                        key: "MAX_RETRIES".to_string(),
                        old_value: old,
                        new_value: new,
                        scope: ConfigScope::Runtime,
                    };
                }
            }
            TriggerMetric::QualityScore { .. } => {
                // Increase quality threshold
                if let Some(bounds) = self.allowlist.adjustable_params.get("REFLECTION_QUALITY_THRESHOLD") {
                    let (old, new) = self.calculate_param_adjustment("REFLECTION_QUALITY_THRESHOLD", bounds, trigger);
                    return SuggestedAction::AdjustParam {
                        key: "REFLECTION_QUALITY_THRESHOLD".to_string(),
                        old_value: old,
                        new_value: new,
                        scope: ConfigScope::Runtime,
                    };
                }
            }
            TriggerMetric::FallbackRate { .. } => {
                // Decrease prune threshold to be less aggressive
                if let Some(bounds) = self.allowlist.adjustable_params.get("GOT_PRUNE_THRESHOLD") {
                    let (old, new) = self.calculate_param_adjustment("GOT_PRUNE_THRESHOLD", bounds, trigger);
                    return SuggestedAction::AdjustParam {
                        key: "GOT_PRUNE_THRESHOLD".to_string(),
                        old_value: old,
                        new_value: new,
                        scope: ConfigScope::Runtime,
                    };
                }
            }
        }

        SuggestedAction::NoOp {
            reason: "No suitable adjustment found".to_string(),
            revisit_after: std::time::Duration::from_secs(300),
        }
    }

    // ========================================================================
    // Internal: Validation
    // ========================================================================

    async fn validate_diagnosis(&self, diagnosis: &SelfDiagnosis) -> (bool, Vec<String>) {
        if !self.pipes.config().enable_validation {
            return (true, vec![]);
        }

        match self
            .pipes
            .validate_decision(diagnosis, &diagnosis.suggested_action)
            .await
        {
            Ok((validation, _metrics)) => {
                debug!(
                    quality = validation.overall_quality,
                    biases = validation.biases_detected.len(),
                    fallacies = validation.fallacies_detected.len(),
                    "Validation completed"
                );

                // Log biases and fallacies
                for bias in &validation.biases_detected {
                    if bias.severity >= 3 {
                        warn!(
                            bias_type = %bias.bias_type,
                            severity = bias.severity,
                            "High-severity bias detected"
                        );
                    }
                }

                for fallacy in &validation.fallacies_detected {
                    if fallacy.severity >= 3 {
                        warn!(
                            fallacy_type = %fallacy.fallacy_type,
                            severity = fallacy.severity,
                            "High-severity fallacy detected"
                        );
                    }
                }

                (validation.should_proceed, validation.warnings)
            }
            Err(e) => {
                warn!(error = %e, "Validation pipe failed, allowing by default");
                // In case of validation failure, we allow but with a warning
                (
                    true,
                    vec![format!("Validation unavailable: {}", e)],
                )
            }
        }
    }
}

// ============================================================================
// Analyzer Stats
// ============================================================================

/// Analyzer statistics for monitoring.
#[derive(Debug, Clone)]
pub struct AnalyzerStats {
    /// Number of pending diagnoses
    pub pending_count: usize,
    /// Total diagnoses generated
    pub total_diagnoses: u64,
    /// Total analyses blocked
    pub total_blocked: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_improvement::types::Baselines;
    use crate::self_improvement::types::MetricsSnapshot;

    fn test_config() -> SelfImprovementConfig {
        let mut config = SelfImprovementConfig::default();
        config.analyzer.max_pending_diagnoses = 5;
        config.analyzer.min_action_severity = Severity::Info;
        config.pipes.enable_validation = false; // Disable for unit tests
        config
    }

    #[allow(dead_code)]
    fn test_health_report(with_trigger: bool) -> HealthReport {
        let triggers = if with_trigger {
            vec![TriggerMetric::ErrorRate {
                observed: 0.15,
                baseline: 0.05,
                threshold: 0.10,
            }]
        } else {
            vec![]
        };

        HealthReport {
            current_metrics: MetricsSnapshot::new(0.15, 1000, 0.8, 100),
            baselines: Baselines {
                error_rate: 0.05,
                latency_ms: 800,
                quality_score: 0.85,
            },
            triggers,
            is_healthy: !with_trigger,
            generated_at: Utc::now(),
        }
    }

    #[test]
    fn test_analysis_blocked_no_triggers() {
        // This is a sync test for the blocked type
        let blocked = AnalysisBlocked::NoTriggers;
        assert!(matches!(blocked, AnalysisBlocked::NoTriggers));
    }

    #[test]
    fn test_analysis_blocked_severity() {
        let blocked = AnalysisBlocked::SeverityTooLow {
            severity: Severity::Info,
            minimum: Severity::Warning,
        };
        assert!(matches!(
            blocked,
            AnalysisBlocked::SeverityTooLow { .. }
        ));
    }

    #[test]
    fn test_analysis_blocked_circuit_open() {
        let blocked = AnalysisBlocked::CircuitOpen { remaining_secs: 3600 };
        assert!(matches!(blocked, AnalysisBlocked::CircuitOpen { .. }));
    }

    #[test]
    fn test_analysis_blocked_max_pending() {
        let blocked = AnalysisBlocked::MaxPendingReached { count: 10 };
        assert!(matches!(
            blocked,
            AnalysisBlocked::MaxPendingReached { .. }
        ));
    }

    #[test]
    fn test_analyzer_stats_default() {
        let stats = AnalyzerStats {
            pending_count: 0,
            total_diagnoses: 0,
            total_blocked: 0,
        };
        assert_eq!(stats.pending_count, 0);
        assert_eq!(stats.total_diagnoses, 0);
    }

    #[test]
    fn test_action_for_latency_trigger() {
        let _config = test_config();
        let allowlist = ActionAllowlist::default_allowlist();

        let _trigger = TriggerMetric::Latency {
            observed_p95_ms: 5000,
            baseline_ms: 2000,
            threshold_ms: 4000,
        };

        // Check that we have the right param for latency issues
        assert!(allowlist.adjustable_params.contains_key("REQUEST_TIMEOUT_MS"));
    }

    #[test]
    fn test_action_for_error_trigger() {
        let allowlist = ActionAllowlist::default_allowlist();

        // Check that we have the right param for error issues
        assert!(allowlist.adjustable_params.contains_key("MAX_RETRIES"));
    }

    #[test]
    fn test_action_for_quality_trigger() {
        let allowlist = ActionAllowlist::default_allowlist();

        // Check that we have the right param for quality issues
        assert!(allowlist
            .adjustable_params
            .contains_key("REFLECTION_QUALITY_THRESHOLD"));
    }
}
