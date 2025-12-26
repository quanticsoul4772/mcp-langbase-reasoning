//! CLI commands for the self-improvement system.
//!
//! Provides operational control and visibility into the autonomous
//! self-improvement system through command-line interface commands.

use chrono::{Duration, Utc};
use clap::Subcommand;

use super::{
    ActionId, ActionOutcome, CircuitState, DiagnosisId, DiagnosisStatus, SelfImprovementStorage,
    Severity, TriggerMetric,
};
use crate::storage::SqliteStorage;

/// Self-improvement CLI subcommands.
#[derive(Subcommand, Debug, Clone)]
pub enum SelfImproveCommands {
    /// Show current self-improvement system status
    Status,

    /// Show history of self-improvement actions
    History {
        /// Maximum number of actions to show
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Filter by outcome: success, failed, rolled_back, pending
        #[arg(long)]
        outcome: Option<String>,
    },

    /// Run system diagnostics
    Diagnostics {
        /// Show verbose diagnostic output
        #[arg(long)]
        verbose: bool,
    },

    /// Show current configuration
    Config,

    /// Show circuit breaker state
    CircuitBreaker,

    /// Show metric baselines
    Baselines,

    /// Enable the self-improvement system
    Enable,

    /// Disable the self-improvement system
    Disable,

    /// Pause self-improvement for a duration
    Pause {
        /// Duration to pause (e.g., "30m", "2h", "1d")
        #[arg(long)]
        duration: String,
    },

    /// Rollback a specific action
    Rollback {
        /// Action ID to rollback
        action_id: String,
    },

    /// Approve a pending diagnosis for execution
    Approve {
        /// Diagnosis ID to approve
        diagnosis_id: String,
    },

    /// Reject a pending diagnosis
    Reject {
        /// Diagnosis ID to reject
        diagnosis_id: String,

        /// Reason for rejection
        #[arg(long)]
        reason: Option<String>,
    },
}

/// Result of CLI command execution.
pub struct CliResult {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Output message
    pub message: String,
}

impl CliResult {
    /// Create a success result with the given message.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            exit_code: 0,
            message: message.into(),
        }
    }

    /// Create an error result with the given message.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            message: message.into(),
        }
    }
}

/// Execute a self-improvement CLI command.
pub async fn execute_command(
    command: SelfImproveCommands,
    storage: &SqliteStorage,
) -> CliResult {
    match command {
        SelfImproveCommands::Status => execute_status(storage).await,
        SelfImproveCommands::History { limit, outcome } => {
            execute_history(storage, limit, outcome).await
        }
        SelfImproveCommands::Diagnostics { verbose } => execute_diagnostics(storage, verbose).await,
        SelfImproveCommands::Config => execute_config().await,
        SelfImproveCommands::CircuitBreaker => execute_circuit_breaker(storage).await,
        SelfImproveCommands::Baselines => execute_baselines(storage).await,
        SelfImproveCommands::Enable => execute_enable(storage).await,
        SelfImproveCommands::Disable => execute_disable(storage).await,
        SelfImproveCommands::Pause { duration } => execute_pause(storage, &duration).await,
        SelfImproveCommands::Rollback { action_id } => execute_rollback(storage, &action_id).await,
        SelfImproveCommands::Approve { diagnosis_id } => {
            execute_approve(storage, &diagnosis_id).await
        }
        SelfImproveCommands::Reject {
            diagnosis_id,
            reason,
        } => execute_reject(storage, &diagnosis_id, reason).await,
    }
}

