//! CLI commands for the self-improvement system.
//!
//! Provides operational control and visibility into the autonomous
//! self-improvement system through command-line interface commands.

use chrono::{Duration, Utc};
use clap::Subcommand;

use super::{ActionOutcome, CircuitState, SelfImprovementStorage, Severity, TriggerMetric};
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

// Helper functions

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
