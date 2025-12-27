//! Integration tests for the self-improvement system.
//!
//! Tests all phases of the self-improvement loop:
//! - Storage operations (CRUD for baselines, diagnoses, actions, etc.)
//! - CLI commands

use chrono::{Duration, Utc};
use serde_json::json;
use tempfile::TempDir;

use mcp_langbase_reasoning::config::DatabaseConfig;
use mcp_langbase_reasoning::self_improvement::cli::{execute_command, SelfImproveCommands};
use mcp_langbase_reasoning::self_improvement::storage::SelfImprovementStorage;
use mcp_langbase_reasoning::self_improvement::{
    ActionId, ActionOutcome, CircuitBreaker, CircuitBreakerConfig, CircuitState, ConfigScope,
    DiagnosisId, DiagnosisStatus, MetricBaseline, MetricsSnapshot, NormalizedReward, ParamValue,
    RewardBreakdown, RewardWeights, SelfDiagnosis, Severity, SuggestedAction, TriggerMetric,
};
use mcp_langbase_reasoning::storage::SqliteStorage;

// ============================================================================
// Test Utilities
// ============================================================================

async fn create_test_storage() -> (SqliteStorage, TempDir) {
    let dir = tempfile::tempdir().expect("Failed to create temp dir");
    let db_path = dir.path().join("test.db");
    let config = DatabaseConfig {
        path: db_path,
        max_connections: 1,
    };
    let storage = SqliteStorage::new(&config)
        .await
        .expect("Failed to create storage");
    (storage, dir)
}

fn create_si_storage(storage: &SqliteStorage) -> SelfImprovementStorage {
    SelfImprovementStorage::new(storage.pool().clone())
}

fn mock_metrics_snapshot() -> MetricsSnapshot {
    MetricsSnapshot::new(0.02, 150, 0.85, 1000)
}

fn mock_degraded_metrics() -> MetricsSnapshot {
    MetricsSnapshot::new(0.15, 800, 0.55, 1000)
}

fn mock_baseline(metric_name: &str) -> MetricBaseline {
    MetricBaseline {
        metric_name: metric_name.to_string(),
        rolling_avg: 0.02,
        rolling_sample_count: 1000,
        rolling_window_start: None,
        ema_value: 0.02,
        ema_alpha: 0.2,
        warning_threshold: 0.05,
        critical_threshold: 0.10,
        last_updated: Utc::now(),
        is_valid: true,
    }
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

fn mock_reward_breakdown() -> RewardBreakdown {
    RewardBreakdown {
        error_rate_reward: 0.30,
        latency_reward: 0.20,
        quality_reward: 0.15,
        weights: RewardWeights::default(),
    }
}

fn mock_normalized_reward() -> NormalizedReward {
    NormalizedReward {
        value: 0.75,
        breakdown: mock_reward_breakdown(),
        confidence: 0.9,
    }
}

fn mock_circuit_breaker_config() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        failure_threshold: 3,
        success_threshold: 2,
        recovery_timeout_secs: 60,
    }
}

fn mock_action_record(
    diagnosis_id: &DiagnosisId,
) -> mcp_langbase_reasoning::self_improvement::storage::ActionRecord {
    mcp_langbase_reasoning::self_improvement::storage::ActionRecord {
        id: ActionId::new(),
        diagnosis_id: diagnosis_id.clone(),
        action_type: "adjust_param".to_string(),
        action_params: serde_json::to_string(&mock_suggested_action()).unwrap(),
        pre_state: r#"{"timeout_ms": 30000}"#.to_string(),
        post_state: Some(r#"{"timeout_ms": 45000}"#.to_string()),
        metrics_before: serde_json::to_string(&mock_degraded_metrics()).unwrap(),
        metrics_after: Some(serde_json::to_string(&mock_metrics_snapshot()).unwrap()),
        executed_at: Utc::now(),
        verified_at: Some(Utc::now()),
        completed_at: Some(Utc::now()),
        outcome: ActionOutcome::Success,
        rollback_reason: None,
        normalized_reward: Some(0.75),
        reward_breakdown: Some(serde_json::to_string(&mock_reward_breakdown()).unwrap()),
        lessons_learned: Some(
            r#"["Timeout increase effective for high-load periods"]"#.to_string(),
        ),
    }
}

fn mock_pending_action_record(
    diagnosis_id: &DiagnosisId,
) -> mcp_langbase_reasoning::self_improvement::storage::ActionRecord {
    mcp_langbase_reasoning::self_improvement::storage::ActionRecord {
        id: ActionId::new(),
        diagnosis_id: diagnosis_id.clone(),
        action_type: "adjust_param".to_string(),
        action_params: serde_json::to_string(&mock_suggested_action()).unwrap(),
        pre_state: r#"{"timeout_ms": 30000}"#.to_string(),
        post_state: None,
        metrics_before: serde_json::to_string(&mock_degraded_metrics()).unwrap(),
        metrics_after: None,
        executed_at: Utc::now(),
        verified_at: None,
        completed_at: None,
        outcome: ActionOutcome::Pending,
        rollback_reason: None,
        normalized_reward: None,
        reward_breakdown: None,
        lessons_learned: None,
    }
}

// ============================================================================
// Baseline Storage Tests
// ============================================================================

#[tokio::test]
async fn test_save_and_get_baseline() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let baseline = mock_baseline("error_rate");
    si_storage.save_baseline(&baseline).await.unwrap();

    let retrieved = si_storage.get_baseline("error_rate").await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.metric_name, "error_rate");
    assert!((retrieved.rolling_avg - 0.02).abs() < 0.001);
}