/// Execute status command.
async fn execute_status(storage: &SqliteStorage) -> CliResult {
    let mut output = String::new();

    output.push_str("\nSelf-Improvement Status\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════════════\n\n");

    // Get self-improvement storage
    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    // Circuit breaker status
    match si_storage.load_circuit_breaker_state().await {
        Ok(Some(summary)) => {
            let state_str = match summary.state {
                CircuitState::Closed => "CLOSED ✓",
                CircuitState::Open => "OPEN ⚠",
                CircuitState::HalfOpen => "HALF-OPEN ⟳",
            };
            output.push_str(&format!("Circuit Breaker: {} ({} consecutive failures)\n",
                state_str, summary.consecutive_failures));
        }
        Ok(None) => {
            output.push_str("Circuit Breaker: CLOSED ✓ (no state recorded)\n");
        }
        Err(e) => {
            output.push_str(&format!("Circuit Breaker: Unknown (error: {})\n", e));
        }
    }

    output.push('\n');

    // Recent actions (last 24h)
    let since = Utc::now() - Duration::hours(24);
    match si_storage.get_actions_since(since).await {
        Ok(actions) => {
            output.push_str(&format!("Recent Actions (last 24h): {}\n", actions.len()));
            for action in actions.iter().take(5) {
                let outcome_str = match &action.outcome {
                    Some(ActionOutcome::Success) => "[SUCCESS]",
                    Some(ActionOutcome::Failed) => "[FAILED]",
                    Some(ActionOutcome::RolledBack) => "[ROLLED_BACK]",
                    Some(ActionOutcome::Pending) | None => "[PENDING]",
                };
                let age = format_duration(Utc::now() - action.executed_at);
                output.push_str(&format!(
                    "  {} {} ago: {} {} → {}\n",
                    outcome_str,
                    age,
                    action.action_type,
                    action.old_value.as_deref().unwrap_or("-"),
                    action.new_value.as_deref().unwrap_or("-")
                ));
            }
        }
        Err(e) => {
            output.push_str(&format!("Recent Actions: Error loading ({})\n", e));
        }
    }

    output.push('\n');

    // Pending diagnoses
    match si_storage.get_pending_diagnoses().await {
        Ok(diagnoses) => {
            output.push_str(&format!("Pending Diagnoses: {}\n", diagnoses.len()));
            for diagnosis in diagnoses.iter().take(3) {
                output.push_str(&format!(
                    "  - [{}] {}: {}\n",
                    severity_symbol(&diagnosis.severity),
                    format_trigger(&diagnosis.trigger),
                    diagnosis.description.chars().take(50).collect::<String>()
                ));
            }
        }
        Err(e) => {
            output.push_str(&format!("Pending Diagnoses: Error loading ({})\n", e));
        }
    }

    CliResult::success(output)
}

/// Execute history command.
async fn execute_history(
    storage: &SqliteStorage,
    limit: usize,
    outcome_filter: Option<String>,
) -> CliResult {
    let mut output = String::new();

    output.push_str("\nSelf-Improvement Action History\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════════════\n\n");

    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    match si_storage.get_action_history(limit).await {
        Ok(actions) => {
            // Filter by outcome if specified
            let filtered: Vec<_> = if let Some(ref filter) = outcome_filter {
                actions
                    .into_iter()
                    .filter(|a| {
                        matches!(
                            (&a.outcome, filter.to_lowercase().as_str()),
                            (ActionOutcome::Success, "success")
                                | (ActionOutcome::Failed, "failed")
                                | (ActionOutcome::RolledBack, "rolled_back")
                                | (ActionOutcome::Pending, "pending")
                        )
                    })
                    .collect()
            } else {
                actions
            };

            if filtered.is_empty() {
                output.push_str("No actions found matching criteria.\n");
            } else {
                output.push_str(&format!("Showing {} action(s):\n\n", filtered.len()));

                for action in filtered {
                    let outcome_str = match &action.outcome {
                        ActionOutcome::Success => "✓ SUCCESS",
                        ActionOutcome::Failed => "✗ FAILED",
                        ActionOutcome::RolledBack => "↩ ROLLED_BACK",
                        ActionOutcome::Pending => "⋯ PENDING",
                    };

                    // Extract resource and values from action_params JSON
                    let params: serde_json::Value =
                        serde_json::from_str(&action.action_params).unwrap_or(serde_json::Value::Null);
                    let resource = params
                        .get("resource")
                        .and_then(|v| v.as_str())
                        .unwrap_or("-");
                    let scope = params
                        .get("scope")
                        .and_then(|v| v.as_str())
                        .unwrap_or("global");
                    let old_value = params
                        .get("old_value")
                        .or_else(|| params.get("current_value"))
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let new_value = params
                        .get("new_value")
                        .or_else(|| params.get("proposed_value"))
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "-".to_string());

                    output.push_str(&format!(
                        "{} | {} | {}\n",
                        action.executed_at.format("%Y-%m-%d %H:%M:%S"),
                        outcome_str,
                        action.action_type
                    ));
                    output.push_str(&format!("    Resource: {} ({})\n", resource, scope));
                    output.push_str(&format!("    Change: {} → {}\n", old_value, new_value));

                    if let Some(reward) = action.normalized_reward {
                        output.push_str(&format!("    Reward: {:.3}\n", reward));
                    }

                    if let Some(ref reason) = action.rollback_reason {
                        output.push_str(&format!("    Rollback reason: {}\n", reason));
                    }

                    output.push('\n');
                }
            }
        }
        Err(e) => {
            return CliResult::error(format!("Failed to load action history: {}", e));
        }
    }

    CliResult::success(output)
}

