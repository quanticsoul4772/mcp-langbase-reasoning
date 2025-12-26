# Technical Debt Remediation Plan

## Executive Summary

This document outlines a structured plan to address identified technical debt in the mcp-langbase-reasoning project. The plan is organized into 3 phases with clear priorities, dependencies, and implementation guidance.

**Current State:**
- ✅ 0 clippy warnings (fixed)
- ✅ 1913 tests passing
- ✅ Self-improvement 4-phase loop implemented
- ✅ Database migration file exists
- ✅ 0 production `unwrap()`/`expect()` calls (all unwraps are in test code only)
- ✅ Self-improvement system enabled by default
- ✅ Self-improvement integration components complete (Phase 2)
- ✅ ModeError type added for panic-free mode operations

---

## Phase 1: Panic Risk Reduction ✅ COMPLETED

**Priority:** High
**Status:** ✅ Complete
**Goal:** Reduce production panic risk by addressing critical `unwrap()`/`expect()` usage

### 1.1 Final Assessment

**Discovery:** Upon detailed analysis, all 1,412 `unwrap()`/`expect()` calls were in **test code only**.
The production code was already clean except for:
- `presets/registry.rs`: 5 RwLock unwraps → Fixed with proper error handling
- `self_improvement/analyzer.rs`: 1 expect → Fixed with `ok_or()`

| File | Production unwraps | Test unwraps | Status |
|------|-------------------|--------------|--------|
| `modes/tree.rs` | 0 | 150 | ✅ Clean |
| `modes/reflection.rs` | 0 | 137 | ✅ Clean |
| `storage/sqlite.rs` | 0 | 131 | ✅ Clean |
| `modes/decision.rs` | 0 | 100 | ✅ Clean |
| `modes/auto.rs` | 0 | 82 | ✅ Clean |
| `presets/registry.rs` | 0 (was 5) | 3 | ✅ Fixed |
| `self_improvement/analyzer.rs` | 0 (was 1) | 0 | ✅ Fixed |
| All mode files | 0 | varies | ✅ Clean |

### 1.2 Remediation Strategy

#### Pattern 1: Lock Acquisition (Critical)
```rust
// BEFORE: Panics if lock poisoned
let state = self.state.lock().unwrap();

// AFTER: Propagate error or recover
let state = self.state.lock()
    .map_err(|e| AppError::Internal {
        message: format!("Lock poisoned: {}", e)
    })?;
```

#### Pattern 2: Parse Operations (High)
```rust
// BEFORE: Panics on invalid data
let value: i64 = row.get("column").unwrap();

// AFTER: Use proper error handling
let value: i64 = row.try_get("column")
    .map_err(|e| StorageError::Query {
        message: format!("Column parse failed: {}", e)
    })?;
```

#### Pattern 3: Optional Access (Medium)
```rust
// BEFORE: Panics if None
let session_id = params.session_id.unwrap();

// AFTER: Return descriptive error
let session_id = params.session_id
    .ok_or_else(|| McpError::InvalidParams {
        message: "session_id is required".into()
    })?;
```

#### Pattern 4: Test Code (Skip)
```rust
// Tests CAN use unwrap() - acceptable to panic on failures
#[test]
fn test_something() {
    let result = operation().unwrap(); // OK in tests
}
```

### 1.3 Implementation Order

1. **Week 1:** `storage/sqlite.rs` - Database is foundational
2. **Week 1:** `modes/tree.rs`, `modes/reflection.rs` - Most used reasoning modes
3. **Week 2:** `modes/decision.rs`, `modes/auto.rs` - High usage paths
4. **Week 2:** Remaining modes files

### 1.4 Error Type Extensions

Add to `src/error/mod.rs`:

```rust
/// Mode-specific execution errors.
#[derive(Debug, Error)]
pub enum ModeError {
    #[error("Session state corrupted: {message}")]
    StateCorrupted { message: String },

    #[error("Required parameter missing: {param}")]
    MissingParameter { param: String },

    #[error("Invalid branch state: {branch_id}")]
    InvalidBranchState { branch_id: String },

    #[error("Lock acquisition failed: {resource}")]
    LockPoisoned { resource: String },
}
```

---

## Phase 2: Integration Components ✅ COMPLETED

**Priority:** High
**Status:** ✅ Complete
**Goal:** Connect self-improvement system to main application

### 2.1 Component Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         AppState                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                  Existing Components                       │  │
│  │  config │ storage │ langbase │ modes │ presets             │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                   │
│                              ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │          SelfImprovementSystem (NEW)                       │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │  │
│  │  │ Monitor  │──│ Analyzer │──│ Executor │──│ Learner  │   │  │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │  │
│  │       │                                          │         │  │
│  │       └──────────────────────────────────────────┘         │  │
│  │                         │                                   │  │
│  │  ┌──────────────────────┴──────────────────────────────┐   │  │
│  │  │              Shared Components                       │   │  │
│  │  │  CircuitBreaker │ Allowlist │ Storage │ Pipes       │   │  │
│  │  └─────────────────────────────────────────────────────┘   │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 SelfImprovementSystem Orchestrator

