//! Core types for the self-improvement system.
//!
//! This module defines the fundamental types used across all phases:
//! - [`SelfDiagnosis`]: Complete diagnosis report with trigger, severity, and action
//! - [`SuggestedAction`]: Actions the system can take (config-only, reversible)
//! - [`NormalizedReward`]: Reward calculation for comparing improvements
//! - [`MetricsSnapshot`]: Point-in-time metrics for before/after comparison

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============================================================================
// Identifiers
// ============================================================================

/// Unique identifier for a diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiagnosisId(pub String);

impl DiagnosisId {
    /// Create a new unique diagnosis ID.
    pub fn new() -> Self {
        Self(format!("diag_{}", uuid::Uuid::new_v4()))
    }
}

impl Default for DiagnosisId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for DiagnosisId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ActionId(pub String);

impl ActionId {
    /// Create a new unique action ID.
    pub fn new() -> Self {
        Self(format!("action_{}", uuid::Uuid::new_v4()))
    }
}

impl Default for ActionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ActionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ============================================================================
// Severity and Status Enums
// ============================================================================

/// Severity levels for detected issues.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational - minor deviation, no action needed
    Info = 0,
    /// Warning - moderate deviation, consider action
    Warning = 1,
    /// High - significant deviation, action recommended
    High = 2,
    /// Critical - severe deviation, immediate action required
    Critical = 3,
}

impl Severity {
    /// Determine severity from deviation percentage.
    pub fn from_deviation(deviation_pct: f64) -> Self {
        match deviation_pct {
            d if d >= 100.0 => Severity::Critical,
            d if d >= 50.0 => Severity::High,
            d if d >= 25.0 => Severity::Warning,
            _ => Severity::Info,
        }
    }

    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Severity {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "info" => Ok(Severity::Info),
            "warning" => Ok(Severity::Warning),
            "high" => Ok(Severity::High),
            "critical" => Ok(Severity::Critical),
            _ => Err(format!("Unknown severity: {}", s)),
        }
    }
}

/// Lifecycle status of a diagnosis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosisStatus {
    /// Diagnosis created, awaiting execution
    Pending,
    /// Action is being executed
    Executing,
    /// Action completed successfully
    Completed,
    /// Action was rolled back due to regression
    RolledBack,
    /// Diagnosis superseded by a newer one
    Superseded,
    /// Awaiting manual approval (if configured)
    AwaitingApproval,
}

impl DiagnosisStatus {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosisStatus::Pending => "pending",
            DiagnosisStatus::Executing => "executing",
            DiagnosisStatus::Completed => "completed",
            DiagnosisStatus::RolledBack => "rolled_back",
            DiagnosisStatus::Superseded => "superseded",
            DiagnosisStatus::AwaitingApproval => "awaiting_approval",
        }
    }
}

impl std::fmt::Display for DiagnosisStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for DiagnosisStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(DiagnosisStatus::Pending),
            "executing" => Ok(DiagnosisStatus::Executing),
            "completed" => Ok(DiagnosisStatus::Completed),
            "rolled_back" => Ok(DiagnosisStatus::RolledBack),
            "superseded" => Ok(DiagnosisStatus::Superseded),
            "awaiting_approval" => Ok(DiagnosisStatus::AwaitingApproval),
            _ => Err(format!("Unknown diagnosis status: {}", s)),
        }
    }
}

/// Outcome of an executed action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionOutcome {
    /// Action pending execution
    Pending,
    /// Action completed successfully with positive or neutral reward
    Success,
    /// Action failed to execute
    Failed,
    /// Action was rolled back due to negative reward
    RolledBack,
}

impl ActionOutcome {
    /// Convert to string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionOutcome::Pending => "pending",
            ActionOutcome::Success => "success",
            ActionOutcome::Failed => "failed",
            ActionOutcome::RolledBack => "rolled_back",
        }
    }
}