/// Execute diagnostics command.
async fn execute_diagnostics(storage: &SqliteStorage, verbose: bool) -> CliResult {
    let mut output = String::new();

    output.push_str("\nSelf-Improvement Diagnostics\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════════════\n\n");

    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    // Check database connectivity
    output.push_str("Database Connectivity:\n");
    match si_storage.health_check().await {
        Ok(()) => output.push_str("  ✓ Database connection OK\n"),
        Err(e) => output.push_str(&format!("  ✗ Database error: {}\n", e)),
    }

    output.push('\n');

    // Check circuit breaker
    output.push_str("Circuit Breaker:\n");
    match si_storage.load_circuit_breaker_state().await {
        Ok(Some(summary)) => {
            output.push_str(&format!("  State: {:?}\n", summary.state));
            output.push_str(&format!("  Consecutive Failures: {}\n", summary.consecutive_failures));
            output.push_str(&format!("  Consecutive Successes: {}\n", summary.consecutive_successes));
        }
        Ok(None) => {
            output.push_str("  No circuit breaker state recorded (using defaults)\n");
        }
        Err(e) => {
            output.push_str(&format!("  ✗ Error loading state: {}\n", e));
        }
    }

    output.push('\n');

    // Check action statistics
    output.push_str("Action Statistics:\n");
    let since_week = Utc::now() - Duration::days(7);
    match si_storage.get_actions_since(since_week).await {
        Ok(actions) => {
            let total = actions.len();
            let successful = actions.iter().filter(|a| matches!(a.outcome, Some(ActionOutcome::Success))).count();
            let failed = actions.iter().filter(|a| matches!(a.outcome, Some(ActionOutcome::Failed))).count();
            let rolled_back = actions.iter().filter(|a| matches!(a.outcome, Some(ActionOutcome::RolledBack))).count();
            let pending = actions.iter().filter(|a| a.outcome.is_none()).count();

            output.push_str(&format!("  Total (7 days): {}\n", total));
            output.push_str(&format!("  Successful: {}\n", successful));
            output.push_str(&format!("  Failed: {}\n", failed));
            output.push_str(&format!("  Rolled Back: {}\n", rolled_back));
            output.push_str(&format!("  Pending: {}\n", pending));

            if total > 0 {
                let success_rate = (successful as f64 / total as f64) * 100.0;
                output.push_str(&format!("  Success Rate: {:.1}%\n", success_rate));
            }
        }
        Err(e) => {
            output.push_str(&format!("  ✗ Error loading actions: {}\n", e));
        }
    }

    output.push('\n');

    // Check baselines
    output.push_str("Metric Baselines:\n");
    match si_storage.get_all_baselines().await {
        Ok(baselines) => {
            if baselines.is_empty() {
                output.push_str("  No baselines recorded yet\n");
            } else {
                output.push_str(&format!("  {} baseline(s) recorded\n", baselines.len()));
                if verbose {
                    for baseline in baselines {
                        output.push_str(&format!(
                            "    - {}: rolling_avg={:.3}, ema={:.3}, samples={}\n",
                            baseline.metric_name,
                            baseline.rolling_avg,
                            baseline.ema_value,
                            baseline.rolling_sample_count
                        ));
                    }
                }
            }
        }
        Err(e) => {
            output.push_str(&format!("  ✗ Error loading baselines: {}\n", e));
        }
    }

    output.push('\n');

    // Check pending diagnoses
    output.push_str("Pending Diagnoses:\n");
    match si_storage.get_pending_diagnoses().await {
        Ok(diagnoses) => {
            if diagnoses.is_empty() {
                output.push_str("  No pending diagnoses\n");
            } else {
                output.push_str(&format!("  {} pending diagnosis(es)\n", diagnoses.len()));
                if verbose {
                    for diagnosis in diagnoses {
                        output.push_str(&format!(
                            "    - [{}] {}: {}\n",
                            severity_symbol(&diagnosis.severity),
                            format_trigger(&diagnosis.trigger),
                            diagnosis.description.chars().take(60).collect::<String>()
                        ));
                    }
                }
            }
        }
        Err(e) => {
            output.push_str(&format!("  ✗ Error loading diagnoses: {}\n", e));
        }
    }

    CliResult::success(output)
}