#[tokio::test]
async fn test_update_baseline() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let mut baseline = mock_baseline("latency");
    si_storage.save_baseline(&baseline).await.unwrap();

    baseline.rolling_avg = 150.0;
    baseline.ema_value = 155.0;
    si_storage.save_baseline(&baseline).await.unwrap();

    let retrieved = si_storage.get_baseline("latency").await.unwrap().unwrap();
    assert!((retrieved.rolling_avg - 150.0).abs() < 0.001);
}

#[tokio::test]
async fn test_get_nonexistent_baseline() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let result = si_storage.get_baseline("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_get_all_baselines() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    for name in &["error_rate", "latency", "quality_score"] {
        si_storage.save_baseline(&mock_baseline(name)).await.unwrap();
    }

    let baselines = si_storage.get_all_baselines().await.unwrap();
    assert_eq!(baselines.len(), 3);
}

// ============================================================================
// Diagnosis Storage Tests
// ============================================================================

#[tokio::test]
async fn test_save_and_get_diagnosis() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let diagnosis = mock_diagnosis();
    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    let pending = si_storage.get_pending_diagnoses().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].severity, Severity::Warning);
}

#[tokio::test]
async fn test_update_diagnosis_status() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let diagnosis = mock_diagnosis();
    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    si_storage
        .update_diagnosis_status(&diagnosis.id, DiagnosisStatus::Executing)
        .await
        .unwrap();

    let pending = si_storage.get_pending_diagnoses().await.unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn test_multiple_pending_diagnoses() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    for _ in 0..3 {
        let mut diagnosis = mock_diagnosis();
        diagnosis.id = DiagnosisId::new();
        si_storage.save_diagnosis(&diagnosis).await.unwrap();
    }

    let pending = si_storage.get_pending_diagnoses().await.unwrap();
    assert_eq!(pending.len(), 3);
}

// ============================================================================
// Action Storage Tests
// ============================================================================

#[tokio::test]
async fn test_save_and_get_action() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    // First save the diagnosis (foreign key constraint)
    let diagnosis = mock_diagnosis();
    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    let action = mock_action_record(&diagnosis.id);
    si_storage.save_action(&action).await.unwrap();

    let history = si_storage.get_action_history(10).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].action_type, "adjust_param");
}

#[tokio::test]
async fn test_action_history_limit() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    for _ in 0..10 {
        // Create diagnosis first (foreign key)
        let mut diagnosis = mock_diagnosis();
        diagnosis.id = DiagnosisId::new();
        si_storage.save_diagnosis(&diagnosis).await.unwrap();

        si_storage
            .save_action(&mock_action_record(&diagnosis.id))
            .await
            .unwrap();
    }

    let history = si_storage.get_action_history(5).await.unwrap();
    assert_eq!(history.len(), 5);
}

#[tokio::test]
async fn test_update_action_verification() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    // Create diagnosis first (foreign key)
    let diagnosis = mock_diagnosis();
    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    let action = mock_pending_action_record(&diagnosis.id);
    si_storage.save_action(&action).await.unwrap();

    si_storage
        .update_action_verification(
            &action.id,
            &mock_metrics_snapshot(),
            &mock_normalized_reward(),
            ActionOutcome::Success,
            None,
        )
        .await
        .unwrap();

    let history = si_storage.get_action_history(1).await.unwrap();
    assert_eq!(history[0].outcome, ActionOutcome::Success);
}

#[tokio::test]
async fn test_get_actions_since() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let since = Utc::now();

    for _ in 0..3 {
        // Create diagnosis first (foreign key)
        let mut diagnosis = mock_diagnosis();
        diagnosis.id = DiagnosisId::new();
        si_storage.save_diagnosis(&diagnosis).await.unwrap();

        si_storage
            .save_action(&mock_action_record(&diagnosis.id))
            .await
            .unwrap();
    }

    let actions = si_storage
        .get_actions_since(since - Duration::minutes(1))
        .await
        .unwrap();
    assert_eq!(actions.len(), 3);

    let future = Utc::now() + Duration::hours(1);
    let actions = si_storage.get_actions_since(future).await.unwrap();
    assert!(actions.is_empty());
}

