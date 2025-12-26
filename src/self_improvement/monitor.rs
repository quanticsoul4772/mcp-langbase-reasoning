//! Monitor phase for the self-improvement system.
//!
//! This module implements the first phase of the self-improvement loop:
//! - Collects system metrics (error rate, latency, quality scores)
//! - Maintains baselines using hybrid EMA + rolling average
//! - Detects anomalies that exceed thresholds
//! - Generates health reports with trigger information
//!
//! # Architecture
//!
//! ```text
//! Metrics Sources → Collector → Baseline Update → Trigger Detection → Health Report
//! ```
//!
//! The monitor runs periodically (configurable interval) and aggregates metrics
//! from the pipe invocation history to detect issues that need attention.

use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio::sync::RwLock;
use tracing::{debug, info};

use super::baseline::{BaselineCalculator, BaselineCollection, MetricBaseline, TriggerLevel};
use super::config::SelfImprovementConfig;
use super::types::{Baselines, HealthReport, MetricsSnapshot, TriggerMetric};

// ============================================================================
// Raw Metrics
// ============================================================================

/// Raw metrics collected from the system for aggregation.
#[derive(Debug, Clone)]
pub struct RawMetrics {
    /// Error rate (0.0 - 1.0)
    pub error_rate: f64,
    /// Latency in milliseconds
    pub latency_ms: i64,
    /// Quality score (0.0 - 1.0)
    pub quality_score: f64,
    /// Fallback rate (0.0 - 1.0)
    pub fallback_rate: f64,
    /// Timestamp of collection
    pub timestamp: DateTime<Utc>,
}

/// Aggregated metrics over a time window.
#[derive(Debug, Clone, Default)]
pub struct AggregatedMetrics {
    /// Total invocations in the window
    pub total_invocations: u64,
    /// Number of errors
    pub error_count: u64,
    /// Sum of latencies for averaging
    pub latency_sum: i64,
    /// Latencies for percentile calculation
    pub latencies: Vec<i64>,
    /// Sum of quality scores
    pub quality_sum: f64,
    /// Number of quality score samples
    pub quality_count: u64,
    /// Number of fallbacks
    pub fallback_count: u64,
    /// Window start time
    pub window_start: Option<DateTime<Utc>>,
    /// Window end time
    pub window_end: Option<DateTime<Utc>>,
}

impl AggregatedMetrics {
    /// Create a new empty aggregation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a data point to the aggregation.
    pub fn add(&mut self, metrics: &RawMetrics) {
        self.total_invocations += 1;

        if metrics.error_rate > 0.0 {
            self.error_count += 1;
        }

        self.latency_sum += metrics.latency_ms;
        self.latencies.push(metrics.latency_ms);

        if metrics.quality_score > 0.0 {
            self.quality_sum += metrics.quality_score;
            self.quality_count += 1;
        }

        if metrics.fallback_rate > 0.0 {
            self.fallback_count += 1;
        }

        if self.window_start.is_none() {
            self.window_start = Some(metrics.timestamp);
        }
        self.window_end = Some(metrics.timestamp);
    }

    /// Calculate error rate from aggregated data.
    pub fn error_rate(&self) -> f64 {
        if self.total_invocations == 0 {
            0.0
        } else {
            self.error_count as f64 / self.total_invocations as f64
        }
    }

    /// Calculate P95 latency from aggregated data.
    pub fn latency_p95(&self) -> i64 {
        if self.latencies.is_empty() {
            0
        } else {
            let mut sorted = self.latencies.clone();
            sorted.sort_unstable();
            let idx = (sorted.len() as f64 * 0.95).ceil() as usize - 1;
            sorted[idx.min(sorted.len() - 1)]
        }
    }

    /// Calculate average quality score.
    pub fn quality_score(&self) -> f64 {
        if self.quality_count == 0 {
            0.0
        } else {
            self.quality_sum / self.quality_count as f64
        }
    }

    /// Calculate fallback rate.
    pub fn fallback_rate(&self) -> f64 {
        if self.total_invocations == 0 {
            0.0
        } else {
            self.fallback_count as f64 / self.total_invocations as f64
        }
    }

