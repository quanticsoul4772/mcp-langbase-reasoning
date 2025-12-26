# Implementation Plan: Auto Mode Parsing Fallback Logging Fix

## Problem Statement

**Location**: `src/modes/auto.rs` (lines 161-164)

**Current Code**:
```rust
let recommended_mode = auto_response
    .recommended_mode
    .parse()
    .unwrap_or(ReasoningMode::Linear);
```

**Problem**: If the AI returns an invalid mode string (e.g., "LINIEAR", "unknown", ""), it silently falls back to Linear mode without any indication. This makes debugging difficult:
- User doesn't know their intended mode was rejected
- No visibility into what invalid value was returned
- Silent behavior change with no audit trail

---

## Solution Design

### Approach: Add Warning Log on Parse Failure

Use `unwrap_or_else` with a closure that logs the invalid mode string before falling back.

**Code Pattern Change**:
```rust
// Before (silent fallback)
let recommended_mode = auto_response
    .recommended_mode
    .parse()
    .unwrap_or(ReasoningMode::Linear);

// After (logs warning with context)
let recommended_mode = auto_response
    .recommended_mode
    .parse()
    .unwrap_or_else(|_| {
        warn!(
            invalid_mode = %auto_response.recommended_mode,
            "Invalid mode returned by auto-router, falling back to Linear"
        );
        ReasoningMode::Linear
    });
```

### Why This Approach?

1. **Diagnostic Visibility**: Invalid modes are logged for debugging
2. **Context Preserved**: Log includes the actual invalid value returned
3. **No API Changes**: Same return type, same fallback behavior
4. **Minimal Change**: Single location, focused fix
5. **Consistent with Codebase**: Uses existing `tracing::warn!` pattern (already imported)

---

## Implementation Steps

### Step 1: Update the parse() call (lines 161-164)

**File**: `src/modes/auto.rs`

**Before**:
```rust
// Convert mode string to enum
let recommended_mode = auto_response
    .recommended_mode
    .parse()
    .unwrap_or(ReasoningMode::Linear);
```

**After**:
```rust
// Convert mode string to enum
let recommended_mode = auto_response
    .recommended_mode
    .parse()
    .unwrap_or_else(|_| {
        warn!(
            invalid_mode = %auto_response.recommended_mode,
            "Invalid mode returned by auto-router, falling back to Linear"
        );
        ReasoningMode::Linear
    });
```

---

## Verification

### Tests
- Existing tests should pass (no API changes)
- The warning log is only triggered when parse fails
- Test `test_reasoning_mode_invalid_string` confirms parse errors occur for invalid strings

### Manual Verification
1. Run `cargo test` - all tests should pass
2. Run `cargo clippy -- -D warnings` - no warnings
3. Optionally test with invalid mode string to see warning in logs

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing code | Very Low | None | Same return type, same fallback value |
| Performance impact | Negligible | None | Only triggered on error path |
| Log spam | Very Low | Low | Only logs when AI returns invalid mode (rare) |

---

## Success Criteria

- [ ] `unwrap_or_else` replaces `unwrap_or`
- [ ] Warning includes the invalid mode string
- [ ] All existing tests pass
- [ ] No clippy warnings
- [ ] Code compiles without errors
