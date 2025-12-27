//! Unit tests for self-improvement module types.
//!
//! Tests the type definitions for analyzer, executor, learner, and system modules.

use chrono::Utc;

use mcp_langbase_reasoning::self_improvement::{
    ActionId, ActionOutcome, Baselines, ConfigScope, DiagnosisId, DiagnosisStatus, HealthReport,
    MetricsSnapshot, NormalizedReward, ParamValue, RewardBreakdown, RewardWeights,
    SelfDiagnosis, Severity, SuggestedAction, TriggerMetric,
};
use mcp_langbase_reasoning::self_improvement::analyzer::{AnalysisBlocked, AnalysisResult};
use mcp_langbase_reasoning::self_improvement::executor::ExecutionBlocked;
use mcp_langbase_reasoning::self_improvement::learner::LearningBlocked;

// ============================================================================
// Test Utilities
// ============================================================================

fn mock_metrics_snapshot() -> MetricsSnapshot {
    MetricsSnapshot::new(0.02, 150, 0.85, 1000)
}

fn mock_degraded_metrics() -> MetricsSnapshot {
    MetricsSnapshot::new(0.15, 800, 0.55, 1000)
}

fn mock_trigger_metric() -> TriggerMetric {
    TriggerMetric::ErrorRate {
        observed: 0.08,
        baseline: 0.02,
        threshold: 0.05,
    }
}

fn mock_suggested_action() -> SuggestedAction {
    SuggestedAction::AdjustParam {
        key: "timeout_ms".to_string(),
        old_value: ParamValue::Integer(30000),
        new_value: ParamValue::Integer(45000),
        scope: ConfigScope::Runtime,
    }
}

fn mock_diagnosis() -> SelfDiagnosis {
    SelfDiagnosis {
        id: DiagnosisId::new(),
        created_at: Utc::now(),
        trigger: mock_trigger_metric(),
        severity: Severity::Warning,
        description: "Error rate elevated above baseline".to_string(),
        suspected_cause: Some("Increased traffic causing timeouts".to_string()),
        suggested_action: mock_suggested_action(),
        action_rationale: Some("Increasing timeout should reduce timeout errors".to_string()),
        status: DiagnosisStatus::Pending,
    }
}

fn mock_health_report_with_triggers() -> HealthReport {
    HealthReport {
        current_metrics: mock_degraded_metrics(),
        baselines: Baselines {
            error_rate: 0.02,
            latency_ms: 150,
            quality_score: 0.85,
        },
        triggers: vec![mock_trigger_metric()],
        is_healthy: false,
        generated_at: Utc::now(),
    }
}

fn mock_healthy_report() -> HealthReport {
    HealthReport {
        current_metrics: mock_metrics_snapshot(),
        baselines: Baselines {
            error_rate: 0.02,
            latency_ms: 150,
            quality_score: 0.85,
        },
        triggers: vec![],
        is_healthy: true,
        generated_at: Utc::now(),
    }
}

// ============================================================================
// Analyzer Type Tests
// ============================================================================

#[test]
fn test_analysis_result_creation() {
    let diagnosis = mock_diagnosis();
    let result = AnalysisResult {
        diagnosis: diagnosis.clone(),
        passed_validation: true,
        validation_warnings: vec!["Minor warning".to_string()],
        circuit_allowed: true,
        analysis_time_ms: 150,
    };

    assert!(result.passed_validation);
    assert!(result.circuit_allowed);
    assert_eq!(result.analysis_time_ms, 150);
    assert_eq!(result.validation_warnings.len(), 1);
}

#[test]
fn test_analysis_result_with_failed_validation() {
    let diagnosis = mock_diagnosis();
    let result = AnalysisResult {
        diagnosis,
        passed_validation: false,
        validation_warnings: vec![
            "Recency bias detected".to_string(),
            "Insufficient evidence".to_string(),
        ],
        circuit_allowed: true,
        analysis_time_ms: 200,
    };

    assert!(!result.passed_validation);
    assert_eq!(result.validation_warnings.len(), 2);
}