    /// Convert to MetricsSnapshot.
    pub fn to_snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot::new(
            self.error_rate(),
            self.latency_p95(),
            self.quality_score(),
            self.total_invocations,
        )
    }
}

// ============================================================================
// Monitor State
// ============================================================================

/// Internal state for the monitor.
#[derive(Debug)]
struct MonitorState {
    /// Baseline collection for all metrics
    baselines: BaselineCollection,
    /// Last health check time
    last_check: Option<DateTime<Utc>>,
    /// Current aggregated metrics
    current_aggregation: AggregatedMetrics,
    /// Last generated health report
    last_report: Option<HealthReport>,
}

impl Default for MonitorState {
    fn default() -> Self {
        Self {
            baselines: BaselineCollection::new(),
            last_check: None,
            current_aggregation: AggregatedMetrics::new(),
            last_report: None,
        }
    }
}

// ============================================================================
// Monitor
// ============================================================================

/// Monitor phase for detecting system health issues.
///
/// The monitor collects metrics, maintains baselines, and detects triggers
/// that indicate the system may need adjustment.
///
/// # Example
///
/// ```rust,ignore
/// use mcp_langbase_reasoning::self_improvement::{Monitor, SelfImprovementConfig};
///
/// let config = SelfImprovementConfig::from_env();
/// let monitor = Monitor::new(config);
///
/// // Record metrics as they come in
/// monitor.record_invocation(error, latency_ms, quality, fallback).await;
///
/// // Periodically check health
/// if let Some(report) = monitor.check_health().await {
///     if report.has_triggers() {
///         // Handle triggers...
///     }
/// }
/// ```
pub struct Monitor {
    config: SelfImprovementConfig,
    calculator: BaselineCalculator,
    state: Arc<RwLock<MonitorState>>,
}

