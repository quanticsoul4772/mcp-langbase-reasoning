# Self-Improvement System: Technical Design Plan

## Executive Summary

This document provides a comprehensive technical design for implementing an autonomous self-improvement layer in the mcp-langbase-reasoning MCP server. The design was developed using structured reasoning tools (Graph-of-Thoughts, Tree reasoning, Divergent thinking, and Reflection) to ensure thorough analysis of all design decisions.

**Key Design Decisions Made:**
- **Baseline Method**: Hybrid (EMA + Rolling Average) - Score: 0.84/1.0
- **Execution Model**: Serial execution with async mutex in v1
- **Action Scope**: Config-only changes (no runtime code modification)
- **Safety**: Circuit breaker + rollback + verification + cooldown

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         MCP CLIENT (Claude, etc.)                        │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          MCP SERVER (Rust)                               │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                     EXISTING INFRASTRUCTURE                        │  │
│  │  Config │ Storage │ Langbase │ Modes │ Presets │ Server/Handlers  │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                    │                                     │
│                                    ▼                                     │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                   SELF-IMPROVEMENT LAYER (NEW)                     │  │
│  │                                                                     │  │
│  │   ┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐    │  │
│  │   │ MONITOR  │───▶│ ANALYZER │───▶│ EXECUTOR │───▶│ LEARNER  │    │  │
│  │   │ Phase 1  │    │ Phase 2  │    │ Phase 3  │    │ Phase 4  │    │  │
│  │   └──────────┘    └──────────┘    └──────────┘    └──────────┘    │  │
│  │        │               │               │               │           │  │
│  │        └───────────────┴───────────────┴───────────────┘           │  │
│  │                              │                                      │  │
│  │   ┌──────────────────────────┴───────────────────────────────┐     │  │
│  │   │              SHARED INFRASTRUCTURE                        │     │  │
│  │   │  Circuit Breaker │ Allowlist │ Config Manager │ Storage   │     │  │
│  │   └──────────────────────────────────────────────────────────┘     │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Module Structure

```
src/self_improvement/
├── mod.rs                    # Public API, SelfImprovementSystem
├── types.rs                  # Core types: SelfDiagnosis, SuggestedAction, etc.
├── config.rs                 # SelfImprovementConfig, PipeConfig, thresholds
├── allowlist.rs              # ActionAllowlist, ParamBounds
├── baseline.rs               # BaselineCalculator (EMA + Rolling Avg)
├── pipes.rs                  # SelfImprovementPipes - Langbase pipe integration
├── monitor.rs                # Monitor phase: health checks, trigger detection
├── analyzer.rs               # Analyzer phase: diagnosis generation via pipes
├── executor.rs               # Executor phase: safe execution, rollback
├── learner.rs                # Learner phase: outcome tracking, learning via pipes
├── storage.rs                # SelfImprovementStorage (extends SqliteStorage)
├── circuit_breaker.rs        # CircuitBreaker implementation
└── cli.rs                    # CLI command handlers
```

---

## 3. Database Schema

