//! Configuration for the self-improvement system.
//!
//! This module provides configuration structures for all phases of the
//! self-improvement loop: Monitor, Analyzer, Executor, and Learner.

use std::time::Duration;

use super::types::Severity;

/// Configuration for the self-improvement system.
#[derive(Debug, Clone)]
pub struct SelfImprovementConfig {
    /// Enable/disable the self-improvement system
    pub enabled: bool,

    /// Monitor configuration
    pub monitor: MonitorConfig,

    /// Analyzer configuration
    pub analyzer: AnalyzerConfig,

    /// Executor configuration
    pub executor: ExecutorConfig,

    /// Learner configuration
    pub learner: LearnerConfig,

    /// Circuit breaker configuration
    pub circuit_breaker: CircuitBreakerConfig,

    /// Baseline calculation configuration
    pub baseline: BaselineConfig,

    /// Pipe configuration for Langbase integration
    pub pipes: SelfImprovementPipeConfig,
}

impl Default for SelfImprovementConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for safety
            monitor: MonitorConfig::default(),
            analyzer: AnalyzerConfig::default(),
            executor: ExecutorConfig::default(),
            learner: LearnerConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            baseline: BaselineConfig::default(),
            pipes: SelfImprovementPipeConfig::default(),
        }
    }
}

impl SelfImprovementConfig {
    /// Load configuration from environment variables.
    pub fn from_env() -> Self {
        let enabled = std::env::var("SELF_IMPROVEMENT_ENABLED")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            enabled,
            monitor: MonitorConfig::from_env(),
            analyzer: AnalyzerConfig::default(),
            executor: ExecutorConfig::from_env(),
            learner: LearnerConfig::default(),
            circuit_breaker: CircuitBreakerConfig::from_env(),
            baseline: BaselineConfig::from_env(),
            pipes: SelfImprovementPipeConfig::from_env(),
        }
    }
}

/// Configuration for the Monitor phase.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    /// How often to check system health (seconds)
    pub check_interval_secs: u64,

    /// Error rate threshold (0.0 - 1.0)
    pub error_rate_threshold: f64,

    /// Latency P95 threshold (milliseconds)
    pub latency_threshold_ms: i64,

    /// Quality score minimum (0.0 - 1.0)
    pub quality_threshold: f64,

    /// Fallback rate threshold (0.0 - 1.0)
    pub fallback_rate_threshold: f64,

    /// Minimum invocations before triggering analysis
    pub min_sample_size: usize,

    /// Time window for aggregating metrics (seconds)
    pub aggregation_window_secs: u64,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 300, // 5 minutes
            error_rate_threshold: 0.05, // 5%
            latency_threshold_ms: 5000, // 5 seconds
            quality_threshold: 0.7,
            fallback_rate_threshold: 0.1, // 10%
            min_sample_size: 50,
            aggregation_window_secs: 3600, // 1 hour
        }
    }
}

impl MonitorConfig {
    /// Load from environment variables.
    pub fn from_env() -> Self {
        Self {
            check_interval_secs: std::env::var("SI_CHECK_INTERVAL_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
            error_rate_threshold: std::env::var("SI_ERROR_RATE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.05),
            latency_threshold_ms: std::env::var("SI_LATENCY_THRESHOLD_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(5000),
            quality_threshold: std::env::var("SI_QUALITY_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.7),
            fallback_rate_threshold: std::env::var("SI_FALLBACK_RATE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1),
            min_sample_size: std::env::var("SI_MIN_SAMPLE_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            aggregation_window_secs: std::env::var("SI_AGGREGATION_WINDOW_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600),
        }
    }
}

/// Configuration for the Analyzer phase.
#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Maximum diagnoses to keep pending
    pub max_pending_diagnoses: usize,

    /// Whether to use reflection pipe for diagnosis
    pub use_reflection_for_diagnosis: bool,

    /// Minimum severity to generate action
    pub min_action_severity: Severity,

    /// Timeout for diagnosis pipe calls (milliseconds)
    pub diagnosis_timeout_ms: u64,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_pending_diagnoses: 10,
            use_reflection_for_diagnosis: true,
            min_action_severity: Severity::Warning,
            diagnosis_timeout_ms: 30000,
        }
    }
}

/// Configuration for the Executor phase.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum actions per hour
    pub max_actions_per_hour: u32,

    /// Cooldown duration after successful action (seconds)
    pub cooldown_duration_secs: u64,

    /// Verification timeout (seconds)
    pub verification_timeout_secs: u64,

    /// Auto-rollback if reward is negative
    pub rollback_on_regression: bool,

    /// Time to wait for metrics to stabilize after change (seconds)
    pub stabilization_period_secs: u64,

    /// Require manual approval for actions
    pub require_approval: bool,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_actions_per_hour: 3,
            cooldown_duration_secs: 3600, // 1 hour
            verification_timeout_secs: 60,
            rollback_on_regression: true,
            stabilization_period_secs: 120, // 2 minutes
            require_approval: false,
        }
    }
}