impl Monitor {
    /// Create a new monitor.
    pub fn new(config: SelfImprovementConfig) -> Self {
        let calculator = BaselineCalculator::new(config.baseline.clone());
        let state = MonitorState {
            baselines: BaselineCollection::initialize(&config.baseline),
            ..Default::default()
        };

        Self {
            config,
            calculator,
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Record a single invocation's metrics.
    ///
    /// This updates both the current aggregation window and the baselines.
    pub async fn record_invocation(
        &self,
        error: bool,
        latency_ms: i64,
        quality_score: f64,
        fallback: bool,
    ) {
        let raw = RawMetrics {
            error_rate: if error { 1.0 } else { 0.0 },
            latency_ms,
            quality_score,
            fallback_rate: if fallback { 1.0 } else { 0.0 },
            timestamp: Utc::now(),
        };

        let mut state = self.state.write().await;

        // Update aggregation
        state.current_aggregation.add(&raw);

        // Update baselines
        let now = Utc::now();

        if let Some(ref mut baseline) = state.baselines.error_rate {
            self.calculator.update(baseline, raw.error_rate, now);
        }
        if let Some(ref mut baseline) = state.baselines.latency {
            self.calculator.update(baseline, raw.latency_ms as f64, now);
        }
        if let Some(ref mut baseline) = state.baselines.quality_score {
            self.calculator.update_inverted(baseline, raw.quality_score, now);
        }
        if let Some(ref mut baseline) = state.baselines.fallback_rate {
            self.calculator.update(baseline, raw.fallback_rate, now);
        }

        debug!(
            error = error,
            latency_ms = latency_ms,
            quality = quality_score,
            fallback = fallback,
            total = state.current_aggregation.total_invocations,
            "Recorded invocation metrics"
        );
    }

    /// Check system health and generate a report if due.
    ///
    /// Returns `Some(HealthReport)` if it's time for a health check and
    /// there are enough samples, `None` otherwise.
    pub async fn check_health(&self) -> Option<HealthReport> {
        let now = Utc::now();
        let check_interval =
            ChronoDuration::seconds(self.config.monitor.check_interval_secs as i64);

        let mut state = self.state.write().await;

        // Check if it's time for a health check
        if let Some(last) = state.last_check {
            if now - last < check_interval {
                return None;
            }
        }

        // Check if we have enough samples
        if state.current_aggregation.total_invocations < self.config.monitor.min_sample_size as u64
        {
            debug!(
                samples = state.current_aggregation.total_invocations,
                min_required = self.config.monitor.min_sample_size,
                "Not enough samples for health check"
            );
            return None;
        }

        // Generate health report
        let report = self.generate_report(&mut state);

        state.last_check = Some(now);
        state.last_report = Some(report.clone());

        // Reset aggregation for next window
        state.current_aggregation = AggregatedMetrics::new();

        Some(report)
    }

    /// Force a health check regardless of timing.
    ///
    /// Returns `None` if there are insufficient samples.
    pub async fn force_check(&self) -> Option<HealthReport> {
        let mut state = self.state.write().await;

        if state.current_aggregation.total_invocations < self.config.monitor.min_sample_size as u64
        {
            return None;
        }

        let report = self.generate_report(&mut state);
        state.last_check = Some(Utc::now());
        state.last_report = Some(report.clone());
        state.current_aggregation = AggregatedMetrics::new();

        Some(report)
    }

    /// Get the last health report without generating a new one.
    pub async fn last_report(&self) -> Option<HealthReport> {
        self.state.read().await.last_report.clone()
    }

    /// Get current baselines.
    pub async fn baselines(&self) -> BaselineCollection {
        self.state.read().await.baselines.clone()
    }

    /// Get current aggregation stats (for diagnostics).
    pub async fn current_stats(&self) -> MonitorStats {
        let state = self.state.read().await;
        MonitorStats {
            total_invocations: state.current_aggregation.total_invocations,
            error_rate: state.current_aggregation.error_rate(),
            latency_p95: state.current_aggregation.latency_p95(),
            quality_score: state.current_aggregation.quality_score(),
            fallback_rate: state.current_aggregation.fallback_rate(),
            baselines_valid: state.baselines.all_valid(),
            last_check: state.last_check,
        }
    }

    /// Reset the monitor state (for testing).
    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        *state = MonitorState::default();
        state.baselines = BaselineCollection::initialize(&self.config.baseline);
    }

    /// Get current metrics as a snapshot.
    ///
    /// Returns the current aggregated metrics without resetting.
    pub async fn get_current_metrics(&self) -> MetricsSnapshot {
        let state = self.state.read().await;
        state.current_aggregation.to_snapshot()
    }

    /// Get current baselines as a Baselines struct.
    ///
    /// Used for reward calculation in the Learner phase.
    pub async fn get_baselines(&self) -> super::types::Baselines {
        let state = self.state.read().await;
        super::types::Baselines {
            error_rate: state
                .baselines
                .error_rate
                .as_ref()
                .map(|b| b.rolling_avg)
                .unwrap_or(0.0),
            latency_ms: state
                .baselines
                .latency
                .as_ref()
                .map(|b| b.rolling_avg as i64)
                .unwrap_or(0),
            quality_score: state
                .baselines
                .quality_score
                .as_ref()
                .map(|b| b.rolling_avg)
                .unwrap_or(0.8),
        }
    }

    // ========================================================================
    // Internal
    // ========================================================================

    fn generate_report(&self, state: &mut MonitorState) -> HealthReport {
        let start = Instant::now();

        let current_metrics = state.current_aggregation.to_snapshot();
        let mut triggers = Vec::new();

        // Check error rate
        if let Some(ref baseline) = state.baselines.error_rate {
            if let Some(trigger) = self.check_error_rate_trigger(&current_metrics, baseline) {
                triggers.push(trigger);
            }
        }

        // Check latency
        if let Some(ref baseline) = state.baselines.latency {
            if let Some(trigger) = self.check_latency_trigger(&current_metrics, baseline) {
                triggers.push(trigger);
            }
        }

        // Check quality score
        if let Some(ref baseline) = state.baselines.quality_score {
            if let Some(trigger) = self.check_quality_trigger(&current_metrics, baseline) {
                triggers.push(trigger);
            }
        }

        // Check fallback rate
        if let Some(ref baseline) = state.baselines.fallback_rate {
            if let Some(trigger) = self.check_fallback_trigger(&current_metrics, baseline) {
                triggers.push(trigger);
            }
        }

        let is_healthy = triggers.is_empty();
        let baselines = state.baselines.to_baselines().unwrap_or(Baselines {
            error_rate: 0.0,
            latency_ms: 0,
            quality_score: 0.0,
        });

        let report = HealthReport {
            current_metrics,
            baselines,
            triggers,
            is_healthy,
            generated_at: Utc::now(),
        };

        let elapsed = start.elapsed();
        info!(
            is_healthy = is_healthy,
            trigger_count = report.triggers.len(),
            elapsed_us = elapsed.as_micros(),
            "Generated health report"
        );

        report
    }

    fn check_error_rate_trigger(
        &self,
        metrics: &MetricsSnapshot,
        baseline: &MetricBaseline,
    ) -> Option<TriggerMetric> {
        let observed = metrics.error_rate;
        let threshold = self.config.monitor.error_rate_threshold;

        // First check against configured threshold
        if observed > threshold {
            return Some(TriggerMetric::ErrorRate {
                observed,
                baseline: baseline.rolling_avg,
                threshold,
            });
        }

        // Skip baseline checks if there's no meaningful baseline
        // (can't detect anomaly when baseline is 0 - that's perfect)
        if baseline.rolling_avg < 0.001 {
            return None;
        }

        // Then check against baseline triggers
        if let Some(level) = self.calculator.check_trigger(baseline, observed) {
            if level >= TriggerLevel::Warning {
                return Some(TriggerMetric::ErrorRate {
                    observed,
                    baseline: baseline.rolling_avg,
                    threshold: baseline.warning_threshold,
                });
            }
        }

        None
    }

    fn check_latency_trigger(
        &self,
        metrics: &MetricsSnapshot,
        baseline: &MetricBaseline,
    ) -> Option<TriggerMetric> {
        let observed = metrics.latency_p95_ms;
        let threshold = self.config.monitor.latency_threshold_ms;

        // First check against configured threshold
        if observed > threshold {
            return Some(TriggerMetric::Latency {
                observed_p95_ms: observed,
                baseline_ms: baseline.rolling_avg as i64,
                threshold_ms: threshold,
            });
        }

        // Then check against baseline triggers
        if let Some(level) = self.calculator.check_trigger(baseline, observed as f64) {
            if level >= TriggerLevel::Warning {
                return Some(TriggerMetric::Latency {
                    observed_p95_ms: observed,
                    baseline_ms: baseline.rolling_avg as i64,
                    threshold_ms: baseline.warning_threshold as i64,
                });
            }
        }

        None
    }

    fn check_quality_trigger(
        &self,
        metrics: &MetricsSnapshot,
        baseline: &MetricBaseline,
    ) -> Option<TriggerMetric> {
        let observed = metrics.quality_score;
        let minimum = self.config.monitor.quality_threshold;

        // Quality is inverted: lower is worse
        if observed < minimum {
            return Some(TriggerMetric::QualityScore {
                observed,
                baseline: baseline.rolling_avg,
                minimum,
            });
        }

        // Check against baseline triggers (inverted)
        if let Some(level) = self.calculator.check_trigger_inverted(baseline, observed) {
            if level >= TriggerLevel::Warning {
                return Some(TriggerMetric::QualityScore {
                    observed,
                    baseline: baseline.rolling_avg,
                    minimum: baseline.warning_threshold,
                });
            }
        }

        None
    }

    fn check_fallback_trigger(
        &self,
        _metrics: &MetricsSnapshot,
        baseline: &MetricBaseline,
    ) -> Option<TriggerMetric> {
        // Use the actual fallback rate from the baseline tracking
        // (we update baseline.rolling_avg with actual 0/1 fallback values)
        //
        // Since we're using the same baseline for both tracking and detection,
        // we only trigger if the current baseline shows high fallback rate.
        // This is a simplified approach - in production, we'd track fallback
        // counts separately in the aggregation.
        let observed = baseline.ema_value; // Recent trend
        let threshold = self.config.monitor.fallback_rate_threshold;

        // Only check if we have enough data
        if !baseline.is_valid {
            return None;
        }

        // Check if fallback rate exceeds threshold
        if observed > threshold {
            return Some(TriggerMetric::FallbackRate {
                observed,
                baseline: baseline.rolling_avg,
                threshold,
            });
        }

        // Also check against derived thresholds if significantly elevated
        if let Some(level) = self.calculator.check_trigger(baseline, observed) {
            if level >= TriggerLevel::Warning && observed > 0.01 {
                // Only trigger if fallback rate is non-trivial (> 1%)
                return Some(TriggerMetric::FallbackRate {
                    observed,
                    baseline: baseline.rolling_avg,
                    threshold: baseline.warning_threshold,
                });
            }
        }

        None
    }
}

// ============================================================================
// Monitor Stats
// ============================================================================

/// Current monitor statistics for diagnostics.
#[derive(Debug, Clone)]
pub struct MonitorStats {
    /// Total invocations in current window
    pub total_invocations: u64,
    /// Current error rate
    pub error_rate: f64,
    /// Current P95 latency
    pub latency_p95: i64,
    /// Current quality score
    pub quality_score: f64,
    /// Current fallback rate
    pub fallback_rate: f64,
    /// Whether all baselines are valid
    pub baselines_valid: bool,
    /// Last health check time
    pub last_check: Option<DateTime<Utc>>,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SelfImprovementConfig {
        let mut config = SelfImprovementConfig::default();
        config.monitor.check_interval_secs = 1; // Fast for testing
        config.monitor.min_sample_size = 5; // Low for testing
        config.baseline.min_samples = 5; // Low for testing
        config
    }