impl std::fmt::Display for ActionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ActionOutcome {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(ActionOutcome::Pending),
            "success" => Ok(ActionOutcome::Success),
            "failed" => Ok(ActionOutcome::Failed),
            "rolled_back" => Ok(ActionOutcome::RolledBack),
            _ => Err(format!("Unknown action outcome: {}", s)),
        }
    }
}

// ============================================================================
// Trigger Metrics
// ============================================================================

/// What triggered the diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerMetric {
    /// Error rate exceeded threshold
    ErrorRate {
        /// Observed error rate (0.0 - 1.0)
        observed: f64,
        /// Baseline error rate
        baseline: f64,
        /// Threshold that triggered the alert
        threshold: f64,
    },
    /// Latency exceeded threshold
    Latency {
        /// Observed P95 latency in milliseconds
        observed_p95_ms: i64,
        /// Baseline latency in milliseconds
        baseline_ms: i64,
        /// Threshold in milliseconds
        threshold_ms: i64,
    },
    /// Quality score dropped below minimum
    QualityScore {
        /// Observed quality score (0.0 - 1.0)
        observed: f64,
        /// Baseline quality score
        baseline: f64,
        /// Minimum acceptable quality
        minimum: f64,
    },
    /// Fallback rate exceeded threshold
    FallbackRate {
        /// Observed fallback rate (0.0 - 1.0)
        observed: f64,
        /// Baseline fallback rate
        baseline: f64,
        /// Threshold that triggered the alert
        threshold: f64,
    },
}

impl TriggerMetric {
    /// Get the name of the metric.
    pub fn metric_name(&self) -> &'static str {
        match self {
            TriggerMetric::ErrorRate { .. } => "error_rate",
            TriggerMetric::Latency { .. } => "latency_p95",
            TriggerMetric::QualityScore { .. } => "quality_score",
            TriggerMetric::FallbackRate { .. } => "fallback_rate",
        }
    }

    /// Calculate deviation percentage from baseline.
    /// Positive = worse, Negative = better (except for quality where it's inverted).
    pub fn deviation_pct(&self) -> f64 {
        match self {
            TriggerMetric::ErrorRate {
                observed, baseline, ..
            } => {
                if *baseline == 0.0 {
                    if *observed > 0.0 {
                        100.0
                    } else {
                        0.0
                    }
                } else {
                    ((observed - baseline) / baseline) * 100.0
                }
            }
            TriggerMetric::Latency {
                observed_p95_ms,
                baseline_ms,
                ..
            } => {
                if *baseline_ms == 0 {
                    if *observed_p95_ms > 0 {
                        100.0
                    } else {
                        0.0
                    }
                } else {
                    ((*observed_p95_ms - baseline_ms) as f64 / *baseline_ms as f64) * 100.0
                }
            }
            TriggerMetric::QualityScore {
                observed, baseline, ..
            } => {
                // Quality: lower is worse, so invert
                if *baseline == 0.0 {
                    -100.0
                } else {
                    ((baseline - observed) / baseline) * 100.0
                }
            }
            TriggerMetric::FallbackRate {
                observed, baseline, ..
            } => {
                if *baseline == 0.0 {
                    if *observed > 0.0 {
                        100.0
                    } else {
                        0.0
                    }
                } else {
                    ((observed - baseline) / baseline) * 100.0
                }
            }
        }
    }

    /// Get the observed value.
    pub fn observed_value(&self) -> f64 {
        match self {
            TriggerMetric::ErrorRate { observed, .. } => *observed,
            TriggerMetric::Latency { observed_p95_ms, .. } => *observed_p95_ms as f64,
            TriggerMetric::QualityScore { observed, .. } => *observed,
            TriggerMetric::FallbackRate { observed, .. } => *observed,
        }
    }

    /// Get the baseline value.
    pub fn baseline_value(&self) -> f64 {
        match self {
            TriggerMetric::ErrorRate { baseline, .. } => *baseline,
            TriggerMetric::Latency { baseline_ms, .. } => *baseline_ms as f64,
            TriggerMetric::QualityScore { baseline, .. } => *baseline,
            TriggerMetric::FallbackRate { baseline, .. } => *baseline,
        }
    }
}