#[test]
fn test_analysis_blocked_circuit_open() {
    let blocked = AnalysisBlocked::CircuitOpen { remaining_secs: 60 };
    match blocked {
        AnalysisBlocked::CircuitOpen { remaining_secs } => {
            assert_eq!(remaining_secs, 60);
        }
        _ => panic!("Expected CircuitOpen"),
    }
}

#[test]
fn test_analysis_blocked_no_triggers() {
    let blocked = AnalysisBlocked::NoTriggers;
    assert!(matches!(blocked, AnalysisBlocked::NoTriggers));
}

#[test]
fn test_analysis_blocked_pipe_unavailable() {
    let blocked = AnalysisBlocked::PipeUnavailable {
        pipe: "reflection-v1".to_string(),
        error: "Connection timeout".to_string(),
    };
    match blocked {
        AnalysisBlocked::PipeUnavailable { pipe, error } => {
            assert_eq!(pipe, "reflection-v1");
            assert!(error.contains("timeout"));
        }
        _ => panic!("Expected PipeUnavailable"),
    }
}

#[test]
fn test_analysis_blocked_max_pending() {
    let blocked = AnalysisBlocked::MaxPendingReached { count: 10 };
    match blocked {
        AnalysisBlocked::MaxPendingReached { count } => {
            assert_eq!(count, 10);
        }
        _ => panic!("Expected MaxPendingReached"),
    }
}

#[test]
fn test_analysis_blocked_severity_too_low() {
    let blocked = AnalysisBlocked::SeverityTooLow {
        severity: Severity::Info,
        minimum: Severity::Warning,
    };
    match blocked {
        AnalysisBlocked::SeverityTooLow { severity, minimum } => {
            assert_eq!(severity, Severity::Info);
            assert_eq!(minimum, Severity::Warning);
        }
        _ => panic!("Expected SeverityTooLow"),
    }
}

#[test]
fn test_health_report_has_triggers() {
    let report_with = mock_health_report_with_triggers();
    assert!(report_with.has_triggers());
    assert!(!report_with.is_healthy);

    let report_without = mock_healthy_report();
    assert!(!report_without.has_triggers());
    assert!(report_without.is_healthy);
}

#[test]
fn test_health_report_trigger_count() {
    let report = mock_health_report_with_triggers();
    assert_eq!(report.triggers.len(), 1);
}

// ============================================================================
// Executor Type Tests
// ============================================================================

#[test]
fn test_execution_blocked_circuit_open() {
    let blocked = ExecutionBlocked::CircuitOpen { remaining_secs: 120 };
    match blocked {
        ExecutionBlocked::CircuitOpen { remaining_secs } => {
            assert_eq!(remaining_secs, 120);
        }
        _ => panic!("Expected CircuitOpen"),
    }
}

#[test]
fn test_execution_blocked_not_allowed() {
    let blocked = ExecutionBlocked::NotAllowed {
        reason: "Not in allowlist".to_string(),
    };
    match blocked {
        ExecutionBlocked::NotAllowed { reason } => {
            assert!(reason.contains("allowlist"));
        }
        _ => panic!("Expected NotAllowed"),
    }
}

#[test]
fn test_execution_blocked_cooldown_active() {
    let blocked = ExecutionBlocked::CooldownActive { remaining_secs: 30 };
    match blocked {
        ExecutionBlocked::CooldownActive { remaining_secs } => {
            assert_eq!(remaining_secs, 30);
        }
        _ => panic!("Expected CooldownActive"),
    }
}

#[test]
fn test_execution_blocked_rate_limit() {
    let blocked = ExecutionBlocked::RateLimitExceeded {
        count: 10,
        max: 5,
    };
    match blocked {
        ExecutionBlocked::RateLimitExceeded { count, max } => {
            assert!(count > max);
        }
        _ => panic!("Expected RateLimitExceeded"),
    }
}

// ============================================================================
// Learner Type Tests
// ============================================================================

#[test]
fn test_learning_blocked_insufficient_samples() {
    let blocked = LearningBlocked::InsufficientSamples {
        required: 100,
        actual: 50,
    };
    match blocked {
        LearningBlocked::InsufficientSamples { required, actual } => {
            assert_eq!(required, 100);
            assert_eq!(actual, 50);
        }
        _ => panic!("Expected InsufficientSamples"),
    }
}