    #[test]
    fn test_aggregated_metrics() {
        let mut agg = AggregatedMetrics::new();

        agg.add(&RawMetrics {
            error_rate: 0.0,
            latency_ms: 100,
            quality_score: 0.9,
            fallback_rate: 0.0,
            timestamp: Utc::now(),
        });

        agg.add(&RawMetrics {
            error_rate: 1.0,
            latency_ms: 200,
            quality_score: 0.8,
            fallback_rate: 1.0,
            timestamp: Utc::now(),
        });

        assert_eq!(agg.total_invocations, 2);
        assert_eq!(agg.error_count, 1);
        assert_eq!(agg.error_rate(), 0.5);
        assert!((agg.quality_score() - 0.85).abs() < 0.001);
    }

    #[test]
    fn test_latency_p95() {
        let mut agg = AggregatedMetrics::new();

        // Add 100 samples
        for i in 1..=100 {
            agg.add(&RawMetrics {
                error_rate: 0.0,
                latency_ms: i,
                quality_score: 0.9,
                fallback_rate: 0.0,
                timestamp: Utc::now(),
            });
        }

        // P95 should be around 95
        assert!(agg.latency_p95() >= 94 && agg.latency_p95() <= 96);
    }

    #[tokio::test]
    async fn test_monitor_record_invocation() {
        let config = test_config();
        let monitor = Monitor::new(config);

        monitor.record_invocation(false, 100, 0.9, false).await;
        monitor.record_invocation(true, 200, 0.8, true).await;

        let stats = monitor.current_stats().await;
        assert_eq!(stats.total_invocations, 2);
    }