impl ExecutorConfig {
    /// Load from environment variables.
    pub fn from_env() -> Self {
        Self {
            max_actions_per_hour: std::env::var("SI_MAX_ACTIONS_PER_HOUR")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            cooldown_duration_secs: std::env::var("SI_COOLDOWN_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600),
            verification_timeout_secs: std::env::var("SI_VERIFICATION_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60),
            rollback_on_regression: std::env::var("SI_ROLLBACK_ON_REGRESSION")
                .map(|v| v.to_lowercase() != "false")
                .unwrap_or(true),
            stabilization_period_secs: std::env::var("SI_STABILIZATION_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(120),
            require_approval: std::env::var("SI_REQUIRE_APPROVAL")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
        }
    }

    /// Get cooldown duration as Duration.
    pub fn cooldown_duration(&self) -> Duration {
        Duration::from_secs(self.cooldown_duration_secs)
    }

    /// Get stabilization period as Duration.
    pub fn stabilization_period(&self) -> Duration {
        Duration::from_secs(self.stabilization_period_secs)
    }
}

/// Configuration for the Learner phase.
#[derive(Debug, Clone)]
pub struct LearnerConfig {
    /// Minimum reward to consider action effective
    pub effective_reward_threshold: f64,

    /// Weight for historical effectiveness in action selection
    pub history_weight: f64,

    /// Maximum history entries per action type
    pub max_history_per_action: usize,

    /// Whether to use reflection pipe for learning synthesis
    pub use_reflection_for_learning: bool,
}

impl Default for LearnerConfig {
    fn default() -> Self {
        Self {
            effective_reward_threshold: 0.1,
            history_weight: 0.3,
            max_history_per_action: 100,
            use_reflection_for_learning: true,
        }
    }
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Number of consecutive successes in half-open to close circuit
    pub success_threshold: u32,

    /// Time to wait before attempting recovery (seconds)
    pub recovery_timeout_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 2,
            recovery_timeout_secs: 3600, // 1 hour
        }
    }
}

impl CircuitBreakerConfig {
    /// Load from environment variables.
    pub fn from_env() -> Self {
        Self {
            failure_threshold: std::env::var("SI_CB_FAILURE_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3),
            success_threshold: std::env::var("SI_CB_SUCCESS_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            recovery_timeout_secs: std::env::var("SI_CB_RECOVERY_TIMEOUT_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600),
        }
    }

    /// Get recovery timeout as Duration.
    pub fn recovery_timeout(&self) -> Duration {
        Duration::from_secs(self.recovery_timeout_secs)
    }
}

/// Configuration for baseline calculation.
#[derive(Debug, Clone)]
pub struct BaselineConfig {
    /// EMA smoothing factor (0 < alpha < 1)
    /// Lower = smoother, less responsive
    /// Higher = more responsive, more noise
    pub ema_alpha: f64,

    /// Rolling average window (seconds)
    pub rolling_window_secs: u64,

    /// Minimum samples before baseline is valid
    pub min_samples: usize,

    /// Threshold multiplier for warning (e.g., 1.5 = 50% above baseline)
    pub warning_multiplier: f64,