// ============================================================================
// Suggested Actions
// ============================================================================

/// Parameter value types that can be adjusted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ParamValue {
    /// Integer value
    Integer(i64),
    /// Floating point value
    Float(f64),
    /// String value
    String(String),
    /// Duration in milliseconds
    DurationMs(u64),
    /// Boolean value
    Boolean(bool),
}

impl ParamValue {
    /// Get as integer, if applicable.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            ParamValue::Integer(v) => Some(*v),
            _ => None,
        }
    }

    /// Get as float, if applicable.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ParamValue::Float(v) => Some(*v),
            ParamValue::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }
}

impl std::fmt::Display for ParamValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParamValue::Integer(v) => write!(f, "{}", v),
            ParamValue::Float(v) => write!(f, "{:.2}", v),
            ParamValue::String(v) => write!(f, "{}", v),
            ParamValue::DurationMs(v) => write!(f, "{}ms", v),
            ParamValue::Boolean(v) => write!(f, "{}", v),
        }
    }
}

/// Scope of a configuration change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigScope {
    /// Environment variable
    Environment,
    /// Configuration file
    ConfigFile {
        /// Path to the configuration file
        path: String,
    },
    /// Runtime configuration (in-memory)
    Runtime,
}

/// Service component that can be restarted.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceComponent {
    /// Full server restart
    Full,
    /// Just the Langbase client
    LangbaseClient,
    /// Just the storage layer
    Storage,
    /// A specific reasoning mode
    Mode {
        /// Name of the mode
        name: String,
    },
}

/// Resource types that can be scaled.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    /// Maximum concurrent requests
    MaxConcurrentRequests,
    /// Database connection pool size
    ConnectionPoolSize,
    /// Cache size (number of entries)
    CacheSize,
    /// Request timeout in milliseconds
    TimeoutMs,
    /// Maximum retry attempts
    MaxRetries,
    /// Delay between retries in milliseconds
    RetryDelayMs,
}

/// Actions the self-improvement system can take.
///
/// # Constraints
/// - All actions MUST be reversible (except cache clear and restart)
/// - Config-only (no runtime code changes)
/// - Bounded by ActionAllowlist
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action_type", rename_all = "snake_case")]
pub enum SuggestedAction {
    /// Adjust a numeric configuration parameter
    AdjustParam {
        /// Configuration key
        key: String,
        /// Previous value
        old_value: ParamValue,
        /// New value to set
        new_value: ParamValue,
        /// Scope of the change
        scope: ConfigScope,
    },

    /// Toggle a feature flag
    ToggleFeature {
        /// Feature flag name
        feature_name: String,
        /// Desired state (true = enabled)
        desired_state: bool,
        /// Reason for the toggle
        reason: String,
    },

    /// Restart a service component (graceful)
    RestartService {
        /// Component to restart
        component: ServiceComponent,
        /// Whether to use graceful shutdown
        graceful: bool,
    },

    /// Clear a cache
    ClearCache {
        /// Name of the cache to clear
        cache_name: String,
    },

    /// Scale a resource limit
    ScaleResource {
        /// Type of resource
        resource: ResourceType,
        /// Previous value
        old_value: u32,
        /// New value
        new_value: u32,
    },

    /// Take no action, continue monitoring
    NoOp {
        /// Reason for not taking action
        reason: String,
        /// When to re-evaluate
        #[serde(with = "duration_secs")]
        revisit_after: Duration,
    },
}

/// Serde helper for Duration as seconds
mod duration_secs {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

impl SuggestedAction {
    /// Get the action type as a string.
    pub fn action_type(&self) -> &'static str {
        match self {
            SuggestedAction::AdjustParam { .. } => "adjust_param",
            SuggestedAction::ToggleFeature { .. } => "toggle_feature",
            SuggestedAction::RestartService { .. } => "restart_service",
            SuggestedAction::ClearCache { .. } => "clear_cache",
            SuggestedAction::ScaleResource { .. } => "scale_resource",
            SuggestedAction::NoOp { .. } => "no_op",
        }
    }