    #[tokio::test]
    async fn test_monitor_insufficient_samples() {
        let config = test_config();
        let monitor = Monitor::new(config);

        // Only 2 samples, need 5
        monitor.record_invocation(false, 100, 0.9, false).await;
        monitor.record_invocation(false, 100, 0.9, false).await;

        let report = monitor.force_check().await;
        assert!(report.is_none());
    }

    #[tokio::test]
    async fn test_monitor_healthy_report() {
        let config = test_config();
        let monitor = Monitor::new(config);

        // Add enough healthy samples
        for _ in 0..10 {
            monitor.record_invocation(false, 100, 0.9, false).await;
        }

        let report = monitor.force_check().await;
        assert!(report.is_some());

        let report = report.unwrap();
        assert!(report.is_healthy);
        assert!(report.triggers.is_empty());
    }

    #[tokio::test]
    async fn test_monitor_error_rate_trigger() {
        let mut config = test_config();
        config.monitor.error_rate_threshold = 0.3; // 30%
        let monitor = Monitor::new(config);

        // Add samples with high error rate (50%)
        for i in 0..10 {
            let error = i % 2 == 0; // 50% error rate
            monitor.record_invocation(error, 100, 0.9, false).await;
        }

        let report = monitor.force_check().await;
        assert!(report.is_some());

        let report = report.unwrap();
        assert!(!report.is_healthy);
        assert!(report.has_triggers());

        // Should have error rate trigger
        let has_error_trigger = report
            .triggers
            .iter()
            .any(|t| matches!(t, TriggerMetric::ErrorRate { .. }));
        assert!(has_error_trigger);
    }

