# Test Coverage Improvement Plan

## Current State

| Metric | Current | Target |
|--------|---------|--------|
| **Overall Line Coverage** | 77.27% | 90%+ |
| **Self-Improvement System** | ~25% avg | 85%+ |
| **CLI Module** | 8.24% | 90%+ |
| **Storage (self-improvement)** | 4.19% | 85%+ |

## Priority Modules (Ordered by Impact)

### Phase 1: Self-Improvement Storage (4.19% → 85%+)

**File**: `src/self_improvement/storage.rs`

**Current Issue**: Database operations untested

**Test Strategy**:
```rust
// tests/self_improvement_storage_test.rs

#[cfg(test)]
mod storage_tests {
    use tempfile::tempdir;
    use mcp_langbase_reasoning::self_improvement::*;
    use mcp_langbase_reasoning::storage::SqliteStorage;

    async fn create_test_storage() -> (SqliteStorage, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let storage = SqliteStorage::new_with_path(&db_path).await.unwrap();
        (storage, dir)
    }

    // Test categories:
    // 1. Action record CRUD
    // 2. Diagnosis lifecycle
    // 3. Baseline persistence
    // 4. Circuit breaker state
    // 5. Effectiveness tracking
    // 6. Cooldown period management
}
```

**Tests to Add** (25+ tests):
- `test_save_and_get_action_record`
- `test_update_action_outcome`
- `test_save_diagnosis`
- `test_update_diagnosis_status`
- `test_get_pending_diagnoses`
- `test_save_baseline`
- `test_get_baseline`
- `test_update_baseline`
- `test_save_circuit_breaker_state`
- `test_get_circuit_breaker_state`
- `test_save_action_effectiveness`
- `test_get_action_effectiveness_by_type`
- `test_update_action_effectiveness`
- `test_save_cooldown`
- `test_check_cooldown_active`
- `test_expired_cooldowns`
- `test_get_recent_actions`
- `test_get_actions_by_outcome`
- `test_cascading_deletes`
- `test_concurrent_writes`
- `test_transaction_rollback`

---

### Phase 2: CLI Module (8.24% → 90%+)

**File**: `src/self_improvement/cli.rs`

**Current Issue**: Command handlers need mock storage

**Test Strategy**:
```rust
// tests/self_improvement_cli_test.rs

#[cfg(test)]
mod cli_tests {
    use tempfile::tempdir;
    use mcp_langbase_reasoning::self_improvement::cli::*;
    use mcp_langbase_reasoning::self_improvement::*;

    async fn setup_test_environment() -> TestEnv {
        // Create temp DB with seeded data
    }

    // Test each command with real storage
}
```

**Tests to Add** (15+ tests):
- `test_status_command_empty_system`
- `test_status_command_with_active_diagnosis`
- `test_history_command_no_actions`
- `test_history_command_with_actions`
- `test_history_command_filtered_by_outcome`
- `test_diagnostics_command`
- `test_diagnostics_command_verbose`
- `test_config_command`
- `test_circuit_breaker_command`
- `test_baselines_command`
- `test_enable_command`
- `test_disable_command`
- `test_pause_command_valid_duration`
- `test_pause_command_invalid_duration`
- `test_rollback_command`
- `test_approve_command`
- `test_reject_command`
- `test_format_duration_helper`
- `test_parse_duration_helper`

---

### Phase 3: Pipes Module (25.25% → 85%+)

**File**: `src/self_improvement/pipes.rs`

**Current Issue**: Langbase API calls need mocking

**Test Strategy**: Use `wiremock` (already in project)

```rust
// tests/self_improvement_pipes_test.rs

use wiremock::{MockServer, Mock, ResponseTemplate, matchers::*};

#[tokio::test]
async fn test_diagnose_health_report() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/pipes/run"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "success": true,
            "completion": json!({
                "suspected_cause": "High error rate",
                "severity": "warning",
                "confidence": 0.85,
                "evidence": ["Error rate 5.2%"],
                "recommended_action_type": "adjust_param",
                "rationale": "Reduce timeout"
            }).to_string()
        })))
        .mount(&mock_server)
        .await;

    // Test diagnosis call
}
```

**Tests to Add** (20+ tests):
- `test_diagnose_health_report_success`
- `test_diagnose_health_report_invalid_response`
- `test_diagnose_health_report_timeout`
- `test_diagnose_health_report_server_error`
- `test_select_action_success`
- `test_select_action_no_candidates`
- `test_select_action_with_effectiveness_history`
- `test_validate_decision_passes`
- `test_validate_decision_detects_bias`
- `test_validate_decision_detects_fallacy`
- `test_validate_decision_timeout_fallback`
- `test_synthesize_learning_success`
- `test_synthesize_learning_partial_data`
- `test_pipe_call_metrics_tracking`
- `test_extract_json_from_markdown`
- `test_extract_json_plain`
- `test_fallback_diagnosis_generation`
- `test_fallback_action_selection`
- `test_fallback_validation`
- `test_retry_behavior`