    /// Check if the action is reversible.
    pub fn is_reversible(&self) -> bool {
        !matches!(
            self,
            SuggestedAction::ClearCache { .. } | SuggestedAction::RestartService { .. }
        )
    }

    /// Create a NoOp action for when diagnosis fails.
    pub fn no_op_diagnosis_unavailable() -> Self {
        SuggestedAction::NoOp {
            reason: "Diagnosis pipe unavailable".to_string(),
            revisit_after: Duration::from_secs(300),
        }
    }

    /// Create a NoOp action for when circuit breaker is open.
    pub fn no_op_circuit_open() -> Self {
        SuggestedAction::NoOp {
            reason: "Circuit breaker is open".to_string(),
            revisit_after: Duration::from_secs(3600),
        }
    }

    /// Create a NoOp action for when in cooldown.
    pub fn no_op_cooldown(ends_in: Duration) -> Self {
        SuggestedAction::NoOp {
            reason: format!("Cooldown active, {} seconds remaining", ends_in.as_secs()),
            revisit_after: ends_in,
        }
    }
}

// ============================================================================
// Diagnosis
// ============================================================================

/// Complete diagnosis report from the Analyzer phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDiagnosis {
    /// Unique identifier
    pub id: DiagnosisId,
    /// When the diagnosis was created
    pub created_at: DateTime<Utc>,
    /// What triggered this diagnosis
    pub trigger: TriggerMetric,
    /// Severity of the issue
    pub severity: Severity,
    /// Human-readable description
    pub description: String,
    /// Root cause analysis from the pipe
    pub suspected_cause: Option<String>,
    /// Recommended action
    pub suggested_action: SuggestedAction,
    /// Rationale for the suggested action
    pub action_rationale: Option<String>,
    /// Current lifecycle status
    pub status: DiagnosisStatus,
}

impl SelfDiagnosis {
    /// Create a new diagnosis.
    pub fn new(
        trigger: TriggerMetric,
        description: String,
        suggested_action: SuggestedAction,
    ) -> Self {
        let severity = Severity::from_deviation(trigger.deviation_pct().abs());

        Self {
            id: DiagnosisId::new(),
            created_at: Utc::now(),
            trigger,
            severity,
            description,
            suspected_cause: None,
            suggested_action,
            action_rationale: None,
            status: DiagnosisStatus::Pending,
        }
    }
}

// ============================================================================
// Metrics and Rewards
// ============================================================================

/// Point-in-time snapshot of system metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Error rate (0.0 - 1.0)
    pub error_rate: f64,
    /// P95 latency in milliseconds
    pub latency_p95_ms: i64,
    /// Quality score (0.0 - 1.0)
    pub quality_score: f64,
    /// Number of samples in this snapshot
    pub sample_count: u64,
    /// When this snapshot was taken
    pub timestamp: DateTime<Utc>,
}

impl MetricsSnapshot {
    /// Create a new metrics snapshot.
    pub fn new(error_rate: f64, latency_p95_ms: i64, quality_score: f64, sample_count: u64) -> Self {
        Self {
            error_rate,
            latency_p95_ms,
            quality_score,
            sample_count,
            timestamp: Utc::now(),
        }
    }
}

/// Baseline values for reward normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baselines {
    /// Baseline error rate
    pub error_rate: f64,
    /// Baseline latency in milliseconds
    pub latency_ms: i64,
    /// Baseline quality score
    pub quality_score: f64,
}

/// Weights for combining individual metric rewards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardWeights {
    /// Weight for error rate improvement
    pub error_rate: f64,
    /// Weight for latency improvement
    pub latency: f64,
    /// Weight for quality improvement
    pub quality: f64,
}