```sql
-- migrations/20240109000001_self_improvement_tables.sql

-- ============================================================================
-- METRIC BASELINES
-- ============================================================================
CREATE TABLE metric_baselines (
    id TEXT PRIMARY KEY,
    metric_name TEXT NOT NULL UNIQUE,

    -- Rolling average baseline (24-hour window)
    rolling_avg_value REAL NOT NULL DEFAULT 0.0,
    rolling_avg_sample_count INTEGER NOT NULL DEFAULT 0,
    rolling_avg_window_start TIMESTAMP,

    -- EMA baseline (alpha = 0.1 for trend detection)
    ema_value REAL NOT NULL DEFAULT 0.0,
    ema_alpha REAL NOT NULL DEFAULT 0.1,

    -- Thresholds derived from baselines
    warning_threshold REAL,
    critical_threshold REAL,

    -- Metadata
    last_updated TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT  -- JSON for additional context
);

CREATE INDEX idx_baselines_metric ON metric_baselines(metric_name);

-- ============================================================================
-- SELF-IMPROVEMENT DIAGNOSES
-- ============================================================================
CREATE TABLE self_diagnoses (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Trigger information
    trigger_metric TEXT NOT NULL,           -- 'error_rate', 'latency', 'quality'
    trigger_type TEXT NOT NULL,             -- JSON: TriggerMetric enum
    observed_value REAL NOT NULL,
    baseline_value REAL NOT NULL,
    deviation_pct REAL NOT NULL,            -- How far from baseline (%)

    -- Severity and description
    severity TEXT NOT NULL,                 -- 'info', 'warning', 'high', 'critical'
    description TEXT NOT NULL,
    suspected_cause TEXT,

    -- Action
    suggested_action TEXT NOT NULL,         -- JSON: SuggestedAction enum
    action_rationale TEXT,

    -- Lifecycle
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending', 'executing', 'completed', 'rolled_back', 'superseded'
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_diagnoses_status ON self_diagnoses(status);
CREATE INDEX idx_diagnoses_created ON self_diagnoses(created_at DESC);
CREATE INDEX idx_diagnoses_severity ON self_diagnoses(severity);

-- ============================================================================
-- ACTION EXECUTION HISTORY
-- ============================================================================
CREATE TABLE self_improvement_actions (
    id TEXT PRIMARY KEY,
    diagnosis_id TEXT NOT NULL REFERENCES self_diagnoses(id),

    -- Action details
    action_type TEXT NOT NULL,              -- 'adjust_param', 'toggle_feature', etc.
    action_params TEXT NOT NULL,            -- JSON: full action specification

    -- State snapshots
    pre_state TEXT NOT NULL,                -- JSON: config state before change
    post_state TEXT,                        -- JSON: config state after change

    -- Metrics snapshots
    metrics_before TEXT NOT NULL,           -- JSON: {error_rate, latency, quality}
    metrics_after TEXT,                     -- JSON: {error_rate, latency, quality}

    -- Timing
    executed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    verified_at TIMESTAMP,
    completed_at TIMESTAMP,

    -- Outcome
    outcome TEXT NOT NULL DEFAULT 'pending', -- 'pending', 'success', 'failed', 'rolled_back'
    rollback_reason TEXT,

    -- Reward calculation
    normalized_reward REAL,                 -- [-1.0, 1.0]
    reward_breakdown TEXT,                  -- JSON: {error_reward, latency_reward, quality_reward}

    -- Learning
    lessons_learned TEXT                    -- JSON: insights for future decisions
);

CREATE INDEX idx_actions_diagnosis ON self_improvement_actions(diagnosis_id);
CREATE INDEX idx_actions_outcome ON self_improvement_actions(outcome);
CREATE INDEX idx_actions_type ON self_improvement_actions(action_type);
CREATE INDEX idx_actions_reward ON self_improvement_actions(normalized_reward);

-- ============================================================================
-- CIRCUIT BREAKER STATE
-- ============================================================================
CREATE TABLE circuit_breaker_state (
    id TEXT PRIMARY KEY DEFAULT 'main',

    -- State machine
    state TEXT NOT NULL DEFAULT 'closed',   -- 'closed', 'open', 'half_open'

    -- Counters
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    consecutive_successes INTEGER NOT NULL DEFAULT 0,
    total_failures INTEGER NOT NULL DEFAULT 0,
    total_successes INTEGER NOT NULL DEFAULT 0,

    -- Timing
    last_failure TIMESTAMP,
    last_success TIMESTAMP,
    last_state_change TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Configuration (stored for audit)
    failure_threshold INTEGER NOT NULL DEFAULT 3,
    success_threshold INTEGER NOT NULL DEFAULT 2,
    recovery_timeout_secs INTEGER NOT NULL DEFAULT 3600
);

-- ============================================================================
-- COOLDOWN PERIODS
-- ============================================================================
CREATE TABLE cooldown_periods (
    id TEXT PRIMARY KEY,
    started_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ends_at TIMESTAMP NOT NULL,
    reason TEXT,
    triggered_by TEXT REFERENCES self_improvement_actions(id),
    is_active INTEGER NOT NULL DEFAULT 1    -- Boolean: 1 = active, 0 = expired
);

CREATE INDEX idx_cooldown_active ON cooldown_periods(is_active, ends_at);

-- ============================================================================
-- ACTION EFFECTIVENESS TRACKING
-- ============================================================================
CREATE TABLE action_effectiveness (
    id TEXT PRIMARY KEY,
    action_type TEXT NOT NULL,
    action_signature TEXT NOT NULL,         -- Hash of action params for grouping

    -- Statistics
    total_attempts INTEGER NOT NULL DEFAULT 0,
    successful_attempts INTEGER NOT NULL DEFAULT 0,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    rolled_back_attempts INTEGER NOT NULL DEFAULT 0,

    -- Reward statistics
    avg_reward REAL NOT NULL DEFAULT 0.0,
    max_reward REAL,
    min_reward REAL,

    -- Timing
    first_attempt TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_attempt TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

    -- Confidence in this action type
    effectiveness_score REAL NOT NULL DEFAULT 0.5,  -- [0, 1]

    UNIQUE(action_type, action_signature)
);

CREATE INDEX idx_effectiveness_type ON action_effectiveness(action_type);
CREATE INDEX idx_effectiveness_score ON action_effectiveness(effectiveness_score DESC);
```

---

## 4. Core Types

### 4.1 SelfDiagnosis

