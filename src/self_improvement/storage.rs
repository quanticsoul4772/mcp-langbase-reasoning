//! Storage layer for the self-improvement system.
//!
//! This module provides persistence for all self-improvement data including:
//! - Metric baselines (EMA + rolling average)
//! - Diagnoses and their lifecycle
//! - Action execution history
//! - Circuit breaker state
//! - Action effectiveness tracking
//!
//! # Database Tables
//!
//! Uses tables from `migrations/20240109000001_self_improvement_tables.sql`:
//! - `metric_baselines` - Hybrid baseline tracking
//! - `self_diagnoses` - Diagnosis records
//! - `self_improvement_actions` - Action execution history
//! - `circuit_breaker_state` - Circuit breaker persistence
//! - `cooldown_periods` - Cooldown enforcement
//! - `action_effectiveness` - Learning statistics
//! - `pipe_effectiveness` - Pipe quality tracking

use chrono::{DateTime, Utc};
use sqlx::sqlite::SqlitePool;
use sqlx::Row;
use tracing::{debug, warn};

use super::{
    ActionId, ActionOutcome, CircuitBreaker, CircuitBreakerConfig, CircuitBreakerSummary,
    CircuitState, DiagnosisId, DiagnosisStatus, MetricBaseline, MetricsSnapshot, NormalizedReward,
    SelfDiagnosis, Severity, SuggestedAction, TriggerMetric,
};
use crate::error::{StorageError, StorageResult};

// ============================================================================
// Action Record
// ============================================================================

/// Record of an executed action for persistence.
#[derive(Debug, Clone)]
pub struct ActionRecord {
    /// Unique action identifier.
    pub id: ActionId,
    /// Diagnosis that triggered this action.
    pub diagnosis_id: DiagnosisId,
    /// Type of action.
    pub action_type: String,
    /// Full action parameters (JSON).
    pub action_params: String,
    /// Config state before change (JSON).
    pub pre_state: String,
    /// Config state after change (JSON).
    pub post_state: Option<String>,
    /// Metrics before action (JSON).
    pub metrics_before: String,
    /// Metrics after action (JSON).
    pub metrics_after: Option<String>,
    /// When action was executed.
    pub executed_at: DateTime<Utc>,
    /// When action was verified.
    pub verified_at: Option<DateTime<Utc>>,
    /// When learning completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Action outcome.
    pub outcome: ActionOutcome,
    /// Reason for rollback (if rolled back).
    pub rollback_reason: Option<String>,
    /// Normalized reward.
    pub normalized_reward: Option<f64>,
    /// Reward breakdown (JSON).
    pub reward_breakdown: Option<String>,
    /// Lessons learned (JSON).
    pub lessons_learned: Option<String>,
}

// ============================================================================
// Action Effectiveness
// ============================================================================

/// Aggregated effectiveness statistics for an action type.
#[derive(Debug, Clone)]
pub struct ActionEffectivenessRecord {
    /// Action type (e.g., "adjust_param", "toggle_feature").
    pub action_type: String,
    /// Signature hash for grouping similar actions.
    pub action_signature: String,
    /// Total attempts.
    pub total_attempts: u32,
    /// Successful attempts.
    pub successful_attempts: u32,
    /// Failed attempts.
    pub failed_attempts: u32,
    /// Rolled back attempts.
    pub rolled_back_attempts: u32,
    /// Average reward.
    pub avg_reward: f64,
    /// Maximum reward achieved.
    pub max_reward: Option<f64>,
    /// Minimum reward achieved.
    pub min_reward: Option<f64>,
    /// Effectiveness score [0, 1].
    pub effectiveness_score: f64,
    /// First attempt timestamp.
    pub first_attempt: DateTime<Utc>,
    /// Last attempt timestamp.
    pub last_attempt: DateTime<Utc>,
}

// ============================================================================
// SelfImprovementStorage
// ============================================================================

/// Storage operations for the self-improvement system.
///
/// Wraps the SQLite connection pool and provides typed operations
/// for all self-improvement data.
#[derive(Clone)]
pub struct SelfImprovementStorage {
    pool: SqlitePool,
}

impl SelfImprovementStorage {
    /// Create a new storage instance from an existing pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    // ========================================================================
    // Baseline Operations
    // ========================================================================