impl Default for RewardWeights {
    fn default() -> Self {
        Self {
            error_rate: 0.5,
            latency: 0.3,
            quality: 0.2,
        }
    }
}

impl RewardWeights {
    /// Adjust weights based on what triggered the diagnosis.
    pub fn for_trigger(trigger: &TriggerMetric) -> Self {
        match trigger {
            TriggerMetric::ErrorRate { .. } => Self {
                error_rate: 0.7,
                latency: 0.2,
                quality: 0.1,
            },
            TriggerMetric::Latency { .. } => Self {
                error_rate: 0.3,
                latency: 0.6,
                quality: 0.1,
            },
            TriggerMetric::QualityScore { .. } => Self {
                error_rate: 0.3,
                latency: 0.2,
                quality: 0.5,
            },
            TriggerMetric::FallbackRate { .. } => Self {
                error_rate: 0.5,
                latency: 0.3,
                quality: 0.2,
            },
        }
    }
}

/// Breakdown of individual metric rewards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardBreakdown {
    /// Reward from error rate improvement (-1.0 to 1.0)
    pub error_rate_reward: f64,
    /// Reward from latency improvement (-1.0 to 1.0)
    pub latency_reward: f64,
    /// Reward from quality improvement (-1.0 to 1.0)
    pub quality_reward: f64,
    /// Weights used for combination
    pub weights: RewardWeights,
}

/// Normalized reward for comparing improvements across metrics.
///
/// All rewards are in range [-1.0, 1.0]:
/// - Positive = improvement
/// - Negative = regression
/// - Zero = no change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedReward {
    /// Composite reward value
    pub value: f64,
    /// Individual metric contributions
    pub breakdown: RewardBreakdown,
    /// Confidence based on sample size
    pub confidence: f64,
}

impl NormalizedReward {
    /// Calculate normalized reward from before/after metrics.
    pub fn calculate(
        trigger: &TriggerMetric,
        pre_metrics: &MetricsSnapshot,
        post_metrics: &MetricsSnapshot,
        baselines: &Baselines,
    ) -> Self {
        let weights = RewardWeights::for_trigger(trigger);

        // Error rate reward: improvement = decrease
        let error_reward = if baselines.error_rate > 0.0 {
            ((pre_metrics.error_rate - post_metrics.error_rate) / baselines.error_rate)
                .clamp(-1.0, 1.0)
        } else {
            0.0
        };

        // Latency reward: improvement = decrease
        let latency_reward = if baselines.latency_ms > 0 {
            ((pre_metrics.latency_p95_ms - post_metrics.latency_p95_ms) as f64
                / baselines.latency_ms as f64)
                .clamp(-1.0, 1.0)
        } else {
            0.0
        };

        // Quality reward: improvement = increase
        let quality_reward = if baselines.quality_score < 1.0 {
            ((post_metrics.quality_score - pre_metrics.quality_score)
                / (1.0 - baselines.quality_score))
                .clamp(-1.0, 1.0)
        } else {
            0.0
        };

        let composite = weights.error_rate * error_reward
            + weights.latency * latency_reward
            + weights.quality * quality_reward;

        // Confidence based on sample size (minimum 100 samples for full confidence)
        let confidence = (post_metrics.sample_count as f64 / 100.0).min(1.0);

        Self {
            value: composite,
            breakdown: RewardBreakdown {
                error_rate_reward: error_reward,
                latency_reward,
                quality_reward,
                weights,
            },
            confidence,
        }
    }

    /// Check if the reward indicates improvement.
    pub fn is_positive(&self) -> bool {
        self.value > 0.0
    }

    /// Check if the reward indicates regression.
    pub fn is_negative(&self) -> bool {
        self.value < 0.0
    }
}

// ============================================================================
// Health Report
// ============================================================================

/// Aggregated health report from the Monitor phase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Current metrics snapshot
    pub current_metrics: MetricsSnapshot,
    /// Baseline values for comparison
    pub baselines: Baselines,
    /// Detected triggers that exceeded thresholds
    pub triggers: Vec<TriggerMetric>,
    /// Whether the system is healthy overall
    pub is_healthy: bool,
    /// When this report was generated
    pub generated_at: DateTime<Utc>,
}