/// Execute config command.
async fn execute_config() -> CliResult {
    use super::SelfImprovementConfig;

    let config = SelfImprovementConfig::from_env();
    let mut output = String::new();

    output.push_str("\nSelf-Improvement Configuration\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════════════\n\n");

    output.push_str(&format!("System Enabled: {}\n\n", if config.enabled { "YES" } else { "NO" }));

    output.push_str("Monitor Settings:\n");
    output.push_str(&format!("  Check Interval: {}s\n", config.monitor.check_interval_secs));
    output.push_str(&format!("  Error Rate Threshold: {:.1}%\n", config.monitor.error_rate_threshold * 100.0));
    output.push_str(&format!("  Latency Threshold: {}ms\n", config.monitor.latency_threshold_ms));
    output.push_str(&format!("  Quality Threshold: {:.2}\n", config.monitor.quality_threshold));
    output.push_str(&format!("  Fallback Rate Threshold: {:.1}%\n", config.monitor.fallback_rate_threshold * 100.0));
    output.push_str(&format!("  Min Sample Size: {}\n", config.monitor.min_sample_size));
    output.push('\n');

    output.push_str("Executor Settings:\n");
    output.push_str(&format!("  Max Actions/Hour: {}\n", config.executor.max_actions_per_hour));
    output.push_str(&format!("  Cooldown Duration: {}s\n", config.executor.cooldown_duration_secs));
    output.push_str(&format!("  Rollback on Regression: {}\n", config.executor.rollback_on_regression));
    output.push_str(&format!("  Require Approval: {}\n", config.executor.require_approval));
    output.push('\n');

    output.push_str("Circuit Breaker Settings:\n");
    output.push_str(&format!("  Failure Threshold: {}\n", config.circuit_breaker.failure_threshold));
    output.push_str(&format!("  Success Threshold: {}\n", config.circuit_breaker.success_threshold));
    output.push_str(&format!("  Recovery Timeout: {}s\n", config.circuit_breaker.recovery_timeout_secs));
    output.push('\n');

    output.push_str("Baseline Settings:\n");
    output.push_str(&format!("  EMA Alpha: {:.2}\n", config.baseline.ema_alpha));
    output.push_str(&format!("  Rolling Window: {}s\n", config.baseline.rolling_window_secs));
    output.push_str(&format!("  Min Samples: {}\n", config.baseline.min_samples));
    output.push_str(&format!("  Warning Multiplier: {:.1}x\n", config.baseline.warning_multiplier));
    output.push_str(&format!("  Critical Multiplier: {:.1}x\n", config.baseline.critical_multiplier));
    output.push('\n');

    output.push_str("Pipe Settings:\n");
    output.push_str(&format!("  Diagnosis Pipe: {}\n", config.pipes.diagnosis_pipe));
    output.push_str(&format!("  Decision Pipe: {}\n", config.pipes.decision_pipe));
    output.push_str(&format!("  Detection Pipe: {}\n", config.pipes.detection_pipe));
    output.push_str(&format!("  Validation Enabled: {}\n", config.pipes.enable_validation));

    CliResult::success(output)
}