    /// Get a metric baseline by name.
    pub async fn get_baseline(&self, metric_name: &str) -> StorageResult<Option<MetricBaseline>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, metric_name, rolling_avg_value, rolling_avg_sample_count,
                rolling_avg_window_start, ema_value, ema_alpha,
                warning_threshold, critical_threshold, last_updated, metadata
            FROM metric_baselines
            WHERE metric_name = ?
            "#,
        )
        .bind(metric_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get baseline: {}", e),
        })?;

        match row {
            Some(row) => {
                let baseline = MetricBaseline {
                    metric_name: row.get("metric_name"),
                    rolling_avg: row.get("rolling_avg_value"),
                    rolling_sample_count: row.get::<i64, _>("rolling_avg_sample_count") as usize,
                    rolling_window_start: None, // Not persisted, reconstructed on load
                    ema_value: row.get("ema_value"),
                    ema_alpha: 0.2, // Default alpha, not persisted
                    warning_threshold: row.get("warning_threshold"),
                    critical_threshold: row.get("critical_threshold"),
                    last_updated: parse_timestamp(row.get("last_updated")),
                    is_valid: row.get::<i64, _>("rolling_avg_sample_count") >= 100,
                };
                Ok(Some(baseline))
            }
            None => Ok(None),
        }
    }

    /// Save or update a metric baseline.
    pub async fn save_baseline(&self, baseline: &MetricBaseline) -> StorageResult<()> {
        let id = format!("baseline_{}", baseline.metric_name);

        sqlx::query(
            r#"
            INSERT INTO metric_baselines (
                id, metric_name, rolling_avg_value, rolling_avg_sample_count,
                ema_value, warning_threshold, critical_threshold, last_updated
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(metric_name) DO UPDATE SET
                rolling_avg_value = excluded.rolling_avg_value,
                rolling_avg_sample_count = excluded.rolling_avg_sample_count,
                ema_value = excluded.ema_value,
                warning_threshold = excluded.warning_threshold,
                critical_threshold = excluded.critical_threshold,
                last_updated = excluded.last_updated
            "#,
        )
        .bind(&id)
        .bind(&baseline.metric_name)
        .bind(baseline.rolling_avg)
        .bind(baseline.rolling_sample_count as i64)
        .bind(baseline.ema_value)
        .bind(baseline.warning_threshold)
        .bind(baseline.critical_threshold)
        .bind(baseline.last_updated.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to save baseline: {}", e),
        })?;

        debug!(metric = %baseline.metric_name, "Baseline saved");
        Ok(())
    }

    // ========================================================================
    // Diagnosis Operations
    // ========================================================================

    /// Save a new diagnosis.
    pub async fn save_diagnosis(&self, diagnosis: &SelfDiagnosis) -> StorageResult<()> {
        let trigger_json = serde_json::to_string(&diagnosis.trigger).map_err(|e| {
            StorageError::Serialization {
                message: format!("Failed to serialize trigger: {}", e),
            }
        })?;

        let action_json =
            serde_json::to_string(&diagnosis.suggested_action).map_err(|e| {
                StorageError::Serialization {
                    message: format!("Failed to serialize action: {}", e),
                }
            })?;

        sqlx::query(
            r#"
            INSERT INTO self_diagnoses (
                id, created_at, trigger_metric, trigger_type,
                observed_value, baseline_value, deviation_pct,
                severity, description, suspected_cause,
                suggested_action, action_rationale, status, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(diagnosis.id.to_string())
        .bind(diagnosis.created_at.to_rfc3339())
        .bind(diagnosis.trigger.metric_name())
        .bind(&trigger_json)
        .bind(get_observed_value(&diagnosis.trigger))
        .bind(get_baseline_value(&diagnosis.trigger))
        .bind(diagnosis.trigger.deviation_pct())
        .bind(diagnosis.severity.as_str())
        .bind(&diagnosis.description)
        .bind(&diagnosis.suspected_cause)
        .bind(&action_json)
        .bind(&diagnosis.action_rationale)
        .bind(diagnosis.status.as_str())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to save diagnosis: {}", e),
        })?;

        debug!(id = %diagnosis.id, "Diagnosis saved");
        Ok(())
    }

    /// Get pending diagnoses.
    pub async fn get_pending_diagnoses(&self) -> StorageResult<Vec<SelfDiagnosis>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, created_at, trigger_metric, trigger_type,
                severity, description, suspected_cause,
                suggested_action, action_rationale, status
            FROM self_diagnoses
            WHERE status = 'pending'
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get pending diagnoses: {}", e),
        })?;

        let mut diagnoses = Vec::new();
        for row in rows {
            if let Some(diag) = parse_diagnosis_row(&row) {
                diagnoses.push(diag);
            }
        }

        Ok(diagnoses)
    }

    /// Update diagnosis status.
    pub async fn update_diagnosis_status(
        &self,
        id: &DiagnosisId,
        status: DiagnosisStatus,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE self_diagnoses
            SET status = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to update diagnosis status: {}", e),
        })?;

        debug!(id = %id, status = ?status, "Diagnosis status updated");
        Ok(())
    }

    // ========================================================================
    // Action Operations
    // ========================================================================

    /// Save an action record.
    pub async fn save_action(&self, action: &ActionRecord) -> StorageResult<()> {
        sqlx::query(
            r#"
            INSERT INTO self_improvement_actions (
                id, diagnosis_id, action_type, action_params,
                pre_state, post_state, metrics_before, metrics_after,
                executed_at, verified_at, completed_at,
                outcome, rollback_reason, normalized_reward,
                reward_breakdown, lessons_learned
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(action.id.to_string())
        .bind(action.diagnosis_id.to_string())
        .bind(&action.action_type)
        .bind(&action.action_params)
        .bind(&action.pre_state)
        .bind(&action.post_state)
        .bind(&action.metrics_before)
        .bind(&action.metrics_after)
        .bind(action.executed_at.to_rfc3339())
        .bind(action.verified_at.map(|t| t.to_rfc3339()))
        .bind(action.completed_at.map(|t| t.to_rfc3339()))
        .bind(action.outcome.as_str())
        .bind(&action.rollback_reason)
        .bind(action.normalized_reward)
        .bind(&action.reward_breakdown)
        .bind(&action.lessons_learned)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to save action: {}", e),
        })?;

        debug!(id = %action.id, "Action saved");
        Ok(())
    }

    /// Get action history.
    pub async fn get_action_history(&self, limit: usize) -> StorageResult<Vec<ActionRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, diagnosis_id, action_type, action_params,
                pre_state, post_state, metrics_before, metrics_after,
                executed_at, verified_at, completed_at,
                outcome, rollback_reason, normalized_reward,
                reward_breakdown, lessons_learned
            FROM self_improvement_actions
            ORDER BY executed_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get action history: {}", e),
        })?;

        let mut actions = Vec::new();
        for row in rows {
            if let Some(action) = parse_action_row(&row) {
                actions.push(action);
            }
        }

        Ok(actions)
    }

    /// Update action with verification results.
    pub async fn update_action_verification(
        &self,
        id: &ActionId,
        metrics_after: &MetricsSnapshot,
        reward: &NormalizedReward,
        outcome: ActionOutcome,
        rollback_reason: Option<&str>,
    ) -> StorageResult<()> {
        let metrics_json = serde_json::to_string(metrics_after).map_err(|e| {
            StorageError::Serialization {
                message: format!("Failed to serialize metrics: {}", e),
            }
        })?;

        let reward_json = serde_json::to_string(&reward.breakdown).map_err(|e| {
            StorageError::Serialization {
                message: format!("Failed to serialize reward breakdown: {}", e),
            }
        })?;

        sqlx::query(
            r#"
            UPDATE self_improvement_actions
            SET metrics_after = ?, verified_at = ?,
                outcome = ?, rollback_reason = ?,
                normalized_reward = ?, reward_breakdown = ?
            WHERE id = ?
            "#,
        )
        .bind(&metrics_json)
        .bind(Utc::now().to_rfc3339())
        .bind(outcome.as_str())
        .bind(rollback_reason)
        .bind(reward.value)
        .bind(&reward_json)
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to update action verification: {}", e),
        })?;

        Ok(())
    }

    // ========================================================================
    // Circuit Breaker Operations
    // ========================================================================

    /// Load circuit breaker state.
    pub async fn load_circuit_breaker(
        &self,
        config: CircuitBreakerConfig,
    ) -> StorageResult<CircuitBreaker> {
        let row = sqlx::query(
            r#"
            SELECT
                state, consecutive_failures, consecutive_successes,
                total_failures, total_successes,
                last_failure, last_success, last_state_change
            FROM circuit_breaker_state
            WHERE id = 'main'
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to load circuit breaker: {}", e),
        })?;

        match row {
            Some(row) => {
                let state_str: String = row.get("state");
                let state = match state_str.as_str() {
                    "open" => CircuitState::Open,
                    "half_open" => CircuitState::HalfOpen,
                    _ => CircuitState::Closed,
                };

                Ok(CircuitBreaker::from_db_state(
                    state,
                    row.get::<i64, _>("consecutive_failures") as u32,
                    row.get::<i64, _>("consecutive_successes") as u32,
                    row.get::<i64, _>("total_failures") as u32,
                    row.get::<i64, _>("total_successes") as u32,
                    row.get::<Option<String>, _>("last_failure")
                        .map(|s| parse_timestamp(&s)),
                    row.get::<Option<String>, _>("last_success")
                        .map(|s| parse_timestamp(&s)),
                    parse_timestamp(row.get("last_state_change")),
                    config,
                ))
            }
            None => {
                // No state saved, return new circuit breaker
                Ok(CircuitBreaker::new(config))
            }
        }
    }

    /// Save circuit breaker state.
    pub async fn save_circuit_breaker(&self, cb: &CircuitBreaker) -> StorageResult<()> {
        let summary = cb.summary();

        let state_str = match summary.state {
            CircuitState::Closed => "closed",
            CircuitState::Open => "open",
            CircuitState::HalfOpen => "half_open",
        };

        // Note: CircuitBreakerSummary only has last_state_change, not last_failure/last_success
        // Those fields in the DB schema are kept for potential future use but set to NULL
        sqlx::query(
            r#"
            INSERT INTO circuit_breaker_state (
                id, state, consecutive_failures, consecutive_successes,
                total_failures, total_successes,
                last_failure, last_success, last_state_change
            ) VALUES ('main', ?, ?, ?, ?, ?, NULL, NULL, ?)
            ON CONFLICT(id) DO UPDATE SET
                state = excluded.state,
                consecutive_failures = excluded.consecutive_failures,
                consecutive_successes = excluded.consecutive_successes,
                total_failures = excluded.total_failures,
                total_successes = excluded.total_successes,
                last_state_change = excluded.last_state_change
            "#,
        )
        .bind(state_str)
        .bind(summary.consecutive_failures as i64)
        .bind(summary.consecutive_successes as i64)
        .bind(summary.total_failures as i64)
        .bind(summary.total_successes as i64)
        .bind(summary.last_state_change.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to save circuit breaker: {}", e),
        })?;

        debug!("Circuit breaker state saved");
        Ok(())
    }

    // ========================================================================
    // Effectiveness Operations
    // ========================================================================

    /// Update effectiveness statistics for an action.
    pub async fn update_effectiveness(
        &self,
        action_type: &str,
        signature: &str,
        reward: f64,
        success: bool,
        rolled_back: bool,
    ) -> StorageResult<()> {
        let id = format!("eff_{}_{}", action_type, signature);

        // First, try to get existing record
        let existing = sqlx::query(
            r#"
            SELECT total_attempts, successful_attempts, failed_attempts,
                   rolled_back_attempts, avg_reward, max_reward, min_reward
            FROM action_effectiveness
            WHERE action_type = ? AND action_signature = ?
            "#,
        )
        .bind(action_type)
        .bind(signature)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get effectiveness: {}", e),
        })?;

        let (total, successful, failed, rolled, avg, max_r, min_r) = match existing {
            Some(row) => {
                let total: i64 = row.get("total_attempts");
                let successful: i64 = row.get("successful_attempts");
                let failed: i64 = row.get("failed_attempts");
                let rolled: i64 = row.get("rolled_back_attempts");
                let avg: f64 = row.get("avg_reward");
                let max_r: Option<f64> = row.get("max_reward");
                let min_r: Option<f64> = row.get("min_reward");
                (total, successful, failed, rolled, avg, max_r, min_r)
            }
            None => (0, 0, 0, 0, 0.0, None, None),
        };

        let new_total = total + 1;
        let new_successful = if success { successful + 1 } else { successful };
        let new_failed = if !success && !rolled_back {
            failed + 1
        } else {
            failed
        };
        let new_rolled = if rolled_back { rolled + 1 } else { rolled };
        let new_avg = (avg * total as f64 + reward) / new_total as f64;
        let new_max = Some(max_r.unwrap_or(reward).max(reward));
        let new_min = Some(min_r.unwrap_or(reward).min(reward));

        // Calculate effectiveness score (success rate weighted by reward)
        let success_rate = new_successful as f64 / new_total as f64;
        let effectiveness = success_rate * 0.7 + (new_avg + 1.0) / 2.0 * 0.3;

        sqlx::query(
            r#"
            INSERT INTO action_effectiveness (
                id, action_type, action_signature,
                total_attempts, successful_attempts, failed_attempts, rolled_back_attempts,
                avg_reward, max_reward, min_reward, effectiveness_score,
                first_attempt, last_attempt
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(action_type, action_signature) DO UPDATE SET
                total_attempts = excluded.total_attempts,
                successful_attempts = excluded.successful_attempts,
                failed_attempts = excluded.failed_attempts,
                rolled_back_attempts = excluded.rolled_back_attempts,
                avg_reward = excluded.avg_reward,
                max_reward = excluded.max_reward,
                min_reward = excluded.min_reward,
                effectiveness_score = excluded.effectiveness_score,
                last_attempt = excluded.last_attempt
            "#,
        )
        .bind(&id)
        .bind(action_type)
        .bind(signature)
        .bind(new_total)
        .bind(new_successful)
        .bind(new_failed)
        .bind(new_rolled)
        .bind(new_avg)
        .bind(new_max)
        .bind(new_min)
        .bind(effectiveness)
        .bind(Utc::now().to_rfc3339())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to update effectiveness: {}", e),
        })?;

        debug!(action_type, signature, effectiveness, "Effectiveness updated");
        Ok(())
    }

    /// Get effectiveness for an action type.
    pub async fn get_effectiveness(
        &self,
        action_type: &str,
    ) -> StorageResult<Vec<ActionEffectivenessRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                action_type, action_signature,
                total_attempts, successful_attempts, failed_attempts, rolled_back_attempts,
                avg_reward, max_reward, min_reward, effectiveness_score,
                first_attempt, last_attempt
            FROM action_effectiveness
            WHERE action_type = ?
            ORDER BY effectiveness_score DESC
            "#,
        )
        .bind(action_type)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get effectiveness: {}", e),
        })?;

        let records = rows
            .into_iter()
            .map(|row| ActionEffectivenessRecord {
                action_type: row.get("action_type"),
                action_signature: row.get("action_signature"),
                total_attempts: row.get::<i64, _>("total_attempts") as u32,
                successful_attempts: row.get::<i64, _>("successful_attempts") as u32,
                failed_attempts: row.get::<i64, _>("failed_attempts") as u32,
                rolled_back_attempts: row.get::<i64, _>("rolled_back_attempts") as u32,
                avg_reward: row.get("avg_reward"),
                max_reward: row.get("max_reward"),
                min_reward: row.get("min_reward"),
                effectiveness_score: row.get("effectiveness_score"),
                first_attempt: parse_timestamp(row.get("first_attempt")),
                last_attempt: parse_timestamp(row.get("last_attempt")),
            })
            .collect();

        Ok(records)
    }

    // ========================================================================
    // CLI Support Operations
    // ========================================================================

    /// Health check - verify database connectivity.
    pub async fn health_check(&self) -> StorageResult<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StorageError::Connection {
                message: format!("Health check failed: {}", e),
            })?;
        Ok(())
    }

    /// Get all metric baselines.
    pub async fn get_all_baselines(&self) -> StorageResult<Vec<MetricBaseline>> {
        let rows = sqlx::query(
            r#"
            SELECT
                id, metric_name, rolling_avg_value, rolling_avg_sample_count,
                rolling_avg_window_start, ema_value, ema_alpha,
                warning_threshold, critical_threshold, last_updated, metadata
            FROM metric_baselines
            ORDER BY metric_name
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get all baselines: {}", e),
        })?;

        let mut baselines = Vec::new();
        for row in rows {
            let baseline = MetricBaseline {
                metric_name: row.get("metric_name"),
                rolling_avg: row.get("rolling_avg_value"),
                rolling_sample_count: row.get::<i64, _>("rolling_avg_sample_count") as usize,
                rolling_window_start: None,
                ema_value: row.get("ema_value"),
                ema_alpha: row.get::<Option<f64>, _>("ema_alpha").unwrap_or(0.2),
                warning_threshold: row.get("warning_threshold"),
                critical_threshold: row.get("critical_threshold"),
                last_updated: row
                    .get::<Option<String>, _>("last_updated")
                    .map(|s| parse_timestamp(&s))
                    .unwrap_or_else(Utc::now),
                is_valid: row.get::<i64, _>("rolling_avg_sample_count") >= 10,
            };
            baselines.push(baseline);
        }

        Ok(baselines)
    }

    /// Get actions since a given timestamp.
    pub async fn get_actions_since(
        &self,
        since: DateTime<Utc>,
    ) -> StorageResult<Vec<ActionRecordSummary>> {
        let since_str = since.to_rfc3339();

        let rows = sqlx::query(
            r#"
            SELECT
                id, diagnosis_id, action_type, action_params,
                pre_state, post_state, executed_at, outcome,
                rollback_reason, normalized_reward
            FROM self_improvement_actions
            WHERE executed_at >= ?
            ORDER BY executed_at DESC
            "#,
        )
        .bind(&since_str)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get actions since: {}", e),
        })?;

        let mut records = Vec::new();
        for row in rows {
            let outcome_str: String = row.get("outcome");
            let outcome = outcome_str.parse::<ActionOutcome>().ok();

            // Extract resource and values from action_params JSON
            let action_params: String = row.get("action_params");
            let params: serde_json::Value =
                serde_json::from_str(&action_params).unwrap_or(serde_json::Value::Null);

            let resource = params
                .get("resource")
                .and_then(|v| v.as_str())
                .map(String::from);
            let old_value = params
                .get("old_value")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    params
                        .get("current_value")
                        .and_then(|v| serde_json::to_string(v).ok())
                });
            let new_value = params
                .get("new_value")
                .and_then(|v| v.as_str())
                .map(String::from)
                .or_else(|| {
                    params
                        .get("proposed_value")
                        .and_then(|v| serde_json::to_string(v).ok())
                });
            let scope = params
                .get("scope")
                .and_then(|v| v.as_str())
                .map(String::from);

            records.push(ActionRecordSummary {
                id: ActionId(row.get("id")),
                diagnosis_id: DiagnosisId(row.get("diagnosis_id")),
                action_type: row.get("action_type"),
                resource,
                scope,
                old_value,
                new_value,
                executed_at: parse_timestamp(row.get("executed_at")),
                outcome,
                error_message: row.get("rollback_reason"),
                reward: row.get("normalized_reward"),
            });
        }

        Ok(records)
    }

    /// Load circuit breaker state summary for CLI display.
    pub async fn load_circuit_breaker_state(&self) -> StorageResult<Option<CircuitBreakerSummary>> {
        let row = sqlx::query(
            r#"
            SELECT
                state, consecutive_failures, consecutive_successes,
                total_failures, total_successes, last_state_change
            FROM circuit_breaker_state
            WHERE id = 'main'
            "#,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to load circuit breaker state: {}", e),
        })?;

        match row {
            Some(row) => {
                let state_str: String = row.get("state");
                let state = match state_str.as_str() {
                    "open" => CircuitState::Open,
                    "half_open" => CircuitState::HalfOpen,
                    _ => CircuitState::Closed,
                };

                let last_state_change_str: String = row.get("last_state_change");
                let last_state_change = parse_timestamp(&last_state_change_str);

                Ok(Some(CircuitBreakerSummary {
                    state,
                    consecutive_failures: row.get::<i64, _>("consecutive_failures") as u32,
                    consecutive_successes: row.get::<i64, _>("consecutive_successes") as u32,
                    total_failures: row.get::<i64, _>("total_failures") as u32,
                    total_successes: row.get::<i64, _>("total_successes") as u32,
                    time_until_recovery: None, // Would need recovery_timeout from config
                    last_state_change,
                }))
            }
            None => Ok(None),
        }
    }

    // ========================================================================
    // Runtime Control Operations (for CLI commands)
    // ========================================================================

    /// Create a pause period for the self-improvement system.
    pub async fn create_pause(
        &self,
        ends_at: DateTime<Utc>,
        reason: &str,
    ) -> StorageResult<()> {
        let id = format!("pause_{}", Utc::now().timestamp_millis());
        let ends_at_str = ends_at.to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO cooldown_periods (id, started_at, ends_at, reason, is_active)
            VALUES (?, datetime('now'), ?, ?, 1)
            "#,
        )
        .bind(&id)
        .bind(&ends_at_str)
        .bind(reason)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to create pause: {}", e),
        })?;

        debug!(id = %id, ends_at = %ends_at, "Pause created");
        Ok(())
    }

    /// Get a specific action by ID.
    pub async fn get_action(&self, action_id: &ActionId) -> StorageResult<Option<ActionRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, diagnosis_id, action_type, action_params,
                pre_state, post_state, metrics_before, metrics_after,
                executed_at, verified_at, completed_at,
                outcome, rollback_reason, normalized_reward,
                reward_breakdown, lessons_learned
            FROM self_improvement_actions
            WHERE id = ?
            "#,
        )
        .bind(&action_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get action: {}", e),
        })?;

        match row {
            Some(row) => {
                let outcome_str: String = row.get("outcome");
                let outcome = outcome_str.parse::<ActionOutcome>().unwrap_or(ActionOutcome::Pending);

                Ok(Some(ActionRecord {
                    id: ActionId(row.get("id")),
                    diagnosis_id: DiagnosisId(row.get("diagnosis_id")),
                    action_type: row.get("action_type"),
                    action_params: row.get("action_params"),
                    pre_state: row.get("pre_state"),
                    post_state: row.get("post_state"),
                    metrics_before: row.get("metrics_before"),
                    metrics_after: row.get("metrics_after"),
                    executed_at: parse_timestamp(row.get("executed_at")),
                    verified_at: row
                        .get::<Option<String>, _>("verified_at")
                        .map(|s| parse_timestamp(&s)),
                    completed_at: row
                        .get::<Option<String>, _>("completed_at")
                        .map(|s| parse_timestamp(&s)),
                    outcome,
                    rollback_reason: row.get("rollback_reason"),
                    normalized_reward: row.get("normalized_reward"),
                    reward_breakdown: row.get("reward_breakdown"),
                    lessons_learned: row.get("lessons_learned"),
                }))
            }
            None => Ok(None),
        }
    }

    /// Rollback an action and update its status.
    pub async fn rollback_action(
        &self,
        action_id: &ActionId,
        reason: &str,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE self_improvement_actions
            SET outcome = 'rolled_back',
                rollback_reason = ?,
                completed_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(reason)
        .bind(&action_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to rollback action: {}", e),
        })?;

        debug!(action_id = %action_id.0, reason = reason, "Action rolled back");
        Ok(())
    }

    /// Get a specific diagnosis by ID.
    pub async fn get_diagnosis(
        &self,
        diagnosis_id: &DiagnosisId,
    ) -> StorageResult<Option<SelfDiagnosis>> {
        let row = sqlx::query(
            r#"
            SELECT
                id, created_at, trigger_type, severity, description,
                suspected_cause, suggested_action, action_rationale, status
            FROM self_diagnoses
            WHERE id = ?
            "#,
        )
        .bind(&diagnosis_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to get diagnosis: {}", e),
        })?;

        match row {
            Some(row) => Ok(parse_diagnosis_row(&row)),
            None => Ok(None),
        }
    }

    /// Reject a diagnosis with a reason.
    pub async fn reject_diagnosis(
        &self,
        diagnosis_id: &DiagnosisId,
        reason: &str,
    ) -> StorageResult<()> {
        sqlx::query(
            r#"
            UPDATE self_diagnoses
            SET status = 'superseded',
                action_rationale = COALESCE(action_rationale, '') || ' [Rejected: ' || ? || ']'
            WHERE id = ?
            "#,
        )
        .bind(reason)
        .bind(&diagnosis_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| StorageError::Query {
            message: format!("Failed to reject diagnosis: {}", e),
        })?;

        debug!(diagnosis_id = %diagnosis_id.0, reason = reason, "Diagnosis rejected");
        Ok(())
    }
}