    #[tokio::test]
    async fn test_monitor_latency_trigger() {
        let mut config = test_config();
        config.monitor.latency_threshold_ms = 500;
        let monitor = Monitor::new(config);

        // Add samples with high latency
        for _ in 0..10 {
            monitor.record_invocation(false, 1000, 0.9, false).await;
        }

        let report = monitor.force_check().await;
        assert!(report.is_some());

        let report = report.unwrap();
        assert!(!report.is_healthy);

        let has_latency_trigger = report
            .triggers
            .iter()
            .any(|t| matches!(t, TriggerMetric::Latency { .. }));
        assert!(has_latency_trigger);
    }

    #[tokio::test]
    async fn test_monitor_quality_trigger() {
        let mut config = test_config();
        config.monitor.quality_threshold = 0.8;
        let monitor = Monitor::new(config);

        // Add samples with low quality
        for _ in 0..10 {
            monitor.record_invocation(false, 100, 0.5, false).await;
        }

        let report = monitor.force_check().await;
        assert!(report.is_some());

        let report = report.unwrap();
        assert!(!report.is_healthy);

        let has_quality_trigger = report
            .triggers
            .iter()
            .any(|t| matches!(t, TriggerMetric::QualityScore { .. }));
        assert!(has_quality_trigger);
    }

    #[tokio::test]
    async fn test_monitor_reset() {
        let config = test_config();
        let monitor = Monitor::new(config);

        // Add samples
        for _ in 0..10 {
            monitor.record_invocation(false, 100, 0.9, false).await;
        }

        let stats = monitor.current_stats().await;
        assert_eq!(stats.total_invocations, 10);

        // Reset
        monitor.reset().await;

        let stats = monitor.current_stats().await;
        assert_eq!(stats.total_invocations, 0);
    }

    #[tokio::test]
    async fn test_monitor_baseline_updates() {
        let config = test_config();
        let monitor = Monitor::new(config);

        // Add enough samples to make baselines valid
        for _ in 0..10 {
            monitor.record_invocation(false, 100, 0.9, false).await;
        }

        let baselines = monitor.baselines().await;
        assert!(baselines.error_rate.is_some());
        assert!(baselines.latency.is_some());
        assert!(baselines.quality_score.is_some());

        // Check that baselines are now valid
        let stats = monitor.current_stats().await;
        assert!(stats.baselines_valid);
    }

    #[tokio::test]
    async fn test_monitor_last_report() {
        let config = test_config();
        let monitor = Monitor::new(config);

        // No report yet
        assert!(monitor.last_report().await.is_none());

        // Add samples and generate report
        for _ in 0..10 {
            monitor.record_invocation(false, 100, 0.9, false).await;
        }
        monitor.force_check().await;

        // Now we have a report
        assert!(monitor.last_report().await.is_some());
    }
}