    /// Threshold multiplier for critical
    pub critical_multiplier: f64,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self {
            ema_alpha: 0.1,
            rolling_window_secs: 86400, // 24 hours
            min_samples: 100,
            warning_multiplier: 1.5,
            critical_multiplier: 2.0,
        }
    }
}

impl BaselineConfig {
    /// Load from environment variables.
    pub fn from_env() -> Self {
        Self {
            ema_alpha: std::env::var("SI_EMA_ALPHA")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.1),
            rolling_window_secs: std::env::var("SI_ROLLING_WINDOW_SECS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(86400),
            min_samples: std::env::var("SI_MIN_SAMPLES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            warning_multiplier: std::env::var("SI_WARNING_MULTIPLIER")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.5),
            critical_multiplier: std::env::var("SI_CRITICAL_MULTIPLIER")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2.0),
        }
    }

    /// Get rolling window as Duration.
    pub fn rolling_window(&self) -> Duration {
        Duration::from_secs(self.rolling_window_secs)
    }
}

/// Configuration for Langbase pipe integration.
#[derive(Debug, Clone)]
pub struct SelfImprovementPipeConfig {
    /// Pipe for diagnosis generation (default: reflection-v1)
    pub diagnosis_pipe: String,

    /// Pipe for action selection (default: decision-framework-v1)
    pub decision_pipe: String,

    /// Pipe for decision validation (default: detection-v1)
    pub detection_pipe: String,

    /// Pipe for learning synthesis (default: reflection-v1)
    pub learning_pipe: String,

    /// Whether to run validation step
    pub enable_validation: bool,

    /// Timeout for pipe calls (milliseconds)
    pub pipe_timeout_ms: u64,
}

impl Default for SelfImprovementPipeConfig {
    fn default() -> Self {
        Self {
            diagnosis_pipe: "reflection-v1".to_string(),
            decision_pipe: "decision-framework-v1".to_string(),
            detection_pipe: "detection-v1".to_string(),
            learning_pipe: "reflection-v1".to_string(),
            enable_validation: true,
            pipe_timeout_ms: 30000,
        }
    }
}

impl SelfImprovementPipeConfig {
    /// Load from environment variables.
    pub fn from_env() -> Self {
        Self {
            diagnosis_pipe: std::env::var("SI_DIAGNOSIS_PIPE")
                .unwrap_or_else(|_| "reflection-v1".to_string()),
            decision_pipe: std::env::var("SI_DECISION_PIPE")
                .unwrap_or_else(|_| "decision-framework-v1".to_string()),
            detection_pipe: std::env::var("SI_DETECTION_PIPE")
                .unwrap_or_else(|_| "detection-v1".to_string()),
            learning_pipe: std::env::var("SI_LEARNING_PIPE")
                .unwrap_or_else(|_| "reflection-v1".to_string()),
            enable_validation: std::env::var("SI_ENABLE_VALIDATION")
                .map(|v| v.to_lowercase() != "false")
                .unwrap_or(true),
            pipe_timeout_ms: std::env::var("SI_PIPE_TIMEOUT_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30000),
        }
    }

    /// Get pipe timeout as Duration.
    pub fn pipe_timeout(&self) -> Duration {
        Duration::from_millis(self.pipe_timeout_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configs() {
        let config = SelfImprovementConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.monitor.check_interval_secs, 300);
        assert_eq!(config.executor.max_actions_per_hour, 3);
        assert_eq!(config.circuit_breaker.failure_threshold, 3);
    }

    #[test]
    fn test_monitor_config_default() {
        let config = MonitorConfig::default();
        assert_eq!(config.error_rate_threshold, 0.05);
        assert_eq!(config.latency_threshold_ms, 5000);
        assert_eq!(config.min_sample_size, 50);
    }

    #[test]
    fn test_executor_duration_helpers() {
        let config = ExecutorConfig::default();
        assert_eq!(config.cooldown_duration(), Duration::from_secs(3600));
        assert_eq!(config.stabilization_period(), Duration::from_secs(120));
    }

    #[test]
    fn test_baseline_config_defaults() {
        let config = BaselineConfig::default();
        assert_eq!(config.ema_alpha, 0.1);
        assert_eq!(config.min_samples, 100);
        assert_eq!(config.warning_multiplier, 1.5);
    }

    #[test]
    fn test_pipe_config_defaults() {
        let config = SelfImprovementPipeConfig::default();
        assert_eq!(config.diagnosis_pipe, "reflection-v1");
        assert_eq!(config.decision_pipe, "decision-framework-v1");
        assert!(config.enable_validation);
    }
}