/// Execute circuit-breaker command.
async fn execute_circuit_breaker(storage: &SqliteStorage) -> CliResult {
    let mut output = String::new();

    output.push_str("\nCircuit Breaker Status\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════════════\n\n");

    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    match si_storage.load_circuit_breaker_state().await {
        Ok(Some(summary)) => {
            let state_str = match summary.state {
                CircuitState::Closed => "CLOSED (normal operation) ✓",
                CircuitState::Open => "OPEN (blocking actions) ⚠",
                CircuitState::HalfOpen => "HALF-OPEN (testing recovery) ⟳",
            };

            output.push_str(&format!("State: {}\n\n", state_str));
            output.push_str(&format!("Consecutive Failures: {}\n", summary.consecutive_failures));
            output.push_str(&format!("Consecutive Successes: {}\n", summary.consecutive_successes));
            output.push_str(&format!("Total Failures: {}\n", summary.total_failures));
            output.push_str(&format!("Total Successes: {}\n", summary.total_successes));

            // Explain current state
            output.push('\n');
            match summary.state {
                CircuitState::Closed => {
                    output.push_str("The circuit breaker is closed, meaning the self-improvement\n");
                    output.push_str("system is operating normally and can execute actions.\n");
                }
                CircuitState::Open => {
                    output.push_str("The circuit breaker is OPEN due to consecutive failures.\n");
                    output.push_str("No new actions will be executed until recovery.\n");
                    output.push_str("The system will automatically attempt recovery after the timeout.\n");
                }
                CircuitState::HalfOpen => {
                    output.push_str("The circuit breaker is testing recovery.\n");
                    output.push_str("A single action will be attempted. If successful, the circuit\n");
                    output.push_str("will close. If it fails, the circuit will open again.\n");
                }
            }
        }
        Ok(None) => {
            output.push_str("State: CLOSED (default - no state recorded) ✓\n\n");
            output.push_str("No circuit breaker state has been recorded yet.\n");
            output.push_str("The system is using default settings.\n");
        }
        Err(e) => {
            return CliResult::error(format!("Failed to load circuit breaker state: {}", e));
        }
    }

    CliResult::success(output)
}

/// Execute baselines command.
async fn execute_baselines(storage: &SqliteStorage) -> CliResult {
    let mut output = String::new();

    output.push_str("\nMetric Baselines\n");
    output.push_str("═══════════════════════════════════════════════════════════════════════════════\n\n");

    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    match si_storage.get_all_baselines().await {
        Ok(baselines) => {
            if baselines.is_empty() {
                output.push_str("No baselines have been recorded yet.\n\n");
                output.push_str("Baselines are calculated automatically as the system collects\n");
                output.push_str("metrics from tool invocations. Once enough samples are collected,\n");
                output.push_str("baselines will appear here.\n");
            } else {
                output.push_str(&format!("Found {} baseline(s):\n\n", baselines.len()));

                for baseline in baselines {
                    output.push_str(&format!("📊 {}\n", baseline.metric_name));
                    output.push_str(&format!("   Rolling Average: {:.4}\n", baseline.rolling_avg));
                    output.push_str(&format!("   EMA Value: {:.4}\n", baseline.ema_value));
                    output.push_str(&format!("   Sample Count: {}\n", baseline.rolling_sample_count));
                    output.push_str(&format!("   Last Updated: {}\n", baseline.last_updated.format("%Y-%m-%d %H:%M:%S UTC")));
                    output.push('\n');
                }
            }
        }
        Err(e) => {
            return CliResult::error(format!("Failed to load baselines: {}", e));
        }
    }

    CliResult::success(output)
}

/// Execute enable command.
async fn execute_enable(storage: &SqliteStorage) -> CliResult {
    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    match si_storage.set_system_enabled(true).await {
        Ok(()) => {
            let mut output = String::new();
            output.push_str("\n✓ Self-improvement system ENABLED\n\n");
            output.push_str("The system will now:\n");
            output.push_str("  - Monitor metrics for anomalies\n");
            output.push_str("  - Diagnose issues when thresholds are exceeded\n");
            output.push_str("  - Execute safe, bounded actions to improve performance\n");
            output.push_str("  - Learn from action outcomes\n\n");
            output.push_str("Use 'self-improve status' to check current state.\n");
            CliResult::success(output)
        }
        Err(e) => CliResult::error(format!("Failed to enable system: {}", e)),
    }
}

/// Execute disable command.
async fn execute_disable(storage: &SqliteStorage) -> CliResult {
    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    match si_storage.set_system_enabled(false).await {
        Ok(()) => {
            let mut output = String::new();
            output.push_str("\n⚠ Self-improvement system DISABLED\n\n");
            output.push_str("The system will no longer:\n");
            output.push_str("  - Execute any automatic actions\n");
            output.push_str("  - Respond to metric anomalies\n\n");
            output.push_str("Metrics will continue to be collected for monitoring.\n");
            output.push_str("Use 'self-improve enable' to re-enable the system.\n");
            CliResult::success(output)
        }
        Err(e) => CliResult::error(format!("Failed to disable system: {}", e)),
    }
}