// ============================================================================
// CLI Support Types
// ============================================================================

/// Simplified action record for CLI display.
#[derive(Debug, Clone)]
pub struct ActionRecordSummary {
    /// Unique action identifier.
    pub id: ActionId,
    /// Diagnosis that triggered this action.
    pub diagnosis_id: DiagnosisId,
    /// Type of action.
    pub action_type: String,
    /// Resource being modified.
    pub resource: Option<String>,
    /// Scope of the change.
    pub scope: Option<String>,
    /// Old value (before change).
    pub old_value: Option<String>,
    /// New value (after change).
    pub new_value: Option<String>,
    /// When action was executed.
    pub executed_at: DateTime<Utc>,
    /// Action outcome.
    pub outcome: Option<ActionOutcome>,
    /// Error message if failed.
    pub error_message: Option<String>,
    /// Normalized reward.
    pub reward: Option<f64>,
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_timestamp(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| {
            warn!(timestamp = s, "Failed to parse timestamp, using current time");
            Utc::now()
        })
}

fn get_observed_value(trigger: &TriggerMetric) -> f64 {
    match trigger {
        TriggerMetric::ErrorRate { observed, .. } => *observed,
        TriggerMetric::Latency { observed_p95_ms, .. } => *observed_p95_ms as f64,
        TriggerMetric::QualityScore { observed, .. } => *observed,
        TriggerMetric::FallbackRate { observed, .. } => *observed,
    }
}