```rust
// src/self_improvement/types.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Unique identifier for a diagnosis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiagnosisId(pub String);

impl DiagnosisId {
    pub fn new() -> Self {
        Self(format!("diag_{}", uuid::Uuid::new_v4()))
    }
}

/// Severity levels for detected issues
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info = 0,
    Warning = 1,
    High = 2,
    Critical = 3,
}

impl Severity {
    pub fn from_deviation(deviation_pct: f64) -> Self {
        match deviation_pct {
            d if d >= 100.0 => Severity::Critical,
            d if d >= 50.0 => Severity::High,
            d if d >= 25.0 => Severity::Warning,
            _ => Severity::Info,
        }
    }
}

/// What triggered the diagnosis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TriggerMetric {
    ErrorRate {
        observed: f64,
        baseline: f64,
        threshold: f64,
    },
    Latency {
        observed_p95_ms: i64,
        baseline_ms: i64,
        threshold_ms: i64,
    },
    QualityScore {
        observed: f64,
        baseline: f64,
        minimum: f64,
    },
    FallbackRate {
        observed: f64,
        baseline: f64,
        threshold: f64,
    },
}

impl TriggerMetric {
    pub fn metric_name(&self) -> &'static str {
        match self {
            TriggerMetric::ErrorRate { .. } => "error_rate",
            TriggerMetric::Latency { .. } => "latency_p95",
            TriggerMetric::QualityScore { .. } => "quality_score",
            TriggerMetric::FallbackRate { .. } => "fallback_rate",
        }
    }

    pub fn deviation_pct(&self) -> f64 {
        match self {
            TriggerMetric::ErrorRate { observed, baseline, .. } => {
                if *baseline == 0.0 { 100.0 } else { ((observed - baseline) / baseline) * 100.0 }
            }
            TriggerMetric::Latency { observed_p95_ms, baseline_ms, .. } => {
                if *baseline_ms == 0 { 100.0 } else { ((*observed_p95_ms - baseline_ms) as f64 / *baseline_ms as f64) * 100.0 }
            }
            TriggerMetric::QualityScore { observed, baseline, .. } => {
                if *baseline == 0.0 { -100.0 } else { ((baseline - observed) / baseline) * 100.0 }
            }
            TriggerMetric::FallbackRate { observed, baseline, .. } => {
                if *baseline == 0.0 { 100.0 } else { ((observed - baseline) / baseline) * 100.0 }
            }
        }
    }
}

/// Lifecycle status of a diagnosis
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DiagnosisStatus {
    Pending,
    Executing,
    Completed,
    RolledBack,
    Superseded,
    AwaitingApproval,
}

/// Complete diagnosis report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfDiagnosis {
    pub id: DiagnosisId,
    pub created_at: DateTime<Utc>,
    pub trigger: TriggerMetric,
    pub severity: Severity,
    pub description: String,
    pub suspected_cause: Option<String>,
    pub suggested_action: SuggestedAction,
    pub action_rationale: Option<String>,
    pub status: DiagnosisStatus,
}
```

### 4.2 SuggestedAction

```rust
/// Actions the self-improvement system can take
///
/// CONSTRAINTS:
/// - All actions MUST be reversible
/// - Config-only (no runtime code changes)
/// - Bounded by ActionAllowlist
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SuggestedAction {
    /// Adjust a numeric configuration parameter
    AdjustParam {
        key: String,
        old_value: ParamValue,
        new_value: ParamValue,
        scope: ConfigScope,
    },

    /// Toggle a feature flag
    ToggleFeature {
        feature_name: String,
        desired_state: bool,
        reason: String,
    },

    /// Restart a service component (graceful)
    RestartService {
        component: ServiceComponent,
        graceful: bool,
    },

    /// Clear a cache
    ClearCache {
        cache_name: String,
    },

    /// Scale a resource limit
    ScaleResource {
        resource: ResourceType,
        old_value: u32,
        new_value: u32,
    },

    /// Take no action, continue monitoring
    NoOp {
        reason: String,
        revisit_after: Duration,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParamValue {
    Integer(i64),
    Float(f64),
    String(String),
    Duration(Duration),
    Boolean(bool),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigScope {
    Environment,
    ConfigFile { path: String },
    Runtime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceComponent {
    Full,
    LangbaseClient,
    Storage,
    Mode { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResourceType {
    MaxConcurrentRequests,
    ConnectionPoolSize,
    CacheSize,
    TimeoutMs,
    MaxRetries,
    RetryDelayMs,
}

impl SuggestedAction {
    pub fn action_type(&self) -> &'static str {
        match self {
            SuggestedAction::AdjustParam { .. } => "adjust_param",
            SuggestedAction::ToggleFeature { .. } => "toggle_feature",
            SuggestedAction::RestartService { .. } => "restart_service",
            SuggestedAction::ClearCache { .. } => "clear_cache",
            SuggestedAction::ScaleResource { .. } => "scale_resource",
            SuggestedAction::NoOp { .. } => "no_op",
        }
    }

    pub fn is_reversible(&self) -> bool {
        match self {
            SuggestedAction::ClearCache { .. } => false,  // Cache clear is not reversible
            SuggestedAction::RestartService { .. } => false,  // Restart is not reversible
            _ => true,
        }
    }
}
```

### 4.3 NormalizedReward