#[test]
fn test_learning_blocked_execution_not_completed() {
    let blocked = LearningBlocked::ExecutionNotCompleted {
        status: ActionOutcome::Pending,
    };
    match blocked {
        LearningBlocked::ExecutionNotCompleted { status } => {
            assert_eq!(status, ActionOutcome::Pending);
        }
        _ => panic!("Expected ExecutionNotCompleted"),
    }
}

#[test]
fn test_learning_blocked_pipe_unavailable() {
    let blocked = LearningBlocked::PipeUnavailable {
        message: "Rate limited".to_string(),
    };
    match blocked {
        LearningBlocked::PipeUnavailable { message } => {
            assert!(message.contains("Rate"));
        }
        _ => panic!("Expected PipeUnavailable"),
    }
}

// ============================================================================
// Trigger Metric Tests
// ============================================================================

#[test]
fn test_trigger_metric_error_rate() {
    let trigger = TriggerMetric::ErrorRate {
        observed: 0.08,
        baseline: 0.02,
        threshold: 0.05,
    };

    match trigger {
        TriggerMetric::ErrorRate { observed, baseline, threshold } => {
            assert!(observed > threshold);
            assert!(observed > baseline);
        }
        _ => panic!("Expected ErrorRate"),
    }
}

#[test]
fn test_trigger_metric_latency() {
    let trigger = TriggerMetric::Latency {
        observed_p95_ms: 500,
        baseline_ms: 200,
        threshold_ms: 300,
    };

    match trigger {
        TriggerMetric::Latency { observed_p95_ms, baseline_ms, threshold_ms } => {
            assert!(observed_p95_ms > threshold_ms);
            assert!(observed_p95_ms > baseline_ms);
        }
        _ => panic!("Expected Latency"),
    }
}

#[test]
fn test_trigger_metric_quality_score() {
    let trigger = TriggerMetric::QualityScore {
        observed: 0.6,
        baseline: 0.85,
        minimum: 0.7,
    };

    match trigger {
        TriggerMetric::QualityScore { observed, baseline, minimum } => {
            assert!(observed < minimum);
            assert!(observed < baseline);
        }
        _ => panic!("Expected QualityScore"),
    }
}

// ============================================================================
// Suggested Action Tests
// ============================================================================

#[test]
fn test_suggested_action_adjust_param() {
    let action = SuggestedAction::AdjustParam {
        key: "timeout_ms".to_string(),
        old_value: ParamValue::Integer(30000),
        new_value: ParamValue::Integer(45000),
        scope: ConfigScope::Runtime,
    };

    match action {
        SuggestedAction::AdjustParam { key, old_value, new_value, scope } => {
            assert_eq!(key, "timeout_ms");
            assert_eq!(scope, ConfigScope::Runtime);
            match (old_value, new_value) {
                (ParamValue::Integer(old), ParamValue::Integer(new)) => {
                    assert!(new > old);
                }
                _ => panic!("Expected Integer values"),
            }
        }
        _ => panic!("Expected AdjustParam"),
    }
}

// ============================================================================
// Reward Tests
// ============================================================================

#[test]
fn test_normalized_reward_creation() {
    let reward = NormalizedReward {
        value: 0.75,
        breakdown: RewardBreakdown {
            error_rate_reward: 0.3,
            latency_reward: 0.2,
            quality_reward: 0.25,
            weights: RewardWeights::default(),
        },
        confidence: 0.85,
    };

    assert!(reward.value > 0.0 && reward.value <= 1.0);
    assert!(reward.confidence > 0.0 && reward.confidence <= 1.0);
}

#[test]
fn test_reward_breakdown_components() {
    let breakdown = RewardBreakdown {
        error_rate_reward: 0.4,
        latency_reward: 0.3,
        quality_reward: 0.2,
        weights: RewardWeights::default(),
    };

    assert!(breakdown.error_rate_reward > 0.0);
    assert!(breakdown.latency_reward > 0.0);
    assert!(breakdown.quality_reward > 0.0);
}

// ============================================================================
// Baselines Tests
// ============================================================================

#[test]
fn test_baselines_creation() {
    let baselines = Baselines {
        error_rate: 0.02,
        latency_ms: 150,
        quality_score: 0.85,
    };

    assert!(baselines.error_rate >= 0.0 && baselines.error_rate <= 1.0);
    assert!(baselines.latency_ms > 0);
    assert!(baselines.quality_score >= 0.0 && baselines.quality_score <= 1.0);
}

