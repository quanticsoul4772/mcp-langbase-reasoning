# Silent Failures Fix Plan

## Overview

This plan addresses 5 categories of silent failures identified in the codebase where errors are either discarded (`let _ =`) or logged but not propagated, leading to data loss and debugging difficulties.

## Design Decisions

### Strategy Selection

| Issue | Recommended Strategy | Rationale |
|-------|---------------------|-----------|
| Invocation Logging | Log error + continue | Audit logging is secondary; shouldn't fail primary operation |
| Preset Registration | Fail fast on startup | Missing presets = broken functionality |
| .env Loading | Distinguish error types | Missing file OK, parse errors should warn |
| Decision/Evidence Storage | Propagate error | Data persistence is critical to operation semantics |

---

## Phase 1: Invocation Logging (Non-Blocking Fix)

**Files:** `auto.rs`, `got.rs`
**Pattern:** `let _ = self.core.storage().log_invocation(&invocation).await;`

### Current Behavior
Invocation logging errors are completely ignored - no log, no trace.

### Proposed Fix
Log the error but don't fail the operation. Invocation logging is an audit concern, not a functional requirement.

```rust
// Before:
let _ = self.core.storage().log_invocation(&invocation).await;

// After:
if let Err(e) = self.core.storage().log_invocation(&invocation).await {
    warn!(
        error = %e,
        tool = %invocation.tool,
        "Failed to log invocation - audit trail incomplete"
    );
}
```

### Affected Locations

| File | Lines | Context |
|------|-------|---------|
| `src/modes/auto.rs` | 153 | Error path - Langbase call failed |
| `src/modes/auto.rs` | 181 | Success path - after processing |
| `src/modes/got.rs` | 738 | Error path - generate failed |
| `src/modes/got.rs` | 784 | Success path - generate completed |
| `src/modes/got.rs` | 840 | Error path - score failed |
| `src/modes/got.rs` | 858 | Success path - score completed |
| `src/modes/got.rs` | 933 | Error path - aggregate failed |
| `src/modes/got.rs` | 970 | Success path - aggregate completed |
| `src/modes/got.rs` | 1029 | Error path - refine failed |
| `src/modes/got.rs` | 1061 | Success path - refine completed |

**Total: 10 locations**

---

## Phase 2: Preset Registration (Fail-Fast Fix)

**File:** `presets/registry.rs`
**Pattern:** `let _ = self.register(...);`

### Current Behavior
Preset registration errors are silently ignored. If a builtin preset fails to register, the system appears functional but that preset is unavailable.

### Proposed Fix
Log errors during registration. Since this happens at startup, failures indicate a programming error (duplicate IDs, invalid presets) rather than runtime issues.

```rust
// Before:
fn register_builtins(&self) {
    let _ = self.register(builtins::code_review_preset());
    let _ = self.register(builtins::debug_analysis_preset());
    let _ = self.register(builtins::architecture_decision_preset());
    let _ = self.register(builtins::strategic_decision_preset());
    let _ = self.register(builtins::evidence_based_conclusion_preset());
}

// After:
fn register_builtins(&self) {
    let presets = [
        ("code-review", builtins::code_review_preset()),
        ("debug-analysis", builtins::debug_analysis_preset()),
        ("architecture-decision", builtins::architecture_decision_preset()),
        ("strategic-decision", builtins::strategic_decision_preset()),
        ("evidence-based-conclusion", builtins::evidence_based_conclusion_preset()),
    ];

    for (name, preset) in presets {
        if let Err(e) = self.register(preset) {
            error!(
                preset = name,
                error = %e,
                "Failed to register builtin preset - this indicates a programming error"
            );
        }
    }
}
```

### Affected Locations

| File | Lines | Preset |
|------|-------|--------|
| `src/presets/registry.rs` | 83 | code-review |
| `src/presets/registry.rs` | 84 | debug-analysis |
| `src/presets/registry.rs` | 87 | architecture-decision |
| `src/presets/registry.rs` | 90 | strategic-decision |
| `src/presets/registry.rs` | 93 | evidence-based-conclusion |

**Total: 5 registrations (consolidate to 1 loop)**

---

## Phase 3: .env Loading (Discriminated Error Handling)

**File:** `config/mod.rs`
**Pattern:** `let _ = dotenvy::dotenv();`

### Current Behavior
All .env loading errors are ignored, including:
- Missing file (acceptable)
- Parse errors (should warn)
- Permission errors (should warn)

### Proposed Fix
Distinguish between "file not found" (normal) and other errors (should log).

```rust
// Before:
let _ = dotenvy::dotenv();

// After:
match dotenvy::dotenv() {
    Ok(path) => {
        debug!(path = %path.display(), "Loaded .env file");
    }
    Err(dotenvy::Error::Io(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
        // .env file not found - this is normal, use environment variables
        debug!("No .env file found, using environment variables");
    }
    Err(e) => {
        warn!(
            error = %e,
            "Failed to load .env file - check file permissions and syntax"
        );
    }
}
```

### Affected Locations

| File | Line | Context |
|------|------|---------|
| `src/config/mod.rs` | 152 | Config::from_env() initialization |