```rust
/// Normalized reward for comparing improvements across metrics
/// All rewards are in range [-1.0, 1.0]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedReward {
    pub value: f64,
    pub breakdown: RewardBreakdown,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardBreakdown {
    pub error_rate_reward: f64,
    pub latency_reward: f64,
    pub quality_reward: f64,
    pub weights: RewardWeights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardWeights {
    pub error_rate: f64,
    pub latency: f64,
    pub quality: f64,
}

impl Default for RewardWeights {
    fn default() -> Self {
        Self {
            error_rate: 0.5,
            latency: 0.3,
            quality: 0.2,
        }
    }
}

impl RewardWeights {
    /// Adjust weights based on trigger type
    pub fn for_trigger(trigger: &TriggerMetric) -> Self {
        match trigger {
            TriggerMetric::ErrorRate { .. } => Self {
                error_rate: 0.7,
                latency: 0.2,
                quality: 0.1,
            },
            TriggerMetric::Latency { .. } => Self {
                error_rate: 0.3,
                latency: 0.6,
                quality: 0.1,
            },
            TriggerMetric::QualityScore { .. } => Self {
                error_rate: 0.3,
                latency: 0.2,
                quality: 0.5,
            },
            TriggerMetric::FallbackRate { .. } => Self {
                error_rate: 0.5,
                latency: 0.3,
                quality: 0.2,
            },
        }
    }
}

impl NormalizedReward {
    pub fn calculate(
        trigger: &TriggerMetric,
        pre_metrics: &MetricsSnapshot,
        post_metrics: &MetricsSnapshot,
        baselines: &Baselines,
    ) -> Self {
        let weights = RewardWeights::for_trigger(trigger);

        // Error rate reward: improvement = decrease
        let error_reward = if baselines.error_rate > 0.0 {
            ((pre_metrics.error_rate - post_metrics.error_rate) / baselines.error_rate)
                .clamp(-1.0, 1.0)
        } else {
            0.0
        };

        // Latency reward: improvement = decrease
        let latency_reward = if baselines.latency_ms > 0 {
            ((pre_metrics.latency_p95_ms - post_metrics.latency_p95_ms) as f64
                / baselines.latency_ms as f64)
                .clamp(-1.0, 1.0)
        } else {
            0.0
        };

        // Quality reward: improvement = increase
        let quality_reward = if baselines.quality_score < 1.0 {
            ((post_metrics.quality_score - pre_metrics.quality_score)
                / (1.0 - baselines.quality_score))
                .clamp(-1.0, 1.0)
        } else {
            0.0
        };

        let composite = weights.error_rate * error_reward
            + weights.latency * latency_reward
            + weights.quality * quality_reward;

        // Confidence based on sample size
        let confidence = (post_metrics.sample_count as f64 / 100.0).min(1.0);

        Self {
            value: composite,
            breakdown: RewardBreakdown {
                error_rate_reward: error_reward,
                latency_reward,
                quality_reward,
                weights,
            },
            confidence,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub error_rate: f64,
    pub latency_p95_ms: i64,
    pub quality_score: f64,
    pub sample_count: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baselines {
    pub error_rate: f64,
    pub latency_ms: i64,
    pub quality_score: f64,
}
```

---

## 5. Action Allowlist

```rust
// src/self_improvement/allowlist.rs

use std::collections::{HashMap, HashSet};

/// Registry of allowed actions with safe bounds
pub struct ActionAllowlist {
    pub adjustable_params: HashMap<String, ParamBounds>,
    pub toggleable_features: HashSet<String>,
    pub scalable_resources: HashMap<ResourceType, ResourceBounds>,
}

#[derive(Debug, Clone)]
pub struct ParamBounds {
    pub current_value: ParamValue,
    pub min: ParamValue,
    pub max: ParamValue,
    pub step: ParamValue,  // Maximum change per action
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ResourceBounds {
    pub min: u32,
    pub max: u32,
    pub step: u32,
}

impl ActionAllowlist {
    /// Create default allowlist based on existing Config structure
    pub fn default_allowlist() -> Self {
        let mut params = HashMap::new();

        // REQUEST_TIMEOUT_MS: 5000-60000ms, step 5000ms
        params.insert("REQUEST_TIMEOUT_MS".to_string(), ParamBounds {
            current_value: ParamValue::Integer(30000),
            min: ParamValue::Integer(5000),
            max: ParamValue::Integer(60000),
            step: ParamValue::Integer(5000),
            description: "HTTP request timeout for Langbase API calls".to_string(),
        });

        // MAX_RETRIES: 1-10, step 1
        params.insert("MAX_RETRIES".to_string(), ParamBounds {
            current_value: ParamValue::Integer(3),
            min: ParamValue::Integer(1),
            max: ParamValue::Integer(10),
            step: ParamValue::Integer(1),
            description: "Maximum retry attempts for failed API calls".to_string(),
        });

        // RETRY_DELAY_MS: 500-5000ms, step 500ms
        params.insert("RETRY_DELAY_MS".to_string(), ParamBounds {
            current_value: ParamValue::Integer(1000),
            min: ParamValue::Integer(500),
            max: ParamValue::Integer(5000),
            step: ParamValue::Integer(500),
            description: "Delay between retry attempts".to_string(),
        });

        // DATABASE_MAX_CONNECTIONS: 1-50, step 5
        params.insert("DATABASE_MAX_CONNECTIONS".to_string(), ParamBounds {
            current_value: ParamValue::Integer(5),
            min: ParamValue::Integer(1),
            max: ParamValue::Integer(50),
            step: ParamValue::Integer(5),
            description: "Maximum SQLite connection pool size".to_string(),
        });

        // Quality thresholds
        params.insert("REFLECTION_QUALITY_THRESHOLD".to_string(), ParamBounds {
            current_value: ParamValue::Float(0.8),
            min: ParamValue::Float(0.5),
            max: ParamValue::Float(0.95),
            step: ParamValue::Float(0.05),
            description: "Quality threshold for reflection mode iterations".to_string(),
        });

        params.insert("GOT_PRUNE_THRESHOLD".to_string(), ParamBounds {
            current_value: ParamValue::Float(0.3),
            min: ParamValue::Float(0.1),
            max: ParamValue::Float(0.7),
            step: ParamValue::Float(0.1),
            description: "Score threshold for Graph-of-Thoughts node pruning".to_string(),
        });

        // Toggleable features
        let mut features = HashSet::new();
        features.insert("ENABLE_AUTO_REFLECTION".to_string());
        features.insert("ENABLE_DETECTION_POST_PROCESS".to_string());
        features.insert("ENABLE_GOT_AGGRESSIVE_PRUNING".to_string());
        features.insert("ENABLE_VERBOSE_LOGGING".to_string());

        // Scalable resources
        let mut resources = HashMap::new();
        resources.insert(ResourceType::MaxConcurrentRequests, ResourceBounds {
            min: 1, max: 20, step: 2,
        });
        resources.insert(ResourceType::ConnectionPoolSize, ResourceBounds {
            min: 1, max: 50, step: 5,
        });
        resources.insert(ResourceType::CacheSize, ResourceBounds {
            min: 100, max: 10000, step: 100,
        });

        Self {
            adjustable_params: params,
            toggleable_features: features,
            scalable_resources: resources,
        }
    }

    /// Validate an action against the allowlist
    pub fn validate(&self, action: &SuggestedAction) -> Result<(), AllowlistError> {
        match action {
            SuggestedAction::AdjustParam { key, new_value, .. } => {
                let bounds = self.adjustable_params.get(key)
                    .ok_or(AllowlistError::ParamNotAllowed(key.clone()))?;

                bounds.validate_value(new_value)?;
                bounds.validate_step(&bounds.current_value, new_value)?;
                Ok(())
            }

            SuggestedAction::ToggleFeature { feature_name, .. } => {
                if !self.toggleable_features.contains(feature_name) {
                    return Err(AllowlistError::FeatureNotToggleable(feature_name.clone()));
                }
                Ok(())
            }

            SuggestedAction::ScaleResource { resource, new_value, old_value } => {
                let bounds = self.scalable_resources.get(resource)
                    .ok_or(AllowlistError::ResourceNotScalable(format!("{:?}", resource)))?;

                if *new_value < bounds.min || *new_value > bounds.max {
                    return Err(AllowlistError::ValueOutOfBounds {
                        value: *new_value as i64,
                        min: bounds.min as i64,
                        max: bounds.max as i64,
                    });
                }

                let change = (*new_value as i32 - *old_value as i32).unsigned_abs();
                if change > bounds.step {
                    return Err(AllowlistError::StepTooLarge {
                        change: change as i64,
                        max_step: bounds.step as i64,
                    });
                }

                Ok(())
            }

            SuggestedAction::RestartService { .. } => Ok(()),  // Always allowed
            SuggestedAction::ClearCache { .. } => Ok(()),  // Always allowed
            SuggestedAction::NoOp { .. } => Ok(()),  // Always allowed
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AllowlistError {
    #[error("Parameter not in allowlist: {0}")]
    ParamNotAllowed(String),

    #[error("Feature not toggleable: {0}")]
    FeatureNotToggleable(String),

    #[error("Resource not scalable: {0}")]
    ResourceNotScalable(String),

    #[error("Value {value} out of bounds [{min}, {max}]")]
    ValueOutOfBounds { value: i64, min: i64, max: i64 },

    #[error("Change {change} exceeds max step {max_step}")]
    StepTooLarge { change: i64, max_step: i64 },

    #[error("Invalid value type")]
    InvalidValueType,
}
```