fn get_baseline_value(trigger: &TriggerMetric) -> f64 {
    match trigger {
        TriggerMetric::ErrorRate { baseline, .. } => *baseline,
        TriggerMetric::Latency { baseline_ms, .. } => *baseline_ms as f64,
        TriggerMetric::QualityScore { baseline, .. } => *baseline,
        TriggerMetric::FallbackRate { baseline, .. } => *baseline,
    }
}

fn parse_diagnosis_row(row: &sqlx::sqlite::SqliteRow) -> Option<SelfDiagnosis> {
    let trigger_json: String = row.get("trigger_type");
    let action_json: String = row.get("suggested_action");
    let severity_str: String = row.get("severity");
    let status_str: String = row.get("status");

    let trigger: TriggerMetric = serde_json::from_str(&trigger_json).ok()?;
    let action: SuggestedAction = serde_json::from_str(&action_json).ok()?;
    let severity = severity_str.parse::<Severity>().ok()?;
    let status = status_str.parse::<DiagnosisStatus>().ok()?;

    Some(SelfDiagnosis {
        id: DiagnosisId(row.get("id")),
        created_at: parse_timestamp(row.get("created_at")),
        trigger,
        severity,
        description: row.get("description"),
        suspected_cause: row.get("suspected_cause"),
        suggested_action: action,
        action_rationale: row.get("action_rationale"),
        status,
    })
}

