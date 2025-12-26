# Implementation Plan: Invocation Logging Silent Serialization Fix

## Problem Statement

**Location**: 17 occurrences across 6 files

| File | Lines | Context |
|------|-------|---------|
| `src/modes/auto.rs` | 137, 177 | `Invocation::new()` input, `.success()` output |
| `src/modes/divergent.rs` | 172, 270 | `Invocation::new()` input, `.success()` output |
| `src/modes/got.rs` | 740, 795, 842, 869, 935, 981, 1031, 1072 | Multiple operations |
| `src/modes/linear.rs` | 117, 146 | `Invocation::new()` input, `.success()` output |
| `src/modes/reflection.rs` | 221 | `.success()` output |
| `src/modes/tree.rs` | 211, 286 | `Invocation::new()` input, `.success()` output |

**Current Code Pattern**:
```rust
let mut invocation = Invocation::new(
    "reasoning.xxx",
    serde_json::to_value(&params).unwrap_or_default(),
)
```

And for success logging:
```rust
invocation = invocation.success(
    serde_json::to_value(&response).unwrap_or_default(),
    latency,
);
```

**Problem**: If `params` or `response` cannot be serialized to JSON:
- `unwrap_or_default()` returns `serde_json::Value::Null`
- No warning or error is logged
- Invocation log loses all diagnostic information about inputs/outputs
- Makes debugging production issues difficult

---

## Solution Design

### Approach: Add Helper Function with Logging

Create a helper function that wraps serialization with warning logging on failure. This approach:
1. Centralizes the fallback logic in one place
2. Logs the serialization error with context
3. Returns a descriptive fallback value instead of empty `Null`
4. Requires minimal changes at call sites

**Helper Function**:
```rust
/// Serialize a value to JSON for logging, with warning on failure.
fn serialize_for_log<T: serde::Serialize>(
    value: &T,
    context: &str,
) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|e| {
        warn!(
            error = %e,
            context = %context,
            "Failed to serialize value for invocation log"
        );
        serde_json::json!({
            "serialization_error": e.to_string(),
            "context": context
        })
    })
}
```

### Why This Approach?

1. **Single Source of Truth**: All serialization fallback logic in one place
2. **Diagnostic Visibility**: Errors are logged with context
3. **Informative Fallback**: Logged value indicates serialization failed (vs just `null`)
4. **Minimal Caller Changes**: Simple find-and-replace at call sites
5. **Consistent with Codebase**: Uses existing `tracing::warn!` pattern

---

## Implementation Steps

### Step 1: Add Helper Function to Each Mode File

Each mode file already imports `warn` from tracing. Add the helper function to each file that needs it.

**Files needing the helper**:
- `src/modes/auto.rs`
- `src/modes/divergent.rs`
- `src/modes/got.rs`
- `src/modes/linear.rs`
- `src/modes/reflection.rs`
- `src/modes/tree.rs`

**Alternative**: Create a shared utility module. However, since each mode file is self-contained, adding the helper locally is cleaner.

### Step 2: Update Call Sites

**Pattern Change**:
```rust
// Before
serde_json::to_value(&params).unwrap_or_default()

// After
serialize_for_log(&params, "reasoning.linear input")
```

**For success() calls**:
```rust
// Before
invocation = invocation.success(
    serde_json::to_value(&response).unwrap_or_default(),
    latency,
);

// After
invocation = invocation.success(
    serialize_for_log(&response, "reasoning.linear output"),
    latency,
);
```

---

## Detailed Changes by File

### 1. `src/modes/linear.rs`

**Add helper after imports**:
```rust
/// Serialize a value to JSON for logging, with warning on failure.
fn serialize_for_log<T: serde::Serialize>(value: &T, context: &str) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|e| {
        warn!(
            error = %e,
            context = %context,
            "Failed to serialize value for invocation log"
        );
        serde_json::json!({
            "serialization_error": e.to_string(),
            "context": context
        })
    })
}
```

**Line 117**: Change input serialization
```rust
// Before
serde_json::to_value(&params).unwrap_or_default(),

// After
serialize_for_log(&params, "reasoning.linear input"),
```

**Line 146**: Change output serialization
```rust
// Before
serde_json::to_value(&reasoning).unwrap_or_default(),

// After
serialize_for_log(&reasoning, "reasoning.linear output"),
```

### 2. `src/modes/auto.rs`

**Add helper after imports**

**Line 137**: `serialize_for_log(&params, "reasoning.auto input")`

**Line 177**: `serialize_for_log(&auto_response, "reasoning.auto output")`

### 3. `src/modes/divergent.rs`

**Add helper after imports**

**Line 172**: `serialize_for_log(&params, "reasoning.divergent input")`

**Line 270**: `serialize_for_log(&divergent_response, "reasoning.divergent output")`

### 4. `src/modes/tree.rs`

**Add helper after imports**

**Line 211**: `serialize_for_log(&params, "reasoning.tree input")`

**Line 286**: `serialize_for_log(&tree_response, "reasoning.tree output")`

### 5. `src/modes/reflection.rs`

**Add helper after imports**

**Line 221**: `serialize_for_log(&reflection, "reasoning.reflection output")`

### 6. `src/modes/got.rs`

**Add helper after imports**

| Line | Context String |
|------|---------------|
| 740 | `"reasoning.got.init input"` |
| 795 | `"reasoning.got.generate output"` |
| 842 | `"reasoning.got.score input"` |
| 869 | `"reasoning.got.score output"` |
| 935 | `"reasoning.got.aggregate input"` |
| 981 | `"reasoning.got.aggregate output"` |
| 1031 | `"reasoning.got.refine input"` |
| 1072 | `"reasoning.got.refine output"` |

---

## Verification

### Tests
- Existing tests should pass (no API changes)
- Helper function only logs on serialization failure (edge case)

### Manual Verification
1. Run `cargo test` - all tests should pass
2. Run `cargo clippy -- -D warnings` - no warnings
3. Optionally create a non-serializable struct to test warning output

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing code | Very Low | None | Same return types, same behavior for valid serialization |
| Performance impact | Negligible | None | Only triggered on error path |
| Log spam | Very Low | Low | Only logs when serialization fails (rare for Serialize types) |

---

## Success Criteria

- [ ] Helper function `serialize_for_log` added to all 6 mode files
- [ ] All 17 occurrences updated to use the helper
- [ ] Context strings identify the specific operation and direction (input/output)
- [ ] All existing tests pass
- [ ] No clippy warnings
- [ ] Code compiles without errors

---

## Alternative Considered: Centralized Module

Could create `src/utils/logging.rs` with the helper function exported for all modes. This was rejected because:
1. Each mode file is currently self-contained
2. Adding a new module for one function is over-engineering
3. The helper is simple and duplication is acceptable (6 copies)

If more shared utilities emerge, consider refactoring to a utility module.
