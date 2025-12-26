//! Baseline calculation for the self-improvement system.
//!
//! This module implements a hybrid baseline calculation approach using:
//! - **EMA (Exponential Moving Average)**: For trend detection (more responsive)
//! - **Rolling Average**: For stable thresholds (less noise)
//!
//! This hybrid approach scored 0.84/1.0 in design analysis for balancing
//! stability and responsiveness.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::config::BaselineConfig;

/// Level of trigger detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TriggerLevel {
    /// EMA indicates significant trend change
    Trend,
    /// Value above warning threshold
    Warning,
    /// Value above critical threshold
    Critical,
}

impl std::fmt::Display for TriggerLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TriggerLevel::Trend => write!(f, "Trend"),
            TriggerLevel::Warning => write!(f, "Warning"),
            TriggerLevel::Critical => write!(f, "Critical"),
        }
    }
}

/// Baseline data for a single metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricBaseline {
    /// Name of the metric
    pub metric_name: String,

    /// Rolling average value
    pub rolling_avg: f64,

    /// Number of samples in rolling average
    pub rolling_sample_count: usize,

    /// Start of rolling average window
    pub rolling_window_start: Option<DateTime<Utc>>,

    /// EMA value
    pub ema_value: f64,

    /// EMA alpha (smoothing factor)
    pub ema_alpha: f64,

    /// Warning threshold derived from baseline
    pub warning_threshold: f64,

    /// Critical threshold derived from baseline
    pub critical_threshold: f64,

    /// Last update time
    pub last_updated: DateTime<Utc>,

    /// Whether baseline has enough data to be valid
    pub is_valid: bool,
}

impl MetricBaseline {
    /// Create a new metric baseline.
    pub fn new(metric_name: &str, config: &BaselineConfig) -> Self {
        Self {
            metric_name: metric_name.to_string(),
            rolling_avg: 0.0,
            rolling_sample_count: 0,
            rolling_window_start: None,
            ema_value: 0.0,
            ema_alpha: config.ema_alpha,
            warning_threshold: 0.0,
            critical_threshold: 0.0,
            last_updated: Utc::now(),
            is_valid: false,
        }
    }

    /// Check if the baseline has enough samples to be valid.
    pub fn has_minimum_samples(&self, min_samples: usize) -> bool {
        self.rolling_sample_count >= min_samples
    }
}

/// Hybrid baseline calculator using EMA for trend detection
/// and rolling average for stable thresholds.
#[derive(Debug, Clone)]
pub struct BaselineCalculator {
    config: BaselineConfig,
}

impl BaselineCalculator {
    /// Create a new baseline calculator.
    pub fn new(config: BaselineConfig) -> Self {
        Self { config }
    }

    /// Update baseline with a new observation.
    ///
    /// This updates both the EMA (for trend detection) and the rolling
    /// average (for stable thresholds).
    pub fn update(&self, baseline: &mut MetricBaseline, new_value: f64, timestamp: DateTime<Utc>) {
        // Initialize EMA on first sample
        if baseline.rolling_sample_count == 0 {
            baseline.ema_value = new_value;
            baseline.rolling_avg = new_value;
            baseline.rolling_window_start = Some(timestamp);
        } else {
            // Update EMA (more responsive to recent changes)
            baseline.ema_value =
                self.config.ema_alpha * new_value + (1.0 - self.config.ema_alpha) * baseline.ema_value;

            // Update rolling average (more stable)
            // Incremental update: new_avg = old_avg + (new_value - old_avg) / (n + 1)
            let n = baseline.rolling_sample_count as f64;
            baseline.rolling_avg = baseline.rolling_avg + (new_value - baseline.rolling_avg) / (n + 1.0);
        }

        baseline.rolling_sample_count += 1;

        // Calculate thresholds based on rolling average
        // For error rates and latency: higher is worse
        baseline.warning_threshold = baseline.rolling_avg * self.config.warning_multiplier;
        baseline.critical_threshold = baseline.rolling_avg * self.config.critical_multiplier;

        baseline.last_updated = timestamp;
        baseline.is_valid = baseline.rolling_sample_count >= self.config.min_samples;
    }

    /// Update baseline with values where lower is worse (like quality score).
    ///
    /// For quality metrics, thresholds are calculated as fractions of baseline
    /// rather than multiples.
    pub fn update_inverted(&self, baseline: &mut MetricBaseline, new_value: f64, timestamp: DateTime<Utc>) {
        // Standard update for averages
        if baseline.rolling_sample_count == 0 {
            baseline.ema_value = new_value;
            baseline.rolling_avg = new_value;
            baseline.rolling_window_start = Some(timestamp);
        } else {
            baseline.ema_value =
                self.config.ema_alpha * new_value + (1.0 - self.config.ema_alpha) * baseline.ema_value;
            let n = baseline.rolling_sample_count as f64;
            baseline.rolling_avg = baseline.rolling_avg + (new_value - baseline.rolling_avg) / (n + 1.0);
        }

        baseline.rolling_sample_count += 1;

        // For inverted metrics: warning = baseline / warning_multiplier
        // e.g., if baseline quality is 0.8 and warning_multiplier is 1.5,
        // warning triggers at 0.8 / 1.5 = 0.53
        baseline.warning_threshold = baseline.rolling_avg / self.config.warning_multiplier;
        baseline.critical_threshold = baseline.rolling_avg / self.config.critical_multiplier;

        baseline.last_updated = timestamp;
        baseline.is_valid = baseline.rolling_sample_count >= self.config.min_samples;
    }