---

## 6. Baseline Calculator

```rust
// src/self_improvement/baseline.rs

use chrono::{DateTime, Duration, Utc};

/// Hybrid baseline calculator using EMA for trend detection
/// and rolling average for stable thresholds
pub struct BaselineCalculator {
    config: BaselineConfig,
}

#[derive(Debug, Clone)]
pub struct BaselineConfig {
    /// EMA smoothing factor (0 < alpha < 1)
    /// Lower = smoother, less responsive
    /// Higher = more responsive, more noise
    pub ema_alpha: f64,

    /// Rolling average window
    pub rolling_window: Duration,

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
            rolling_window: Duration::hours(24),
            min_samples: 100,
            warning_multiplier: 1.5,
            critical_multiplier: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MetricBaseline {
    pub metric_name: String,
    pub rolling_avg: f64,
    pub rolling_sample_count: usize,
    pub ema_value: f64,
    pub warning_threshold: f64,
    pub critical_threshold: f64,
    pub last_updated: DateTime<Utc>,
    pub is_valid: bool,
}

impl BaselineCalculator {
    pub fn new(config: BaselineConfig) -> Self {
        Self { config }
    }

    /// Update baseline with new observation
    pub fn update(&self, baseline: &mut MetricBaseline, new_value: f64, timestamp: DateTime<Utc>) {
        // Update EMA (more responsive to recent changes)
        baseline.ema_value = self.config.ema_alpha * new_value
            + (1.0 - self.config.ema_alpha) * baseline.ema_value;

        // Update rolling average (more stable)
        // In practice, this would query the database for values in the window
        // Here we use a simplified incremental update
        let n = baseline.rolling_sample_count as f64;
        baseline.rolling_avg = (baseline.rolling_avg * n + new_value) / (n + 1.0);
        baseline.rolling_sample_count += 1;

        // Calculate thresholds based on rolling average
        baseline.warning_threshold = baseline.rolling_avg * self.config.warning_multiplier;
        baseline.critical_threshold = baseline.rolling_avg * self.config.critical_multiplier;

        baseline.last_updated = timestamp;
        baseline.is_valid = baseline.rolling_sample_count >= self.config.min_samples;
    }

    /// Check if a value triggers an alert
    pub fn check_trigger(&self, baseline: &MetricBaseline, value: f64) -> Option<TriggerLevel> {
        if !baseline.is_valid {
            return None;  // Not enough data yet
        }

        // Use EMA for trend detection (are we moving away from normal?)
        let ema_deviation = (value - baseline.ema_value).abs() / baseline.ema_value;

        // Use rolling avg thresholds for stable alerting
        if value >= baseline.critical_threshold {
            Some(TriggerLevel::Critical)
        } else if value >= baseline.warning_threshold {
            Some(TriggerLevel::Warning)
        } else if ema_deviation > 0.5 {
            // EMA shows significant trend change even if absolute value is OK
            Some(TriggerLevel::Trend)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerLevel {
    Trend,      // EMA indicates trend change
    Warning,    // Above warning threshold
    Critical,   // Above critical threshold
}
```