// ============================================================================
// Circuit Breaker Storage Tests
// ============================================================================

#[tokio::test]
async fn test_save_and_load_circuit_breaker() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let config = mock_circuit_breaker_config();
    let cb = CircuitBreaker::new(config.clone());
    si_storage.save_circuit_breaker(&cb).await.unwrap();

    let loaded = si_storage.load_circuit_breaker(config).await.unwrap();
    assert_eq!(loaded.summary().state, CircuitState::Closed);
}

#[tokio::test]
async fn test_circuit_breaker_state_persistence() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let config = mock_circuit_breaker_config();
    let mut cb = CircuitBreaker::new(config.clone());
    cb.record_failure();
    cb.record_failure();
    si_storage.save_circuit_breaker(&cb).await.unwrap();

    let loaded = si_storage.load_circuit_breaker(config).await.unwrap();
    assert_eq!(loaded.summary().consecutive_failures, 2);
}

#[tokio::test]
async fn test_load_circuit_breaker_when_none_exists() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let config = mock_circuit_breaker_config();
    let loaded = si_storage.load_circuit_breaker(config).await.unwrap();
    assert_eq!(loaded.summary().state, CircuitState::Closed);
}

// ============================================================================
// Effectiveness Storage Tests
// ============================================================================

#[tokio::test]
async fn test_update_effectiveness_new_record() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    si_storage
        .update_effectiveness("adjust_param", "timeout_ms_increase", 0.75, true, false)
        .await
        .unwrap();

    let records = si_storage.get_effectiveness("adjust_param").await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].total_attempts, 1);
    assert_eq!(records[0].successful_attempts, 1);
}

#[tokio::test]
async fn test_update_effectiveness_multiple_attempts() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    si_storage
        .update_effectiveness("adjust_param", "timeout_ms", 0.8, true, false)
        .await
        .unwrap();
    si_storage
        .update_effectiveness("adjust_param", "timeout_ms", 0.6, true, false)
        .await
        .unwrap();
    si_storage
        .update_effectiveness("adjust_param", "timeout_ms", -0.2, false, false)
        .await
        .unwrap();

    let records = si_storage.get_effectiveness("adjust_param").await.unwrap();
    assert_eq!(records[0].total_attempts, 3);
    assert_eq!(records[0].successful_attempts, 2);
    assert_eq!(records[0].failed_attempts, 1);
}

#[tokio::test]
async fn test_effectiveness_max_min_tracking() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    for reward in &[0.5, 0.9, 0.3, 0.7] {
        si_storage
            .update_effectiveness("test_action", "sig", *reward, true, false)
            .await
            .unwrap();
    }

    let records = si_storage.get_effectiveness("test_action").await.unwrap();
    assert_eq!(records[0].max_reward, Some(0.9));
    assert_eq!(records[0].min_reward, Some(0.3));
}

// ============================================================================
// Runtime Control Storage Tests
// ============================================================================

#[tokio::test]
async fn test_set_system_enabled() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    si_storage.set_system_enabled(true).await.unwrap();
    si_storage.set_system_enabled(false).await.unwrap();
    si_storage.set_system_enabled(true).await.unwrap();
}

#[tokio::test]
async fn test_create_pause() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let ends_at = Utc::now() + Duration::hours(2);
    si_storage
        .create_pause(ends_at, "Maintenance window")
        .await
        .unwrap();
}

#[tokio::test]
async fn test_health_check() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    si_storage.health_check().await.unwrap();
}

// ============================================================================
// CLI Command Tests
// ============================================================================

#[tokio::test]
async fn test_status_command_empty_system() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(SelfImproveCommands::Status, &storage).await;
    assert_eq!(result.exit_code, 0);
    assert!(result.message.contains("Self-Improvement Status"));
}

#[tokio::test]
async fn test_status_command_with_data() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    // Save circuit breaker state
    let config = mock_circuit_breaker_config();
    let cb = CircuitBreaker::new(config);
    si_storage.save_circuit_breaker(&cb).await.unwrap();

    // Save diagnosis
    si_storage.save_diagnosis(&mock_diagnosis()).await.unwrap();

    let result = execute_command(SelfImproveCommands::Status, &storage).await;
    assert_eq!(result.exit_code, 0);
    assert!(result.message.contains("CLOSED"));
}