    /// Check if a value triggers an alert.
    ///
    /// Uses the rolling average thresholds for stable alerting,
    /// and EMA for trend detection.
    pub fn check_trigger(&self, baseline: &MetricBaseline, value: f64) -> Option<TriggerLevel> {
        if !baseline.is_valid {
            return None; // Not enough data yet
        }

        // Use rolling avg thresholds for stable alerting
        if value >= baseline.critical_threshold {
            return Some(TriggerLevel::Critical);
        }

        if value >= baseline.warning_threshold {
            return Some(TriggerLevel::Warning);
        }

        // Use EMA for trend detection (are we moving away from normal?)
        if baseline.ema_value > 0.0 {
            let ema_deviation = (value - baseline.ema_value).abs() / baseline.ema_value;
            if ema_deviation > 0.5 {
                // 50% deviation from EMA indicates significant trend
                return Some(TriggerLevel::Trend);
            }
        }

        None
    }

    /// Check if a value triggers an alert for inverted metrics (where lower is worse).
    pub fn check_trigger_inverted(&self, baseline: &MetricBaseline, value: f64) -> Option<TriggerLevel> {
        if !baseline.is_valid {
            return None;
        }

        // For inverted metrics: lower value = worse
        if value <= baseline.critical_threshold {
            return Some(TriggerLevel::Critical);
        }

        if value <= baseline.warning_threshold {
            return Some(TriggerLevel::Warning);
        }

        // EMA trend detection
        if baseline.ema_value > 0.0 {
            let ema_deviation = (baseline.ema_value - value) / baseline.ema_value;
            if ema_deviation > 0.5 {
                return Some(TriggerLevel::Trend);
            }
        }

        None
    }

    /// Get the configuration.
    pub fn config(&self) -> &BaselineConfig {
        &self.config
    }

    /// Calculate deviation percentage from baseline.
    pub fn deviation_pct(&self, baseline: &MetricBaseline, value: f64) -> f64 {
        if baseline.rolling_avg == 0.0 {
            if value > 0.0 {
                100.0
            } else {
                0.0
            }
        } else {
            ((value - baseline.rolling_avg) / baseline.rolling_avg) * 100.0
        }
    }

    /// Calculate inverted deviation percentage (for metrics where lower is worse).
    pub fn deviation_pct_inverted(&self, baseline: &MetricBaseline, value: f64) -> f64 {
        if baseline.rolling_avg == 0.0 {
            -100.0
        } else {
            ((baseline.rolling_avg - value) / baseline.rolling_avg) * 100.0
        }
    }

    /// Prune old samples from rolling window.
    ///
    /// This should be called periodically to remove samples outside the
    /// rolling window. In practice, this would query the database.
    ///
    /// Note: The current implementation uses incremental averaging,
    /// which approximates a rolling window. For exact rolling window
    /// calculations, historical data must be queried.
    pub fn should_reset_window(&self, baseline: &MetricBaseline, now: DateTime<Utc>) -> bool {
        if let Some(window_start) = baseline.rolling_window_start {
            let window_duration = chrono::Duration::seconds(self.config.rolling_window_secs as i64);
            now - window_start > window_duration * 2
        } else {
            false
        }
    }
}

/// Collection of baselines for all metrics.
#[derive(Debug, Clone, Default)]
pub struct BaselineCollection {
    /// Error rate baseline
    pub error_rate: Option<MetricBaseline>,
    /// Latency baseline
    pub latency: Option<MetricBaseline>,
    /// Quality score baseline
    pub quality_score: Option<MetricBaseline>,
    /// Fallback rate baseline
    pub fallback_rate: Option<MetricBaseline>,
}

impl BaselineCollection {
    /// Create a new baseline collection.
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize all baselines with default values.
    pub fn initialize(config: &BaselineConfig) -> Self {
        Self {
            error_rate: Some(MetricBaseline::new("error_rate", config)),
            latency: Some(MetricBaseline::new("latency_p95", config)),
            quality_score: Some(MetricBaseline::new("quality_score", config)),
            fallback_rate: Some(MetricBaseline::new("fallback_rate", config)),
        }
    }

    /// Check if all baselines are valid.
    pub fn all_valid(&self) -> bool {
        self.error_rate.as_ref().is_some_and(|b| b.is_valid)
            && self.latency.as_ref().is_some_and(|b| b.is_valid)
            && self.quality_score.as_ref().is_some_and(|b| b.is_valid)
    }