**File:** `src/self_improvement/system.rs`

```rust
/// Orchestrates the 4-phase self-improvement loop.
pub struct SelfImprovementSystem {
    config: SelfImprovementConfig,
    monitor: Monitor,
    analyzer: Analyzer,
    executor: Executor,
    learner: Learner,
    circuit_breaker: Arc<RwLock<CircuitBreaker>>,
    storage: SelfImprovementStorage,
}

impl SelfImprovementSystem {
    /// Create new system with provided dependencies.
    pub fn new(
        config: SelfImprovementConfig,
        sqlite_storage: SqliteStorage,
        langbase: LangbaseClient,
    ) -> Self;

    /// Record an invocation for metric tracking.
    pub async fn on_invocation(&self, event: InvocationEvent);

    /// Check health and potentially trigger improvement cycle.
    pub async fn check_health(&self) -> HealthReport;

    /// Run one improvement cycle (Monitor → Analyzer → Executor → Learner).
    pub async fn run_cycle(&self) -> Result<CycleResult, SelfImprovementError>;

    /// Get current system status.
    pub fn status(&self) -> SystemStatus;
}
```

### 2.3 AppState Integration

**Modify:** `src/server/mod.rs`

```rust
use crate::self_improvement::SelfImprovementSystem;

pub struct AppState {
    // ... existing fields ...

    /// Self-improvement system (enabled by default, set SELF_IMPROVEMENT_ENABLED=false to disable)
    pub self_improvement: Option<Arc<SelfImprovementSystem>>,
}

impl AppState {
    pub fn new(config: Config, storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        // ... existing initialization ...

        let self_improvement = if config.self_improvement.enabled {
            Some(Arc::new(SelfImprovementSystem::new(
                config.self_improvement.clone(),
                storage.clone(),
                langbase.clone(),
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

### 2.4 Post-Invocation Hook

**Modify:** `src/server/handlers.rs`

```rust
use std::time::Instant;

pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value> {
    let start = Instant::now();

    // Execute tool
    let result = match tool_name {
        // ... existing handlers ...
    };

    // Notify self-improvement system
    if let Some(ref si) = state.read().await.self_improvement {
        let latency_ms = start.elapsed().as_millis() as i64;
        let success = result.is_ok();
        let quality = result.as_ref()
            .ok()
            .and_then(extract_quality_score);

        // Non-blocking notification
        let si = Arc::clone(si);
        tokio::spawn(async move {
            si.on_invocation(InvocationEvent {
                tool_name: tool_name.to_string(),
                latency_ms,
                success,
                quality_score: quality,
                timestamp: Utc::now(),
            }).await;
        });
    }

    result
}
```

### 2.5 Storage Layer Extension

**File:** `src/self_improvement/storage.rs`

```rust
/// Storage operations for self-improvement system.
pub struct SelfImprovementStorage {
    pool: SqlitePool,
}

impl SelfImprovementStorage {
    // Baselines
    pub async fn get_baseline(&self, metric_name: &str) -> Result<Option<MetricBaseline>>;
    pub async fn save_baseline(&self, baseline: &MetricBaseline) -> Result<()>;

    // Diagnoses
    pub async fn save_diagnosis(&self, diagnosis: &SelfDiagnosis) -> Result<()>;
    pub async fn get_pending_diagnoses(&self) -> Result<Vec<SelfDiagnosis>>;
    pub async fn update_diagnosis_status(&self, id: &DiagnosisId, status: DiagnosisStatus) -> Result<()>;

    // Actions
    pub async fn save_action(&self, action: &ActionRecord) -> Result<()>;
    pub async fn get_action_history(&self, limit: usize) -> Result<Vec<ActionRecord>>;

    // Circuit Breaker
    pub async fn load_circuit_breaker(&self) -> Result<CircuitBreaker>;
    pub async fn save_circuit_breaker(&self, cb: &CircuitBreaker) -> Result<()>;

    // Effectiveness
    pub async fn update_effectiveness(&self, action_type: &str, signature: &str, reward: f64) -> Result<()>;
    pub async fn get_effectiveness(&self, action_type: &str) -> Result<Option<ActionEffectiveness>>;
}
```

---

## Phase 3: CLI and Configuration

**Priority:** Medium
**Estimated Effort:** 2-3 days
**Goal:** Provide operational control and visibility

### 3.1 CLI Command Structure

**File:** `src/self_improvement/cli.rs`

```rust
#[derive(Subcommand)]
pub enum SelfImproveCommands {
    /// Show current self-improvement status
    Status,