---

### Phase 4: Analyzer Module (14.98% → 85%+)

**File**: `src/self_improvement/analyzer.rs`

**Test Strategy**: Mock `SelfImprovementPipes`

```rust
// Use mockall for pipe mocking
use mockall::mock;

mock! {
    pub SelfImprovementPipes {
        pub async fn diagnose(&self, report: &HealthReport) -> Result<DiagnosisResponse, PipeError>;
        pub async fn select_action(&self, ...) -> Result<ActionSelectionResponse, PipeError>;
        pub async fn validate_decision(&self, ...) -> Result<ValidationResponse, PipeError>;
    }
}
```

**Tests to Add** (15+ tests):
- `test_analyze_healthy_report_no_action`
- `test_analyze_degraded_health_triggers_diagnosis`
- `test_analyze_with_circuit_breaker_open`
- `test_analyze_respects_cooldown`
- `test_action_selection_uses_effectiveness_history`
- `test_validation_rejects_biased_decision`
- `test_validation_accepts_clean_decision`
- `test_validation_timeout_continues`
- `test_blocked_when_max_pending_reached`
- `test_analyzer_stats_tracking`
- `test_consecutive_failure_tracking`
- `test_effectiveness_history_update`
- `test_action_filtering_by_allowlist`
- `test_severity_escalation`
- `test_multi_trigger_analysis`

---

### Phase 5: Learner Module (40.19% → 85%+)

**File**: `src/self_improvement/learner.rs`

**Tests to Add** (15+ tests):
- `test_calculate_reward_improvement`
- `test_calculate_reward_regression`
- `test_calculate_reward_neutral`
- `test_reward_weights_normalization`
- `test_reward_breakdown_calculation`
- `test_normalize_reward_bounds`
- `test_synthesize_lessons_success`
- `test_synthesize_lessons_fallback`
- `test_update_effectiveness_success`
- `test_update_effectiveness_failure`
- `test_update_effectiveness_rollback`
- `test_learner_stats_tracking`
- `test_outcome_determination_success`
- `test_outcome_determination_failure`
- `test_outcome_determination_neutral`

---

### Phase 6: Executor Module (45.88% → 85%+)

**File**: `src/self_improvement/executor.rs`

**Tests to Add** (15+ tests):
- `test_execute_adjust_param_action`
- `test_execute_toggle_feature_action`
- `test_execute_invalid_action_rejected`
- `test_execute_out_of_bounds_rejected`
- `test_config_state_snapshot`
- `test_config_state_restore`
- `test_rollback_on_regression`
- `test_verify_and_complete_success`
- `test_verify_and_complete_failure`
- `test_stabilization_period_wait`
- `test_blocked_by_circuit_breaker`
- `test_blocked_by_cooldown`
- `test_executor_stats_tracking`
- `test_concurrent_execution_prevention`
- `test_pre_post_state_diff`

---

### Phase 7: System Module (23.80% → 85%+)

**File**: `src/self_improvement/system.rs`

**Tests to Add** (10+ tests):
- `test_system_initialization`
- `test_on_invocation_records_metrics`
- `test_check_health_returns_status`
- `test_run_cycle_full_flow`
- `test_run_cycle_blocked_by_circuit_breaker`
- `test_system_enable_disable`
- `test_system_pause_resume`
- `test_concurrent_cycle_prevention`
- `test_cycle_result_tracking`
- `test_error_recovery`

---

### Phase 8: Handler Coverage (45.67% → 80%+)

**File**: `src/server/handlers.rs`

**Tests to Add** (15+ tests):
- `test_handle_linear_tool`
- `test_handle_tree_tool`
- `test_handle_divergent_tool`
- `test_handle_reflection_tool`
- `test_handle_auto_tool`
- `test_handle_got_tool`
- `test_handle_detection_tool`
- `test_handle_decision_tool`
- `test_handle_evidence_tool`
- `test_handle_preset_tool`
- `test_handle_invalid_tool`
- `test_handle_missing_params`
- `test_handle_langbase_error`
- `test_handle_storage_error`
- `test_session_management`

---

### Phase 9: GoT Mode (24.10% → 80%+)

**File**: `src/modes/got.rs`

**Tests to Add** (15+ tests):
- `test_init_graph_session`
- `test_add_node`
- `test_add_edge`
- `test_score_nodes`
- `test_prune_graph`
- `test_merge_nodes`
- `test_extract_solution`
- `test_aggregate_results`
- `test_graph_traversal`
- `test_node_scoring_algorithms`
- `test_pruning_strategies`
- `test_solution_extraction_strategies`
- `test_error_handling`
- `test_concurrent_operations`
- `test_large_graph_performance`