#[tokio::test]
async fn test_history_command_no_actions() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(
        SelfImproveCommands::History {
            limit: 20,
            outcome: None,
        },
        &storage,
    )
    .await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_history_command_with_actions() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    for _ in 0..5 {
        // Create diagnosis first (foreign key)
        let mut diagnosis = mock_diagnosis();
        diagnosis.id = DiagnosisId::new();
        si_storage.save_diagnosis(&diagnosis).await.unwrap();

        si_storage
            .save_action(&mock_action_record(&diagnosis.id))
            .await
            .unwrap();
    }

    let result = execute_command(
        SelfImproveCommands::History {
            limit: 20,
            outcome: None,
        },
        &storage,
    )
    .await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_config_command() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(SelfImproveCommands::Config, &storage).await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_circuit_breaker_command() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(SelfImproveCommands::CircuitBreaker, &storage).await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_baselines_command() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    for name in &["error_rate", "latency"] {
        si_storage.save_baseline(&mock_baseline(name)).await.unwrap();
    }

    let result = execute_command(SelfImproveCommands::Baselines, &storage).await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_enable_command() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(SelfImproveCommands::Enable, &storage).await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_disable_command() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(SelfImproveCommands::Disable, &storage).await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_pause_command_valid_duration() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(
        SelfImproveCommands::Pause {
            duration: "30m".to_string(),
        },
        &storage,
    )
    .await;
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
async fn test_diagnostics_command() {
    let (storage, _dir) = create_test_storage().await;

    let result = execute_command(
        SelfImproveCommands::Diagnostics { verbose: false },
        &storage,
    )
    .await;
    assert_eq!(result.exit_code, 0);
}

// ============================================================================
// Full Lifecycle Integration Test
// ============================================================================

#[tokio::test]
async fn test_full_action_lifecycle() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    // 1. Save diagnosis
    let diagnosis = mock_diagnosis();
    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    // 2. Update to executing
    si_storage
        .update_diagnosis_status(&diagnosis.id, DiagnosisStatus::Executing)
        .await
        .unwrap();

    // 3. Save pending action
    let action = mock_pending_action_record(&diagnosis.id);
    si_storage.save_action(&action).await.unwrap();

    // 4. Update with verification
    si_storage
        .update_action_verification(
            &action.id,
            &mock_metrics_snapshot(),
            &mock_normalized_reward(),
            ActionOutcome::Success,
            None,
        )
        .await
        .unwrap();

    // 5. Update diagnosis to completed
    si_storage
        .update_diagnosis_status(&diagnosis.id, DiagnosisStatus::Completed)
        .await
        .unwrap();

    // 6. Update effectiveness
    si_storage
        .update_effectiveness("adjust_param", "timeout_ms", 0.75, true, false)
        .await
        .unwrap();

    // Verify final state
    let pending = si_storage.get_pending_diagnoses().await.unwrap();
    assert!(pending.is_empty());

    let history = si_storage.get_action_history(1).await.unwrap();
    assert_eq!(history[0].outcome, ActionOutcome::Success);

    let effectiveness = si_storage.get_effectiveness("adjust_param").await.unwrap();
    assert_eq!(effectiveness.len(), 1);
}

// ============================================================================
// Concurrent Operations Test
// ============================================================================

#[tokio::test]
async fn test_concurrent_baseline_updates() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let mut handles = vec![];
    for i in 0..5 {
        let storage_clone = si_storage.clone();
        let handle = tokio::spawn(async move {
            let mut baseline = mock_baseline("concurrent_metric");
            baseline.rolling_avg = i as f64 * 10.0;
            storage_clone.save_baseline(&baseline).await
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok());
    }

    let baseline = si_storage
        .get_baseline("concurrent_metric")
        .await
        .unwrap();
    assert!(baseline.is_some());
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[tokio::test]
async fn test_special_characters_in_strings() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    let mut diagnosis = mock_diagnosis();
    diagnosis.description =
        "Error: \"connection failed\" with 'quotes' & <special> chars".to_string();
    diagnosis.suspected_cause = Some("Network issue → timeout → retry".to_string());

    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    let pending = si_storage.get_pending_diagnoses().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].description.contains("connection failed"));
}

#[tokio::test]
async fn test_large_action_params() {
    let (storage, _dir) = create_test_storage().await;
    let si_storage = create_si_storage(&storage);

    // Create diagnosis first (foreign key)
    let diagnosis = mock_diagnosis();
    si_storage.save_diagnosis(&diagnosis).await.unwrap();

    let mut action = mock_action_record(&diagnosis.id);

    let large_params = json!({
        "scope": "global",
        "changes": (0..100).map(|i| format!("change_{}", i)).collect::<Vec<_>>(),
        "metadata": {
            "description": "x".repeat(1000),
        }
    });
    action.action_params = serde_json::to_string(&large_params).unwrap();

    si_storage.save_action(&action).await.unwrap();
}