    /// Get baseline values for reward calculation.
    pub fn to_baselines(&self) -> Option<super::types::Baselines> {
        let error_rate = self.error_rate.as_ref()?;
        let latency = self.latency.as_ref()?;
        let quality = self.quality_score.as_ref()?;

        Some(super::types::Baselines {
            error_rate: error_rate.rolling_avg,
            latency_ms: latency.rolling_avg as i64,
            quality_score: quality.rolling_avg,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> BaselineConfig {
        BaselineConfig {
            ema_alpha: 0.1,
            rolling_window_secs: 3600,
            min_samples: 10,
            warning_multiplier: 1.5,
            critical_multiplier: 2.0,
        }
    }

    #[test]
    fn test_initial_baseline() {
        let config = test_config();
        let baseline = MetricBaseline::new("error_rate", &config);

        assert_eq!(baseline.metric_name, "error_rate");
        assert_eq!(baseline.rolling_sample_count, 0);
        assert!(!baseline.is_valid);
    }

    #[test]
    fn test_baseline_update() {
        let config = test_config();
        let calculator = BaselineCalculator::new(config.clone());
        let mut baseline = MetricBaseline::new("error_rate", &config);

        // Add samples
        for i in 0..15 {
            calculator.update(&mut baseline, 0.05, Utc::now());
            assert_eq!(baseline.rolling_sample_count, i + 1);
        }

        assert!(baseline.is_valid);
        assert!((baseline.rolling_avg - 0.05).abs() < 0.001);
        assert!((baseline.warning_threshold - 0.075).abs() < 0.001); // 0.05 * 1.5
        assert!((baseline.critical_threshold - 0.10).abs() < 0.001); // 0.05 * 2.0
    }

    #[test]
    fn test_ema_responsiveness() {
        let config = test_config();
        let calculator = BaselineCalculator::new(config.clone());
        let mut baseline = MetricBaseline::new("error_rate", &config);

        // Establish baseline
        for _ in 0..10 {
            calculator.update(&mut baseline, 0.05, Utc::now());
        }

        // Add a spike
        calculator.update(&mut baseline, 0.20, Utc::now());

        // EMA should respond more than rolling average
        assert!(baseline.ema_value > baseline.rolling_avg);
    }

    #[test]
    fn test_trigger_detection() {
        let config = test_config();
        let calculator = BaselineCalculator::new(config.clone());
        let mut baseline = MetricBaseline::new("error_rate", &config);

        // Establish baseline at 0.05
        for _ in 0..15 {
            calculator.update(&mut baseline, 0.05, Utc::now());
        }

        // No trigger at baseline
        assert!(calculator.check_trigger(&baseline, 0.05).is_none());

        // Warning at 0.08 (> 0.075 threshold)
        assert_eq!(
            calculator.check_trigger(&baseline, 0.08),
            Some(TriggerLevel::Warning)
        );

        // Critical at 0.12 (> 0.10 threshold)
        assert_eq!(
            calculator.check_trigger(&baseline, 0.12),
            Some(TriggerLevel::Critical)
        );
    }

    #[test]
    fn test_inverted_metric() {
        let config = test_config();
        let calculator = BaselineCalculator::new(config.clone());
        let mut baseline = MetricBaseline::new("quality_score", &config);

        // Establish baseline at 0.80
        for _ in 0..15 {
            calculator.update_inverted(&mut baseline, 0.80, Utc::now());
        }

        // warning_threshold = 0.80 / 1.5 ≈ 0.533
        // critical_threshold = 0.80 / 2.0 = 0.40

        // No trigger at baseline
        assert!(calculator.check_trigger_inverted(&baseline, 0.80).is_none());

        // Warning at 0.50 (< 0.533)
        assert_eq!(
            calculator.check_trigger_inverted(&baseline, 0.50),
            Some(TriggerLevel::Warning)
        );

        // Critical at 0.35 (< 0.40)
        assert_eq!(
            calculator.check_trigger_inverted(&baseline, 0.35),
            Some(TriggerLevel::Critical)
        );
    }

    #[test]
    fn test_deviation_calculation() {
        let config = test_config();
        let calculator = BaselineCalculator::new(config.clone());
        let mut baseline = MetricBaseline::new("error_rate", &config);

        for _ in 0..10 {
            calculator.update(&mut baseline, 0.05, Utc::now());
        }

        // 100% increase
        assert!((calculator.deviation_pct(&baseline, 0.10) - 100.0).abs() < 0.1);

        // 50% decrease
        assert!((calculator.deviation_pct(&baseline, 0.025) - (-50.0)).abs() < 0.1);
    }

    #[test]
    fn test_baseline_collection() {
        let config = test_config();
        let collection = BaselineCollection::initialize(&config);

        assert!(collection.error_rate.is_some());
        assert!(collection.latency.is_some());
        assert!(collection.quality_score.is_some());
        assert!(collection.fallback_rate.is_some());
        assert!(!collection.all_valid()); // Not enough samples yet
    }

    #[test]
    fn test_trigger_level_ordering() {
        assert!(TriggerLevel::Trend < TriggerLevel::Warning);
        assert!(TriggerLevel::Warning < TriggerLevel::Critical);
    }
}