#[test]
fn test_baselines_comparison_with_metrics() {
    let baselines = Baselines {
        error_rate: 0.02,
        latency_ms: 150,
        quality_score: 0.85,
    };

    let degraded = mock_degraded_metrics();

    // Degraded metrics should be worse than baselines
    assert!(degraded.error_rate > baselines.error_rate);
    assert!(degraded.latency_p95_ms > baselines.latency_ms);
    assert!(degraded.quality_score < baselines.quality_score);
}

// ============================================================================
// Severity Tests
// ============================================================================

#[test]
fn test_severity_ordering() {
    assert!(Severity::Info < Severity::Warning);
    assert!(Severity::Warning < Severity::High);
    assert!(Severity::High < Severity::Critical);
}

#[test]
fn test_severity_equality() {
    assert_eq!(Severity::Warning, Severity::Warning);
    assert_ne!(Severity::Info, Severity::Critical);
}

// ============================================================================
// Diagnosis Status Tests
// ============================================================================

#[test]
fn test_diagnosis_status_variants() {
    let pending = DiagnosisStatus::Pending;
    let executing = DiagnosisStatus::Executing;
    let completed = DiagnosisStatus::Completed;
    let rolled_back = DiagnosisStatus::RolledBack;

    assert_eq!(pending, DiagnosisStatus::Pending);
    assert_eq!(executing, DiagnosisStatus::Executing);
    assert_eq!(completed, DiagnosisStatus::Completed);
    assert_eq!(rolled_back, DiagnosisStatus::RolledBack);
}

// ============================================================================
// Action Outcome Tests
// ============================================================================

#[test]
fn test_action_outcome_variants() {
    let pending = ActionOutcome::Pending;
    let success = ActionOutcome::Success;
    let failed = ActionOutcome::Failed;
    let rolled_back = ActionOutcome::RolledBack;

    assert_eq!(pending, ActionOutcome::Pending);
    assert_eq!(success, ActionOutcome::Success);
    assert_eq!(failed, ActionOutcome::Failed);
    assert_eq!(rolled_back, ActionOutcome::RolledBack);
}

// ============================================================================
// ID Generation Tests
// ============================================================================

#[test]
fn test_diagnosis_id_uniqueness() {
    let id1 = DiagnosisId::new();
    let id2 = DiagnosisId::new();
    assert_ne!(id1, id2);
}

#[test]
fn test_action_id_uniqueness() {
    let id1 = ActionId::new();
    let id2 = ActionId::new();
    assert_ne!(id1, id2);
}

// ============================================================================
// Config Scope Tests
// ============================================================================

#[test]
fn test_config_scope_variants() {
    assert_eq!(ConfigScope::Runtime, ConfigScope::Runtime);
    assert_eq!(ConfigScope::Environment, ConfigScope::Environment);
    assert_eq!(
        ConfigScope::ConfigFile { path: "config.toml".to_string() },
        ConfigScope::ConfigFile { path: "config.toml".to_string() }
    );
    assert_ne!(ConfigScope::Runtime, ConfigScope::Environment);
}

// ============================================================================
// Param Value Tests
// ============================================================================

#[test]
fn test_param_value_integer() {
    let val = ParamValue::Integer(42);
    match val {
        ParamValue::Integer(n) => assert_eq!(n, 42),
        _ => panic!("Expected Integer"),
    }
}

#[test]
fn test_param_value_float() {
    let val = ParamValue::Float(3.14);
    match val {
        ParamValue::Float(f) => assert!((f - 3.14).abs() < 0.001),
        _ => panic!("Expected Float"),
    }
}

#[test]
fn test_param_value_string() {
    let val = ParamValue::String("test".to_string());
    match val {
        ParamValue::String(s) => assert_eq!(s, "test"),
        _ => panic!("Expected String"),
    }
}

#[test]
fn test_param_value_boolean() {
    let val = ParamValue::Boolean(true);
    match val {
        ParamValue::Boolean(b) => assert!(b),
        _ => panic!("Expected Boolean"),
    }
}