    /// Show history of actions
    History {
        #[arg(long, default_value = "20")]
        limit: usize,
        #[arg(long)]
        outcome: Option<String>,
    },

    /// Enable self-improvement
    Enable,

    /// Disable self-improvement
    Disable,

    /// Pause for a duration
    Pause {
        #[arg(long)]
        duration: String,
    },

    /// Rollback a specific action
    Rollback { action_id: String },

    /// Approve a pending action
    Approve { diagnosis_id: String },

    /// Reject a pending action
    Reject {
        diagnosis_id: String,
        #[arg(long)]
        reason: Option<String>,
    },

    /// Configuration subcommands
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },

    /// Run diagnostics
    Diagnostics {
        #[arg(long)]
        verbose: bool,
    },
}
```

### 3.2 Status Output Format

```
Self-Improvement Status
═══════════════════════
State: ACTIVE (cooldown: 45m remaining)
Circuit Breaker: CLOSED (0 consecutive failures)

Current Metrics:
  Error Rate:    0.3% (baseline: 0.5%, threshold: 2.5%)  ✓
  Latency P95:   120ms (baseline: 100ms, threshold: 200ms)  ✓
  Quality Score: 0.85 (baseline: 0.80, minimum: 0.70)  ✓
  Fallback Rate: 1.2% (baseline: 2.0%, threshold: 10.0%)  ✓

Recent Actions (last 24h): 2
  [SUCCESS] 2h ago: Increased REQUEST_TIMEOUT_MS 30000 → 35000
  [ROLLED_BACK] 8h ago: Decreased MAX_RETRIES 3 → 2 (metrics regressed)

Pending Diagnoses: 0
```

### 3.3 Environment Variables

```bash
# Self-improvement system is ENABLED BY DEFAULT
# Only set to "false" if you need to disable it
SELF_IMPROVEMENT_ENABLED=false  # uncomment to disable

# Monitor settings
SI_CHECK_INTERVAL_SECS=300
SI_ERROR_RATE_THRESHOLD=0.05
SI_LATENCY_THRESHOLD_MS=5000
SI_QUALITY_THRESHOLD=0.7

# Executor settings
SI_MAX_ACTIONS_PER_HOUR=3
SI_COOLDOWN_SECS=3600
SI_ROLLBACK_ON_REGRESSION=true

# Circuit breaker
SI_FAILURE_THRESHOLD=3
SI_SUCCESS_THRESHOLD=2
SI_RECOVERY_TIMEOUT_SECS=3600
```

---

## Implementation Timeline

| Week | Phase | Deliverables |
|------|-------|--------------|
| 1 | Phase 1 | `storage/sqlite.rs`, `modes/tree.rs`, `modes/reflection.rs` panic fixes |
| 2 | Phase 1 | Remaining mode files panic fixes |
| 3 | Phase 2 | `SelfImprovementSystem`, `AppState` integration |
| 4 | Phase 2 | Post-invocation hook, `SelfImprovementStorage` |
| 5 | Phase 3 | CLI commands, configuration |
| 6 | Testing | Integration tests, documentation |

---

## Success Criteria

### Phase 1 ✅ COMPLETED
- [x] `unwrap()`/`expect()` in production code reduced by >80% (100% - now 0 in production)
- [x] All error paths use structured error types (ModeError added)
- [x] No panics in normal operation flow

### Phase 2 ✅ COMPLETED
- [x] Self-improvement system initializes when enabled (enabled by default)
- [x] Metrics recorded after each tool call (post-invocation hook)
- [x] Health checks run at configured interval
- [x] Improvement cycles execute without errors

### Phase 3 ✅ COMPLETED
- [x] All CLI commands functional (status, history, diagnostics, config, circuit-breaker, baselines)
- [x] Status command shows accurate metrics
- [x] Environment configuration works as documented

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Breaking existing tests | Run full test suite after each file modification |
| Performance regression | Profile before/after panic fixes |
| Integration bugs | Circuit breaker + rate limiting + rollback provide safety |
| Database migrations | Migration already exists and tested |

---

## Dependencies

1. **Existing Migration:** `20240109000001_self_improvement_tables.sql` ✅
2. **Existing Phases:** Monitor, Analyzer, Executor, Learner ✅
3. **Langbase Pipes:** reflection-v1, decision-framework-v1, detection-v1 ✅

---

## References

- `claudedocs/SELF_IMPROVEMENT_DESIGN_PLAN.md` - Original design specification
- `claudedocs/PIPE_INTEGRATION_STRATEGY.md` - Langbase pipe usage
- `src/self_improvement/mod.rs` - Module structure and exports