impl HealthReport {
    /// Check if any triggers were detected.
    pub fn has_triggers(&self) -> bool {
        !self.triggers.is_empty()
    }

    /// Check if action is needed based on triggers.
    ///
    /// Returns true if there are any triggers indicating the system is unhealthy.
    pub fn needs_action(&self) -> bool {
        !self.is_healthy && self.has_triggers()
    }

    /// Get the most severe trigger.
    pub fn most_severe_trigger(&self) -> Option<&TriggerMetric> {
        self.triggers
            .iter()
            .max_by_key(|t| Severity::from_deviation(t.deviation_pct().abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_from_deviation() {
        assert_eq!(Severity::from_deviation(10.0), Severity::Info);
        assert_eq!(Severity::from_deviation(30.0), Severity::Warning);
        assert_eq!(Severity::from_deviation(60.0), Severity::High);
        assert_eq!(Severity::from_deviation(150.0), Severity::Critical);
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn test_trigger_metric_deviation() {
        let trigger = TriggerMetric::ErrorRate {
            observed: 0.10,
            baseline: 0.05,
            threshold: 0.08,
        };
        assert_eq!(trigger.deviation_pct(), 100.0); // 100% increase

        let trigger = TriggerMetric::QualityScore {
            observed: 0.7,
            baseline: 0.8,
            minimum: 0.6,
        };
        // Use approximate comparison for floating point
        assert!((trigger.deviation_pct() - 12.5).abs() < 0.0001); // 12.5% decrease
    }

    #[test]
    fn test_suggested_action_reversibility() {
        let adjust = SuggestedAction::AdjustParam {
            key: "TIMEOUT".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(35000),
            scope: ConfigScope::Runtime,
        };
        assert!(adjust.is_reversible());

        let clear = SuggestedAction::ClearCache {
            cache_name: "sessions".to_string(),
        };
        assert!(!clear.is_reversible());
    }

    #[test]
    fn test_normalized_reward_calculation() {
        let trigger = TriggerMetric::ErrorRate {
            observed: 0.10,
            baseline: 0.05,
            threshold: 0.08,
        };

        let pre = MetricsSnapshot::new(0.10, 1000, 0.80, 100);
        let post = MetricsSnapshot::new(0.05, 900, 0.85, 100);
        let baselines = Baselines {
            error_rate: 0.05,
            latency_ms: 1000,
            quality_score: 0.80,
        };

        let reward = NormalizedReward::calculate(&trigger, &pre, &post, &baselines);
        assert!(reward.is_positive());
        assert!(reward.value > 0.5); // Significant improvement
    }

    #[test]
    fn test_diagnosis_serialization() {
        let diagnosis = SelfDiagnosis::new(
            TriggerMetric::Latency {
                observed_p95_ms: 5000,
                baseline_ms: 2000,
                threshold_ms: 4000,
            },
            "High latency detected".to_string(),
            SuggestedAction::AdjustParam {
                key: "REQUEST_TIMEOUT_MS".to_string(),
                old_value: ParamValue::Integer(30000),
                new_value: ParamValue::Integer(35000),
                scope: ConfigScope::Runtime,
            },
        );

        let json = serde_json::to_string(&diagnosis).unwrap();
        let parsed: SelfDiagnosis = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id.0, diagnosis.id.0);
        assert_eq!(parsed.severity, diagnosis.severity);
    }

    #[test]
    fn test_param_value_display() {
        assert_eq!(ParamValue::Integer(42).to_string(), "42");
        assert_eq!(ParamValue::Float(3.14159).to_string(), "3.14");
        assert_eq!(ParamValue::Boolean(true).to_string(), "true");
        assert_eq!(ParamValue::DurationMs(5000).to_string(), "5000ms");
    }
}