---

## 7. Circuit Breaker

```rust
// src/self_improvement/circuit_breaker.rs

use chrono::{DateTime, Duration, Utc};

/// Circuit breaker to prevent cascading failures
/// from repeated bad self-improvement actions
pub struct CircuitBreaker {
    state: CircuitState,
    consecutive_failures: u32,
    consecutive_successes: u32,
    last_failure: Option<DateTime<Utc>>,
    last_state_change: DateTime<Utc>,
    config: CircuitBreakerConfig,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: u32,

    /// Number of consecutive successes in half-open to close circuit
    pub success_threshold: u32,

    /// Time to wait before attempting recovery (half-open)
    pub recovery_timeout: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            success_threshold: 2,
            recovery_timeout: Duration::hours(1),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - actions allowed
    Closed,
    /// Blocking all actions - too many failures
    Open,
    /// Testing recovery - allowing one action
    HalfOpen,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            consecutive_failures: 0,
            consecutive_successes: 0,
            last_failure: None,
            last_state_change: Utc::now(),
            config,
        }
    }

    /// Check if an action can be executed
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(last_fail) = self.last_failure {
                    if Utc::now() - last_fail >= self.config.recovery_timeout {
                        self.transition_to(CircuitState::HalfOpen);
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,  // Allow one test execution
        }
    }

    /// Record a successful action
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.consecutive_successes += 1;

        match self.state {
            CircuitState::HalfOpen => {
                if self.consecutive_successes >= self.config.success_threshold {
                    self.transition_to(CircuitState::Closed);
                }
            }
            CircuitState::Open => {
                // Shouldn't happen, but handle gracefully
                self.transition_to(CircuitState::HalfOpen);
            }
            CircuitState::Closed => {}
        }
    }

    /// Record a failed action
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.consecutive_successes = 0;
        self.last_failure = Some(Utc::now());

        match self.state {
            CircuitState::Closed => {
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => {
                // Failed during recovery - go back to open
                self.transition_to(CircuitState::Open);
            }
            CircuitState::Open => {}
        }
    }

    fn transition_to(&mut self, new_state: CircuitState) {
        tracing::info!(
            from = ?self.state,
            to = ?new_state,
            failures = self.consecutive_failures,
            successes = self.consecutive_successes,
            "Circuit breaker state transition"
        );
        self.state = new_state;
        self.last_state_change = Utc::now();
    }

    pub fn state(&self) -> CircuitState {
        self.state
    }
}
```

---

## 8. Configuration

```rust
// src/self_improvement/config.rs

use std::time::Duration;

/// Configuration for the self-improvement system
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
}

impl Default for SelfImprovementConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor: MonitorConfig::default(),
            analyzer: AnalyzerConfig::default(),
            executor: ExecutorConfig::default(),
            learner: LearnerConfig::default(),
            circuit_breaker: CircuitBreakerConfig::default(),
            baseline: BaselineConfig::default(),
        }
    }
}

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
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: 300,  // 5 minutes
            error_rate_threshold: 0.05,  // 5%
            latency_threshold_ms: 5000,  // 5 seconds
            quality_threshold: 0.7,
            fallback_rate_threshold: 0.1,  // 10%
            min_sample_size: 50,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    /// Maximum diagnoses to keep pending
    pub max_pending_diagnoses: usize,

    /// Whether to use Reflection mode for diagnosis
    pub use_reflection_for_diagnosis: bool,

    /// Minimum severity to generate action
    pub min_action_severity: Severity,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_pending_diagnoses: 10,
            use_reflection_for_diagnosis: true,
            min_action_severity: Severity::Warning,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum actions per hour
    pub max_actions_per_hour: u32,

    /// Cooldown duration after successful action
    pub cooldown_duration: Duration,

    /// Verification timeout
    pub verification_timeout: Duration,

    /// Auto-rollback if reward is negative
    pub rollback_on_regression: bool,

    /// Time to wait for metrics to stabilize after change
    pub stabilization_period: Duration,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_actions_per_hour: 3,
            cooldown_duration: Duration::from_secs(3600),  // 1 hour
            verification_timeout: Duration::from_secs(60),
            rollback_on_regression: true,
            stabilization_period: Duration::from_secs(120),  // 2 minutes
        }
    }
}

#[derive(Debug, Clone)]
pub struct LearnerConfig {
    /// Minimum reward to consider action effective
    pub effective_reward_threshold: f64,

    /// Weight for historical effectiveness in action selection
    pub history_weight: f64,

    /// Maximum history entries per action type
    pub max_history_per_action: usize,
}

impl Default for LearnerConfig {
    fn default() -> Self {
        Self {
            effective_reward_threshold: 0.1,
            history_weight: 0.3,
            max_history_per_action: 100,
        }
    }
}
```

---