/// Execute pause command.
async fn execute_pause(storage: &SqliteStorage, duration_str: &str) -> CliResult {
    // Parse duration string (e.g., "30m", "2h", "1d")
    let duration = match parse_duration_string(duration_str) {
        Ok(d) => d,
        Err(e) => return CliResult::error(format!("Invalid duration format: {}", e)),
    };

    let si_storage = SelfImprovementStorage::new(storage.pool().clone());
    let ends_at = Utc::now() + duration;

    match si_storage.create_pause(ends_at, "CLI pause command").await {
        Ok(()) => {
            let mut output = String::new();
            output.push_str(&format!(
                "\n⏸ Self-improvement system PAUSED until {}\n\n",
                ends_at.format("%Y-%m-%d %H:%M:%S UTC")
            ));
            output.push_str("During this time:\n");
            output.push_str("  - No automatic actions will be executed\n");
            output.push_str("  - Metrics will continue to be collected\n");
            output.push_str("  - System will automatically resume after the pause expires\n\n");
            output.push_str("To resume early, use 'self-improve enable'.\n");
            CliResult::success(output)
        }
        Err(e) => CliResult::error(format!("Failed to pause system: {}", e)),
    }
}

/// Execute rollback command.
async fn execute_rollback(storage: &SqliteStorage, action_id_str: &str) -> CliResult {
    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    // Parse action ID
    let action_id = ActionId(action_id_str.to_string());

    // Get the action details first
    match si_storage.get_action(&action_id).await {
        Ok(Some(action)) => {
            // Check if action can be rolled back
            if action.outcome == ActionOutcome::RolledBack {
                return CliResult::error("Action has already been rolled back.");
            }
            if action.outcome == ActionOutcome::Pending {
                return CliResult::error(
                    "Action is still pending. Cancel it with 'self-improve reject' instead.",
                );
            }

            // Execute rollback
            match si_storage
                .rollback_action(&action_id, "Manual rollback via CLI")
                .await
            {
                Ok(()) => {
                    let mut output = String::new();
                    output.push_str(&format!("\n↩ Action {} rolled back successfully\n\n", action_id_str));
                    output.push_str(&format!("Action type: {}\n", action.action_type));
                    output.push_str("The configuration change has been reverted.\n\n");
                    output.push_str("Note: A learning record has been created for this rollback.\n");
                    CliResult::success(output)
                }
                Err(e) => CliResult::error(format!("Failed to rollback action: {}", e)),
            }
        }
        Ok(None) => CliResult::error(format!("Action '{}' not found.", action_id_str)),
        Err(e) => CliResult::error(format!("Failed to load action: {}", e)),
    }
}

/// Execute approve command.
async fn execute_approve(storage: &SqliteStorage, diagnosis_id_str: &str) -> CliResult {
    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    let diagnosis_id = DiagnosisId(diagnosis_id_str.to_string());

    // Get the diagnosis details first
    match si_storage.get_diagnosis(&diagnosis_id).await {
        Ok(Some(diagnosis)) => {
            // Check if diagnosis is awaiting approval
            if diagnosis.status != DiagnosisStatus::AwaitingApproval
                && diagnosis.status != DiagnosisStatus::Pending
            {
                return CliResult::error(format!(
                    "Diagnosis is not pending approval. Current status: {:?}",
                    diagnosis.status
                ));
            }

            // Approve the diagnosis
            match si_storage
                .update_diagnosis_status(&diagnosis_id, DiagnosisStatus::Pending)
                .await
            {
                Ok(()) => {
                    let mut output = String::new();
                    output.push_str(&format!(
                        "\n✓ Diagnosis {} approved for execution\n\n",
                        diagnosis_id_str
                    ));
                    output.push_str(&format!("Severity: {:?}\n", diagnosis.severity));
                    output.push_str(&format!("Description: {}\n", diagnosis.description));
                    output.push_str(&format!(
                        "Suggested Action: {}\n\n",
                        diagnosis.suggested_action.action_type()
                    ));
                    output.push_str("The action will be executed in the next improvement cycle.\n");
                    CliResult::success(output)
                }
                Err(e) => CliResult::error(format!("Failed to approve diagnosis: {}", e)),
            }
        }
        Ok(None) => CliResult::error(format!("Diagnosis '{}' not found.", diagnosis_id_str)),
        Err(e) => CliResult::error(format!("Failed to load diagnosis: {}", e)),
    }
}