---

## Test Infrastructure

### New Test File Structure

```
tests/
├── config_env_test.rs        (existing)
├── integration_test.rs       (existing)
├── langbase_test.rs          (existing)
├── mcp_protocol_test.rs      (existing)
├── modes_test.rs             (existing)
├── storage_test.rs           (existing)
├── self_improvement/         (NEW)
│   ├── mod.rs
│   ├── storage_test.rs
│   ├── cli_test.rs
│   ├── pipes_test.rs
│   ├── analyzer_test.rs
│   ├── executor_test.rs
│   ├── learner_test.rs
│   └── system_test.rs
├── handlers_test.rs          (NEW)
└── got_test.rs               (NEW)
```

### Test Helpers Module

```rust
// tests/common/mod.rs

pub mod fixtures {
    pub fn mock_health_report() -> HealthReport { ... }
    pub fn mock_diagnosis() -> SelfDiagnosis { ... }
    pub fn mock_action() -> SuggestedAction { ... }
    pub fn mock_metrics() -> MetricsSnapshot { ... }
}

pub mod mocks {
    pub fn mock_langbase_diagnosis_response() -> ResponseTemplate { ... }
    pub fn mock_langbase_action_response() -> ResponseTemplate { ... }
    pub fn mock_langbase_validation_response() -> ResponseTemplate { ... }
}

pub async fn create_test_storage() -> (SqliteStorage, TempDir) { ... }
pub async fn create_test_pipes(mock_url: &str) -> SelfImprovementPipes { ... }
```

### Mock Traits (using mockall)

```rust
// src/self_improvement/pipes.rs - add trait

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PipeOperations {
    async fn diagnose(&self, report: &HealthReport) -> Result<DiagnosisResponse, PipeError>;
    async fn select_action(&self, diagnosis: &SelfDiagnosis, candidates: &[SuggestedAction], history: &[ActionEffectiveness]) -> Result<ActionSelectionResponse, PipeError>;
    async fn validate_decision(&self, diagnosis: &SelfDiagnosis, action: &SuggestedAction) -> Result<ValidationResponse, PipeError>;
    async fn synthesize_learning(&self, context: &LearningContext) -> Result<LearningResponse, PipeError>;
}
```

---

## Execution Plan

### Week 1: Foundation
1. Create `tests/common/mod.rs` with fixtures and helpers
2. Implement storage tests (Phase 1)
3. Run coverage, verify improvement

### Week 2: External Dependencies
4. Implement pipes tests with wiremock (Phase 3)
5. Implement CLI tests (Phase 2)
6. Run coverage, verify improvement

### Week 3: Core Logic
7. Add mockall trait to pipes
8. Implement analyzer tests (Phase 4)
9. Implement learner tests (Phase 5)
10. Run coverage, verify improvement

### Week 4: Remaining Modules
11. Implement executor tests (Phase 6)
12. Implement system tests (Phase 7)
13. Implement handler tests (Phase 8)
14. Implement GoT tests (Phase 9)
15. Final coverage verification

---

## CI Integration

```yaml
# .github/workflows/coverage.yml
name: Coverage

on: [push, pull_request]

jobs:
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: llvm-tools-preview
      - uses: taiki-e/install-action@cargo-llvm-cov
      - run: cargo llvm-cov --all-features --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: true
```

### Coverage Gates

```yaml
# codecov.yml
coverage:
  status:
    project:
      default:
        target: 85%
        threshold: 2%
    patch:
      default:
        target: 80%
```

---

## Dependencies to Add

```toml
# Cargo.toml [dev-dependencies]
mockall = "0.13"        # Already present
wiremock = "0.6"        # Already present
tempfile = "3"          # Already present
tokio-test = "0.4"      # Already present
assert-json-diff = "2"  # Already present
```

No new dependencies required - all testing infrastructure already exists.

---

## Success Metrics

| Module | Before | After | Tests Added |
|--------|--------|-------|-------------|
| storage (self-improvement) | 4.19% | 85%+ | 25+ |
| cli | 8.24% | 90%+ | 15+ |
| pipes | 25.25% | 85%+ | 20+ |
| analyzer | 14.98% | 85%+ | 15+ |
| learner | 40.19% | 85%+ | 15+ |
| executor | 45.88% | 85%+ | 15+ |
| system | 23.80% | 85%+ | 10+ |
| handlers | 45.67% | 80%+ | 15+ |
| got | 24.10% | 80%+ | 15+ |
| **TOTAL** | **77.27%** | **90%+** | **145+** |