## 9. Langbase Pipe Integration

> **Full Details**: See `claudedocs/PIPE_INTEGRATION_STRATEGY.md`

### Strategy: Existing Pipes First

After multi-criteria decision analysis, the recommended approach is to **use existing pipes with tailored prompts** rather than creating dedicated self-improvement pipes.

| Decision | Score | Rationale |
|----------|-------|-----------|
| Use existing pipes | 0.770 | Faster development, no new maintenance burden |
| Hybrid (one new pipe) | 0.775 | Marginally better but adds maintenance |
| **Recommendation** | Existing | Start simple, specialize only if metrics indicate need |

### Pipe Usage by Phase

```
┌─────────┐     ┌───────────────┐     ┌────────────────────┐     ┌─────────────┐
│ Monitor │────▶│ reflection-v1 │────▶│ decision-framework │────▶│ [Execute]   │
│ (local) │     │ (diagnosis)   │     │ (action select)    │     │ (local)     │
└─────────┘     └───────────────┘     └────────────────────┘     └──────┬──────┘
                                                                        │
                                                       [Optional: detection-v1 validation]
                                                                        │
                                                                        ▼
                                                                 ┌─────────────┐
                                                                 │ reflection-v1│
                                                                 │ (learning)   │
                                                                 └─────────────┘
```

| Phase | Pipe | Purpose |
|-------|------|---------|
| Monitor | None | Pure metrics aggregation from SQLite |
| Analyzer (Diagnosis) | `reflection-v1` | Root cause analysis with tailored health prompt |
| Analyzer (Action) | `decision-framework-v1` | Multi-criteria action selection |
| Executor (Validate) | `detection-v1` | Optional bias/fallacy check on decision |
| Learner | `reflection-v1` | Outcome synthesis and lessons extraction |

### PipeConfig

```rust
#[derive(Debug, Clone)]
pub struct PipeConfig {
    pub diagnosis_pipe: String,      // default: "reflection-v1"
    pub decision_pipe: String,       // default: "decision-framework-v1"
    pub detection_pipe: String,      // default: "detection-v1"
    pub learning_pipe: String,       // default: "reflection-v1"
    pub enable_validation: bool,     // default: true
    pub pipe_timeout_ms: u64,        // default: 30000
}
```

### Trigger for New Pipe Creation

Create `self-diagnosis-v1` only if ANY threshold is exceeded:

| Metric | Threshold | Action |
|--------|-----------|--------|
| Diagnosis JSON parse failures | > 20% | Create specialized pipe with strict output schema |
| Diagnosis latency | > 10s average | Use smaller/faster model in dedicated pipe |
| Action positive reward rate | < 40% | Improve prompts or create specialized action-selection pipe |

### Safety: Pipe Failure Handling

```rust
// If diagnosis pipe fails, NEVER take action
if diagnosis_result.is_err() {
    tracing::warn!("Diagnosis pipe unavailable, deferring action");
    return Ok(SuggestedAction::NoOp {
        reason: "Diagnosis unavailable".into(),
        revisit_after: Duration::from_secs(300),
    });
}
```

---

## 10. CLI Commands

```rust
// src/self_improvement/cli.rs

use clap::{Parser, Subcommand};

#[derive(Subcommand)]
pub enum SelfImproveCommands {
    /// Show current self-improvement status
    Status,

    /// Show history of actions
    History {
        #[arg(long, default_value = "20")]
        limit: usize,

        #[arg(long)]
        since: Option<String>,

        #[arg(long)]
        outcome: Option<String>,  // 'success', 'failed', 'rolled_back'
    },

    /// Enable self-improvement
    Enable,

    /// Disable self-improvement
    Disable,

    /// Pause for a duration
    Pause {
        #[arg(long)]
        duration: String,  // e.g., "2h", "30m"
    },

    /// Rollback a specific action
    Rollback {
        action_id: String,
    },

    /// Approve a pending action (if manual approval is enabled)
    Approve {
        diagnosis_id: String,
    },

    /// Reject a pending action
    Reject {
        diagnosis_id: String,

        #[arg(long)]
        reason: Option<String>,
    },

    /// Configuration commands
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Run diagnostics
    Diagnostics {
        #[arg(long)]
        verbose: bool,
    },

    /// Simulate an action without executing
    Simulate {
        /// Action type: 'adjust_param', 'toggle_feature', etc.
        action_type: String,

        /// Action parameters as JSON
        #[arg(long)]
        params: String,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// List all adjustable parameters
    List,

    /// Show current value of a parameter
    Get { key: String },

    /// Show allowlist bounds for a parameter
    Bounds { key: String },
}

/// Example CLI output for `selfimprove status`:
///
/// ```text
/// Self-Improvement Status
/// =======================
/// State: ACTIVE (cooldown: 45m remaining)
/// Circuit Breaker: CLOSED (0 consecutive failures)
///
/// Current Metrics:
///   Error Rate:    0.3% (baseline: 0.5%, threshold: 2.5%)  ✓
///   Latency P95:   120ms (baseline: 100ms, threshold: 200ms)  ✓
///   Quality Score: 0.85 (baseline: 0.80, minimum: 0.70)  ✓
///   Fallback Rate: 1.2% (baseline: 2.0%, threshold: 10.0%)  ✓
///
/// Recent Actions (last 24h): 2
///   [SUCCESS] 2h ago: Increased REQUEST_TIMEOUT_MS 30000 → 35000
///   [ROLLED_BACK] 8h ago: Decreased MAX_RETRIES 3 → 2 (metrics regressed)
///
/// Pending Diagnoses: 0
/// ```
```

---

## 11. Integration Points

### 11.1 Extend AppState

```rust
// In src/server/mod.rs, add to AppState:

pub struct AppState {
    // ... existing fields ...

    /// Self-improvement system (optional)
    pub self_improvement: Option<Arc<SelfImprovementSystem>>,
}

impl AppState {
    pub fn new(config: Config, storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        // ... existing initialization ...

        let self_improvement = if config.self_improvement.enabled {
            Some(Arc::new(SelfImprovementSystem::new(
                storage.clone(),
                config.self_improvement.clone(),
            )))
        } else {
            None
        };

        Self {
            // ... existing fields ...
            self_improvement,
        }
    }
}
```

### 11.2 Extend Config

```rust
// In src/config/mod.rs, add:

pub struct Config {
    // ... existing fields ...

    /// Self-improvement configuration
    pub self_improvement: SelfImprovementConfig,
}

// Load from environment:
let self_improvement = SelfImprovementConfig {
    enabled: env::var("SELF_IMPROVEMENT_ENABLED")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false),
    // ... other fields from env vars ...
};
```

### 11.3 Post-Invocation Hook

```rust
// In src/server/handlers.rs, after each tool call:

pub async fn handle_tool_call(
    state: &AppState,
    tool_name: &str,
    params: Value,
) -> Result<ToolResponse, ToolError> {
    let start = Instant::now();

    // Execute tool
    let result = match tool_name {
        // ... existing handlers ...
    };

    let latency_ms = start.elapsed().as_millis() as i64;
    let success = result.is_ok();

    // Notify self-improvement system
    if let Some(ref si) = state.self_improvement {
        si.on_invocation(InvocationEvent {
            tool_name: tool_name.to_string(),
            latency_ms,
            success,
            quality_score: result.as_ref().ok().and_then(|r| r.quality_score()),
            timestamp: Utc::now(),
        }).await;
    }

    result
}
```

---

## 12. Implementation Timeline

| Week | Phase | Deliverables |
|------|-------|-------------|
| 1-2 | Foundation | Database schema, types.rs, config.rs, allowlist.rs, pipes.rs |
| 3 | Monitor | monitor.rs, baseline.rs, health checks, trigger detection |
| 4 | Analyzer | analyzer.rs, diagnosis generation via pipes, action selection |
| 5-6 | Executor | executor.rs, rollback, verification, cooldown, circuit_breaker.rs |
| 7 | Learner | learner.rs, reward calculation, effectiveness tracking, learning synthesis |
| 8 | Integration | cli.rs, AppState integration, pipe metrics tracking, testing, documentation |

---

## 13. Testing Strategy

### Unit Tests
- All types serialization/deserialization
- Allowlist validation logic
- Baseline calculation accuracy
- Circuit breaker state transitions
- Reward normalization
- Pipe prompt template generation
- Pipe response JSON parsing

### Integration Tests
- Monitor → Analyzer flow
- Executor backup/rollback
- Full loop with mock storage
- CLI commands
- Pipe call sequence with mocked Langbase client
- Pipe failure fallback behavior (NoOp on unavailability)

### Property-Based Tests (proptest)
- Reward calculations for random metrics
- Circuit breaker invariants
- Baseline stability under noise

### Pipe-Specific Tests
- JSON output schema compliance from each pipe
- Prompt template variable substitution
- Parse failure tracking and threshold alerting
- Timeout handling and fallback

---

## 14. Reasoning Tools Used

This design was developed using the following Langbase reasoning tools:

1. **Graph-of-Thoughts (GoT)**: Explored design space with branching thoughts, scored and aggregated insights
2. **Tree Reasoning**: Evaluated 4 design approaches for each phase with confidence scoring
3. **Divergent Thinking**: Generated unconventional approaches and challenged assumptions
4. **Reflection**: Identified weaknesses and gaps in the design
5. **Decision Making**: Multi-criteria analysis for baseline method selection

**Key Insights from Reasoning:**
- Hybrid baseline method (EMA + Rolling Avg) scored highest (0.84) for balancing stability and responsiveness
- Verification testing suite identified as terminal candidate (score: 0.85) for production readiness
- Reflection revealed need for clearer guidelines on dynamic reward weight adjustment
- Divergent thinking suggested anomaly detection for baseline adjustment (implemented)

---

## 15. References

- Original proposal: `claudedocs/test-plan-prompts.md`
- **Pipe integration strategy**: `claudedocs/PIPE_INTEGRATION_STRATEGY.md`
- Previous analysis: Earlier conversation in this session
- Existing codebase: `src/config/mod.rs`, `src/server/mod.rs`, `src/storage/mod.rs`
- Langbase reasoning sessions:
  - GoT (main design): `3ada8e4d-d3a1-442a-88cf-07e4455c9aa8`
  - GoT (pipe strategy): `35fdffd6-dceb-48c9-8b1c-92be188826e7`
  - Tree: `ee896b5e-1f81-43a3-b3c3-3a152e90061d`
  - Reflection: `b29dc6a9-88e1-4e94-bd48-2522a14668e2`
  - Decision (pipe selection): `08b10a00-0d22-4145-b0f2-40e357436f60`