fn parse_action_row(row: &sqlx::sqlite::SqliteRow) -> Option<ActionRecord> {
    let outcome_str: String = row.get("outcome");
    let outcome = outcome_str.parse::<ActionOutcome>().ok()?;

    Some(ActionRecord {
        id: ActionId(row.get("id")),
        diagnosis_id: DiagnosisId(row.get("diagnosis_id")),
        action_type: row.get("action_type"),
        action_params: row.get("action_params"),
        pre_state: row.get("pre_state"),
        post_state: row.get("post_state"),
        metrics_before: row.get("metrics_before"),
        metrics_after: row.get("metrics_after"),
        executed_at: parse_timestamp(row.get("executed_at")),
        verified_at: row
            .get::<Option<String>, _>("verified_at")
            .map(|s| parse_timestamp(&s)),
        completed_at: row
            .get::<Option<String>, _>("completed_at")
            .map(|s| parse_timestamp(&s)),
        outcome,
        rollback_reason: row.get("rollback_reason"),
        normalized_reward: row.get("normalized_reward"),
        reward_breakdown: row.get("reward_breakdown"),
        lessons_learned: row.get("lessons_learned"),
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_record_creation() {
        let record = ActionRecord {
            id: ActionId::new(),
            diagnosis_id: DiagnosisId::new(),
            action_type: "adjust_param".to_string(),
            action_params: "{}".to_string(),
            pre_state: "{}".to_string(),
            post_state: None,
            metrics_before: "{}".to_string(),
            metrics_after: None,
            executed_at: Utc::now(),
            verified_at: None,
            completed_at: None,
            outcome: ActionOutcome::Pending,
            rollback_reason: None,
            normalized_reward: None,
            reward_breakdown: None,
            lessons_learned: None,
        };

        assert_eq!(record.action_type, "adjust_param");
        assert_eq!(record.outcome, ActionOutcome::Pending);
    }

    #[test]
    fn test_effectiveness_record() {
        let record = ActionEffectivenessRecord {
            action_type: "adjust_param".to_string(),
            action_signature: "abc123".to_string(),
            total_attempts: 10,
            successful_attempts: 8,
            failed_attempts: 1,
            rolled_back_attempts: 1,
            avg_reward: 0.5,
            max_reward: Some(0.8),
            min_reward: Some(-0.2),
            effectiveness_score: 0.75,
            first_attempt: Utc::now(),
            last_attempt: Utc::now(),
        };

        assert_eq!(record.successful_attempts, 8);
        assert_eq!(record.effectiveness_score, 0.75);
    }
}