/// Execute reject command.
async fn execute_reject(
    storage: &SqliteStorage,
    diagnosis_id_str: &str,
    reason: Option<String>,
) -> CliResult {
    let si_storage = SelfImprovementStorage::new(storage.pool().clone());

    let diagnosis_id = DiagnosisId(diagnosis_id_str.to_string());

    // Get the diagnosis details first
    match si_storage.get_diagnosis(&diagnosis_id).await {
        Ok(Some(diagnosis)) => {
            // Check if diagnosis can be rejected
            if diagnosis.status == DiagnosisStatus::Completed {
                return CliResult::error("Diagnosis has already been completed and cannot be rejected.");
            }
            if diagnosis.status == DiagnosisStatus::RolledBack {
                return CliResult::error("Diagnosis was already rolled back.");
            }

            let rejection_reason = reason.unwrap_or_else(|| "Rejected via CLI".to_string());

            // Reject the diagnosis (mark as superseded)
            match si_storage
                .reject_diagnosis(&diagnosis_id, &rejection_reason)
                .await
            {
                Ok(()) => {
                    let mut output = String::new();
                    output.push_str(&format!(
                        "\n✗ Diagnosis {} rejected\n\n",
                        diagnosis_id_str
                    ));
                    output.push_str(&format!("Reason: {}\n", rejection_reason));
                    output.push_str(&format!("Original description: {}\n\n", diagnosis.description));
                    output.push_str("The suggested action will not be executed.\n");
                    CliResult::success(output)
                }
                Err(e) => CliResult::error(format!("Failed to reject diagnosis: {}", e)),
            }
        }
        Ok(None) => CliResult::error(format!("Diagnosis '{}' not found.", diagnosis_id_str)),
        Err(e) => CliResult::error(format!("Failed to load diagnosis: {}", e)),
    }
}

// Helper functions

/// Parse duration string (e.g., "30m", "2h", "1d") into chrono::Duration.
fn parse_duration_string(s: &str) -> Result<Duration, String> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Err("Duration cannot be empty".to_string());
    }

    let (num_str, unit) = if s.ends_with('s') {
        (&s[..s.len() - 1], 's')
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], 'm')
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], 'h')
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], 'd')
    } else {
        return Err("Duration must end with 's' (seconds), 'm' (minutes), 'h' (hours), or 'd' (days)".to_string());
    };

    let num: i64 = num_str
        .parse()
        .map_err(|_| format!("Invalid number: '{}'", num_str))?;

    if num <= 0 {
        return Err("Duration must be positive".to_string());
    }

    match unit {
        's' => Ok(Duration::seconds(num)),
        'm' => Ok(Duration::minutes(num)),
        'h' => Ok(Duration::hours(num)),
        'd' => Ok(Duration::days(num)),
        _ => unreachable!(),
    }
}

fn severity_symbol(severity: &Severity) -> &'static str {
    match severity {
        Severity::Critical => "🔴",
        Severity::High => "🟠",
        Severity::Warning => "🟡",
        Severity::Info => "🟢",
    }
}

fn format_trigger(trigger: &TriggerMetric) -> &'static str {
    match trigger {
        TriggerMetric::ErrorRate { .. } => "ErrorRate",
        TriggerMetric::Latency { .. } => "Latency",
        TriggerMetric::QualityScore { .. } => "QualityScore",
        TriggerMetric::FallbackRate { .. } => "FallbackRate",
    }
}

fn format_duration(duration: chrono::Duration) -> String {
    let total_secs = duration.num_seconds();
    if total_secs < 60 {
        format!("{}s", total_secs)
    } else if total_secs < 3600 {
        format!("{}m", total_secs / 60)
    } else if total_secs < 86400 {
        format!("{}h", total_secs / 3600)
    } else {
        format!("{}d", total_secs / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(chrono::Duration::seconds(30)), "30s");
        assert_eq!(format_duration(chrono::Duration::seconds(90)), "1m");
        assert_eq!(format_duration(chrono::Duration::seconds(3600)), "1h");
        assert_eq!(format_duration(chrono::Duration::seconds(86400)), "1d");
    }

    #[test]
    fn test_severity_symbol() {
        assert_eq!(severity_symbol(&Severity::Critical), "🔴");
        assert_eq!(severity_symbol(&Severity::Warning), "🟡");
        assert_eq!(severity_symbol(&Severity::Info), "🟢");
    }

    #[test]
    fn test_cli_result_success() {
        let result = CliResult::success("test message");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.message, "test message");
    }

    #[test]
    fn test_cli_result_error() {
        let result = CliResult::error("error message");
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.message, "error message");
    }
}
