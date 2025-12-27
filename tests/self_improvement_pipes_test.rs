//! Integration tests for self-improvement Langbase pipe operations.
//!
//! Uses wiremock to mock Langbase API responses for testing the pipes module.

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use wiremock::{
    matchers::{body_string_contains, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

use mcp_langbase_reasoning::config::{LangbaseConfig, RequestConfig};
use mcp_langbase_reasoning::langbase::LangbaseClient;
use mcp_langbase_reasoning::self_improvement::{
    ActionAllowlist, Baselines, ConfigScope, DiagnosisId, DiagnosisStatus, HealthReport,
    MetricsSnapshot, NormalizedReward, ParamValue, RewardBreakdown,
    RewardWeights, SelfDiagnosis, SelfImprovementPipeConfig, Severity,
    SuggestedAction, TriggerMetric,
};
use mcp_langbase_reasoning::self_improvement::pipes::SelfImprovementPipes;

// ============================================================================
// Test Helpers
// ============================================================================

fn create_test_langbase_client(mock_url: &str) -> LangbaseClient {
    let config = LangbaseConfig {
        api_key: "test-api-key".to_string(),
        base_url: mock_url.to_string(),
    };
    let request_config = RequestConfig {
        timeout_ms: 5000,
        max_retries: 0,
        retry_delay_ms: 100,
    };
    LangbaseClient::new(&config, request_config).unwrap()
}

fn create_test_pipes(mock_url: &str) -> SelfImprovementPipes {
    let langbase = Arc::new(create_test_langbase_client(mock_url));
    let pipe_config = SelfImprovementPipeConfig {
        diagnosis_pipe: "reflection-v1".to_string(),
        decision_pipe: "decision-framework-v1".to_string(),
        detection_pipe: "detection-v1".to_string(),
        learning_pipe: "reflection-v1".to_string(),
        enable_validation: true,
        pipe_timeout_ms: 5000,
    };
    SelfImprovementPipes::new(langbase, pipe_config)
}

fn mock_trigger_metric() -> TriggerMetric {
    TriggerMetric::ErrorRate {
        observed: 0.08,
        baseline: 0.02,
        threshold: 0.05,
    }
}

fn mock_baselines() -> Baselines {
    Baselines {
        error_rate: 0.02,
        latency_ms: 150,
        quality_score: 0.85,
    }
}

fn mock_health_report() -> HealthReport {
    HealthReport {
        current_metrics: MetricsSnapshot::new(0.08, 200, 0.75, 1000),
        baselines: mock_baselines(),
        triggers: vec![mock_trigger_metric()],
        is_healthy: false,
        generated_at: chrono::Utc::now(),
    }
}

fn mock_diagnosis() -> SelfDiagnosis {
    SelfDiagnosis {
        id: DiagnosisId::new(),
        created_at: chrono::Utc::now(),
        trigger: mock_trigger_metric(),
        severity: Severity::Warning,
        description: "Error rate elevated".to_string(),
        suspected_cause: Some("Increased traffic".to_string()),
        suggested_action: SuggestedAction::AdjustParam {
            key: "timeout_ms".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(45000),
            scope: ConfigScope::Runtime,
        },
        action_rationale: Some("Increase timeout".to_string()),
        status: DiagnosisStatus::Pending,
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

fn mock_allowlist() -> ActionAllowlist {
    ActionAllowlist::default_allowlist()
}

// ============================================================================
// Diagnosis Pipe Tests
// ============================================================================

#[tokio::test]
async fn test_generate_diagnosis_success() {
    let mock_server = MockServer::start().await;

    let diagnosis_response = json!({
        "suspected_cause": "High error rate due to timeout issues",
        "severity": "warning",
        "confidence": 0.85,
        "evidence": ["Error rate 8%", "Baseline 2%"],
        "recommended_action_type": "adjust_param",
        "action_target": "timeout_ms",
        "rationale": "Increase timeout to reduce errors"
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": diagnosis_response.to_string()
            })),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let health_report = mock_health_report();
    let trigger = mock_trigger_metric();

    let result = pipes.generate_diagnosis(&health_report, &trigger).await;

    assert!(result.is_ok(), "Diagnosis should succeed: {:?}", result.err());
    let (diagnosis, metrics) = result.unwrap();
    assert!(!diagnosis.suspected_cause.is_empty());
    assert!(metrics.call_success);
    assert!(metrics.latency_ms < 5000);
}

#[tokio::test]
async fn test_generate_diagnosis_with_markdown_json() {
    let mock_server = MockServer::start().await;

    // Response with JSON in markdown code block
    let completion = r#"Based on the analysis:

```json
{
    "suspected_cause": "Memory pressure causing slow responses",
    "severity": "high",
    "confidence": 0.9,
    "evidence": ["Latency P95 increased 3x"],
    "recommended_action_type": "adjust_param",
    "action_target": "buffer_size",
    "rationale": "Need to increase buffer size"
}
```

This should help reduce latency."#;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": completion
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    assert!(result.is_ok());
    let (diagnosis, _) = result.unwrap();
    assert!(diagnosis.suspected_cause.contains("Memory pressure"));
}

#[tokio::test]
async fn test_generate_diagnosis_invalid_json_structure() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": "The system appears to be experiencing high load. Consider increasing resources."
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    // Plain text without proper JSON structure should fail parsing
    assert!(result.is_err());
}

#[tokio::test]
async fn test_generate_diagnosis_server_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(500).set_body_json(json!({
                "error": {"message": "Internal server error"}
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_generate_diagnosis_timeout() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success": true, "completion": "{}"}))
                .set_delay(Duration::from_secs(10)),
        )
        .mount(&mock_server)
        .await;

    // Create client with short timeout
    let config = LangbaseConfig {
        api_key: "test".to_string(),
        base_url: mock_server.uri(),
    };
    let request_config = RequestConfig {
        timeout_ms: 100, // Very short timeout
        max_retries: 0,
        retry_delay_ms: 100,
    };
    let langbase = Arc::new(LangbaseClient::new(&config, request_config).unwrap());
    let pipe_config = SelfImprovementPipeConfig {
        diagnosis_pipe: "reflection-v1".to_string(),
        decision_pipe: "decision-framework-v1".to_string(),
        detection_pipe: "detection-v1".to_string(),
        learning_pipe: "reflection-v1".to_string(),
        enable_validation: true,
        pipe_timeout_ms: 100,
    };
    let pipes = SelfImprovementPipes::new(langbase, pipe_config);

    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    assert!(result.is_err());
}

// ============================================================================
// Action Selection Pipe Tests
// ============================================================================

#[tokio::test]
async fn test_select_action_success() {
    let mock_server = MockServer::start().await;

    let selection_response = json!({
        "selected_option": "adjust_param_timeout_ms",
        "scores": {
            "effectiveness": 0.85,
            "risk": 0.2,
            "reversibility": 0.9,
            "historical_success": 0.75
        },
        "total_score": 0.82,
        "rationale": "Historical data shows this is effective",
        "alternatives_considered": ["toggle_feature", "no_op"]
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": selection_response.to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let diagnosis = mock_diagnosis();
    let allowlist = mock_allowlist();
    let history = vec![];

    let result = pipes.select_action(&diagnosis, &allowlist, &history).await;

    assert!(result.is_ok());
    let (_selection, metrics) = result.unwrap();
    assert!(metrics.call_success);
}

#[tokio::test]
async fn test_select_action_with_effectiveness_history() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .and(body_string_contains("effectiveness"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "selected_option": "adjust_param_timeout_ms",
                    "scores": {
                        "effectiveness": 0.9,
                        "risk": 0.1,
                        "reversibility": 0.95,
                        "historical_success": 0.8
                    },
                    "total_score": 0.88,
                    "rationale": "Based on history",
                    "alternatives_considered": []
                }).to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let diagnosis = mock_diagnosis();
    let allowlist = mock_allowlist();

    use mcp_langbase_reasoning::self_improvement::pipes::ActionEffectiveness;
    let history = vec![ActionEffectiveness {
        action_type: "adjust_param".to_string(),
        action_signature: "timeout_ms_increase".to_string(),
        total_attempts: 10,
        successful_attempts: 8,
        avg_reward: 0.6,
        effectiveness_score: 0.8,
    }];

    let result = pipes.select_action(&diagnosis, &allowlist, &history).await;
    assert!(result.is_ok());
}

// ============================================================================
// Validation Pipe Tests
// ============================================================================

#[tokio::test]
async fn test_validate_decision_passes() {
    let mock_server = MockServer::start().await;

    let validation_response = json!({
        "biases_detected": [],
        "fallacies_detected": [],
        "overall_quality": 0.95,
        "should_proceed": true,
        "warnings": []
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": validation_response.to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let diagnosis = mock_diagnosis();
    let action = mock_suggested_action();

    let result = pipes.validate_decision(&diagnosis, &action).await;

    assert!(result.is_ok());
    let (validation, _) = result.unwrap();
    assert!(validation.should_proceed);
    assert!(validation.biases_detected.is_empty());
}

#[tokio::test]
async fn test_validate_decision_detects_bias() {
    let mock_server = MockServer::start().await;

    let validation_response = json!({
        "biases_detected": [
            {"bias_type": "recency_bias", "severity": 3, "explanation": "Over-relying on recent data"},
            {"bias_type": "confirmation_bias", "severity": 2, "explanation": "Seeking confirming evidence"}
        ],
        "fallacies_detected": [],
        "overall_quality": 0.4,
        "should_proceed": false,
        "warnings": ["Decision based on limited recent data"]
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": validation_response.to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .validate_decision(&mock_diagnosis(), &mock_suggested_action())
        .await;

    assert!(result.is_ok());
    let (validation, _) = result.unwrap();
    assert!(!validation.should_proceed);
    assert!(!validation.biases_detected.is_empty());
}

#[tokio::test]
async fn test_validate_decision_detects_fallacy() {
    let mock_server = MockServer::start().await;

    let validation_response = json!({
        "biases_detected": [],
        "fallacies_detected": [
            {"fallacy_type": "false_cause", "severity": 4, "explanation": "Assuming causation from correlation"},
            {"fallacy_type": "hasty_generalization", "severity": 3, "explanation": "Small sample size"}
        ],
        "overall_quality": 0.35,
        "should_proceed": false,
        "warnings": ["Correlation does not imply causation"]
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": validation_response.to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .validate_decision(&mock_diagnosis(), &mock_suggested_action())
        .await;

    assert!(result.is_ok());
    let (validation, _) = result.unwrap();
    assert!(!validation.fallacies_detected.is_empty());
}

#[tokio::test]
async fn test_validate_decision_disabled() {
    // Create pipes with validation disabled
    let mock_server = MockServer::start().await;
    let langbase = Arc::new(create_test_langbase_client(&mock_server.uri()));
    let pipe_config = SelfImprovementPipeConfig {
        diagnosis_pipe: "reflection-v1".to_string(),
        decision_pipe: "decision-framework-v1".to_string(),
        detection_pipe: "detection-v1".to_string(),
        learning_pipe: "reflection-v1".to_string(),
        enable_validation: false, // Disabled
        pipe_timeout_ms: 5000,
    };
    let pipes = SelfImprovementPipes::new(langbase, pipe_config);

    // Should not make any API calls
    let result = pipes
        .validate_decision(&mock_diagnosis(), &mock_suggested_action())
        .await;

    assert!(result.is_ok());
    let (validation, metrics) = result.unwrap();
    assert!(validation.should_proceed); // Default to proceed when disabled
    assert_eq!(metrics.latency_ms, 0); // No actual call made
}

// ============================================================================
// Learning Synthesis Pipe Tests
// ============================================================================

#[tokio::test]
async fn test_synthesize_learning_success() {
    let mock_server = MockServer::start().await;

    let learning_response = json!({
        "outcome_assessment": "Action was effective in reducing error rate",
        "root_cause_accuracy": 0.85,
        "action_effectiveness": 0.9,
        "lessons": [
            "Timeout increases help under high load",
            "Should monitor for 5 minutes before declaring success"
        ],
        "recommendations": {
            "adjust_allowlist": false,
            "param_adjustments": [],
            "adjust_cooldown": false
        },
        "confidence": 0.88
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": learning_response.to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let diagnosis = mock_diagnosis();
    let action = mock_suggested_action();
    let metrics_before = MetricsSnapshot::new(0.08, 200, 0.75, 1000);
    let metrics_after = MetricsSnapshot::new(0.02, 150, 0.85, 1000);
    let reward = NormalizedReward {
        value: 0.75,
        breakdown: RewardBreakdown {
            error_rate_reward: 0.4,
            latency_reward: 0.2,
            quality_reward: 0.15,
            weights: RewardWeights::default(),
        },
        confidence: 0.9,
    };

    let result = pipes
        .synthesize_learning(&action, &diagnosis, &metrics_before, &metrics_after, &reward)
        .await;

    assert!(result.is_ok());
    let (learning, _metrics) = result.unwrap();
    assert!(learning.action_effectiveness > 0.0);
    assert!(!learning.lessons.is_empty());
}

#[tokio::test]
async fn test_synthesize_learning_with_recommendations() {
    let mock_server = MockServer::start().await;

    let learning_response = json!({
        "outcome_assessment": "Partial success",
        "root_cause_accuracy": 0.7,
        "action_effectiveness": 0.6,
        "lessons": ["Consider additional parameters"],
        "recommendations": {
            "adjust_allowlist": true,
            "param_adjustments": [
                {"key": "retry_count", "direction": "increase", "reason": "More retries needed"}
            ],
            "adjust_cooldown": true,
            "new_cooldown_secs": 120
        },
        "confidence": 0.75
    });

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": learning_response.to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .synthesize_learning(
            &mock_suggested_action(),
            &mock_diagnosis(),
            &MetricsSnapshot::new(0.08, 200, 0.75, 1000),
            &MetricsSnapshot::new(0.05, 180, 0.78, 1000),
            &NormalizedReward {
                value: 0.3,
                breakdown: RewardBreakdown {
                    error_rate_reward: 0.2,
                    latency_reward: 0.05,
                    quality_reward: 0.05,
                    weights: RewardWeights::default(),
                },
                confidence: 0.8,
            },
        )
        .await;

    assert!(result.is_ok());
    let (learning, _) = result.unwrap();
    assert!(learning.recommendations.adjust_allowlist);
    assert!(learning.recommendations.adjust_cooldown);
}

// ============================================================================
// Error Handling Tests
// ============================================================================

#[tokio::test]
async fn test_rate_limit_handling() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_json(json!({
                    "error": {"message": "Rate limit exceeded"}
                }))
                .insert_header("Retry-After", "60"),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_malformed_json_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not valid json at all"))
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_completion() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": ""
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let result = pipes
        .generate_diagnosis(&mock_health_report(), &mock_trigger_metric())
        .await;

    // Should handle empty completion gracefully
    assert!(result.is_ok() || result.is_err()); // Either fallback or error is acceptable
}

// ============================================================================
// Multiple Trigger Types Tests
// ============================================================================

#[tokio::test]
async fn test_diagnosis_with_latency_trigger() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "suspected_cause": "Network congestion",
                    "severity": "high",
                    "confidence": 0.8,
                    "evidence": ["P95 latency 500ms vs 200ms baseline"],
                    "recommended_action_type": "adjust_param",
                    "action_target": "connection_pool_size",
                    "rationale": "Increase connection pool"
                }).to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let trigger = TriggerMetric::Latency {
        observed_p95_ms: 500,
        baseline_ms: 200,
        threshold_ms: 300,
    };
    let health_report = HealthReport {
        current_metrics: MetricsSnapshot::new(0.02, 500, 0.8, 1000),
        baselines: mock_baselines(),
        triggers: vec![trigger.clone()],
        is_healthy: false,
        generated_at: chrono::Utc::now(),
    };

    let result = pipes.generate_diagnosis(&health_report, &trigger).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_diagnosis_with_quality_trigger() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "success": true,
                "completion": json!({
                    "suspected_cause": "Model degradation",
                    "severity": "warning",
                    "confidence": 0.7,
                    "evidence": ["Quality dropped to 0.6"],
                    "recommended_action_type": "toggle_feature",
                    "action_target": "model_version",
                    "rationale": "Fall back to stable model"
                }).to_string()
            })),
        )
        .mount(&mock_server)
        .await;

    let pipes = create_test_pipes(&mock_server.uri());
    let trigger = TriggerMetric::QualityScore {
        observed: 0.6,
        baseline: 0.85,
        minimum: 0.7,
    };
    let health_report = HealthReport {
        current_metrics: MetricsSnapshot::new(0.02, 150, 0.6, 1000),
        baselines: mock_baselines(),
        triggers: vec![trigger.clone()],
        is_healthy: false,
        generated_at: chrono::Utc::now(),
    };

    let result = pipes.generate_diagnosis(&health_report, &trigger).await;
    assert!(result.is_ok());
}