**Total: 1 location**

---

## Phase 4: Decision Storage (Error Propagation)

**Files:** `decision.rs`
**Pattern:** Storage errors logged but operation returns success

### Current Behavior
```rust
if let Err(e) = self.core.storage().create_decision(&stored_decision).await {
    warn!(error = %e, "Failed to persist decision to storage");
}
// Returns Ok(result) - user thinks data was saved!
```

### Design Decision: Should Storage Failures Be Fatal?

**Arguments for making it fatal (RECOMMENDED):**
- Data persistence is a core expectation of the operation
- Users expect decisions/evidence to be queryable later
- "Silent success" violates principle of least surprise
- Debugging becomes nearly impossible

**Arguments against:**
- Primary value is the reasoning result, not storage
- Could add `persist: bool` flag for soft-persistence mode

### Proposed Fix
Propagate storage errors. The operation semantics include persistence.

```rust
// Before:
if let Err(e) = self.core.storage().create_decision(&stored_decision).await {
    warn!(error = %e, "Failed to persist decision to storage");
}

// After:
self.core.storage().create_decision(&stored_decision).await.map_err(|e| {
    error!(
        error = %e,
        decision_id = %decision_id,
        "Failed to persist decision - operation failed"
    );
    e
})?;
```

### Affected Locations

| File | Lines | Operation |
|------|-------|-----------|
| `src/modes/decision.rs` | 499-505 | create_decision |
| `src/modes/decision.rs` | 634-637 | create_perspective |

**Total: 2 locations**

---

## Phase 5: Evidence Storage (Error Propagation)

**Files:** `evidence.rs`
**Pattern:** Same as decision mode

### Proposed Fix
Same approach as Phase 4.

```rust
// Before:
if let Err(e) = self.core.storage().create_evidence_assessment(&stored_evidence).await {
    warn!(error = %e, "Failed to persist evidence assessment to storage");
}

// After:
self.core.storage().create_evidence_assessment(&stored_evidence).await.map_err(|e| {
    error!(
        error = %e,
        assessment_id = %assessment_id,
        "Failed to persist evidence assessment - operation failed"
    );
    e
})?;
```

### Affected Locations

| File | Lines | Operation |
|------|-------|-----------|
| `src/modes/evidence.rs` | 610-616 | create_evidence_assessment |
| `src/modes/evidence.rs` | 771-776 | create_probability_update |

**Total: 2 locations**

---

## Implementation Checklist

### Phase 1: Invocation Logging
- [ ] `src/modes/auto.rs:153` - Add warning log on failure
- [ ] `src/modes/auto.rs:181` - Add warning log on failure
- [ ] `src/modes/got.rs:738` - Add warning log on failure
- [ ] `src/modes/got.rs:784` - Add warning log on failure
- [ ] `src/modes/got.rs:840` - Add warning log on failure
- [ ] `src/modes/got.rs:858` - Add warning log on failure
- [ ] `src/modes/got.rs:933` - Add warning log on failure
- [ ] `src/modes/got.rs:970` - Add warning log on failure
- [ ] `src/modes/got.rs:1029` - Add warning log on failure
- [ ] `src/modes/got.rs:1061` - Add warning log on failure

### Phase 2: Preset Registration
- [ ] `src/presets/registry.rs:81-94` - Refactor to loop with error logging

### Phase 3: .env Loading
- [ ] `src/config/mod.rs:152` - Add discriminated error handling

### Phase 4: Decision Storage
- [ ] `src/modes/decision.rs:499-505` - Propagate create_decision error
- [ ] `src/modes/decision.rs:634-637` - Propagate create_perspective error

### Phase 5: Evidence Storage
- [ ] `src/modes/evidence.rs:610-616` - Propagate create_evidence_assessment error
- [ ] `src/modes/evidence.rs:771-776` - Propagate create_probability_update error

---

## Testing Strategy

### Unit Tests
No new unit tests required - these are error handling improvements.

### Integration Tests
Verify that:
1. Storage failures in decision/evidence modes return errors
2. Invocation logging failures don't break primary operations

### Manual Testing
1. Simulate storage failure (e.g., readonly database)
2. Verify decision/evidence operations fail appropriately
3. Verify invocation logging failures produce warnings

---

## Rollback Plan

All changes are backward-compatible in terms of API. If storage error propagation causes issues:
1. Revert Phase 4 and 5 changes
2. Consider adding `soft_persist: bool` parameter for optional persistence

---

## Summary

| Phase | Files | Locations | Strategy | Risk |
|-------|-------|-----------|----------|------|
| 1 | 2 | 10 | Log + continue | Low |
| 2 | 1 | 5→1 | Log errors | Low |
| 3 | 1 | 1 | Discriminate errors | Low |
| 4 | 1 | 2 | Propagate errors | Medium |
| 5 | 1 | 2 | Propagate errors | Medium |

**Total: 6 files, 20 locations to modify**

The medium-risk items (Phases 4 & 5) change observable behavior - operations that previously "succeeded" will now fail when storage fails. This is the correct semantic but may require downstream handling.
