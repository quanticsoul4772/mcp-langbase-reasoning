# Implementation Plan: Error Body Fallback Logging Fix

## Problem Statement

**Location**: `src/langbase/client.rs` (3 occurrences)
**Lines**: 128, 173, 214

**Current Code Pattern**:
```rust
let error_body = response.text().await.unwrap_or_default();
```

**Problem**: If reading the error body fails, the actual error details are silently discarded, making API debugging difficult. The resulting empty string provides no indication that the error body couldn't be read.

---

## Solution Design

### Approach: Log Warning Before Fallback

Use `unwrap_or_else` with a closure that logs a warning before returning a descriptive fallback message.

**Code Pattern Change**:
```rust
// Before (silent failure)
let error_body = response.text().await.unwrap_or_default();

// After (logs warning with error details)
let error_body = response.text().await.unwrap_or_else(|e| {
    warn!(error = %e, "Failed to read API error response body");
    "Unable to read error response".to_string()
});
```

### Why This Approach?

1. **Diagnostic Visibility**: Errors reading the body are logged for debugging
2. **Informative Fallback**: Message indicates body couldn't be read (vs empty string)
3. **No API Changes**: Same return types, no interface changes
4. **Minimal Change**: Simple, focused fix at each location
5. **Consistent with Codebase**: Uses existing `tracing::warn!` pattern

---

## Implementation Steps

### Step 1: Update execute_request (line 128)

**Function**: `execute_request`
**Context**: API call to run a pipe

```diff
 if !status.is_success() {
-    let error_body = response.text().await.unwrap_or_default();
+    let error_body = response.text().await.unwrap_or_else(|e| {
+        warn!(error = %e, status = %status, "Failed to read pipe run error response body");
+        "Unable to read error response".to_string()
+    });
     return Err(LangbaseError::Api {
         status: status.as_u16(),
         message: error_body,
     });
 }
```

### Step 2: Update create_pipe (line 173)

**Function**: `create_pipe`
**Context**: API call to create a new pipe

```diff
 if !status.is_success() {
-    let error_body = response.text().await.unwrap_or_default();
+    let error_body = response.text().await.unwrap_or_else(|e| {
+        warn!(error = %e, status = %status, "Failed to read pipe creation error response body");
+        "Unable to read error response".to_string()
+    });
     return Err(LangbaseError::Api {
         status: status.as_u16(),
         message: error_body,
     });
 }
```

### Step 3: Update delete_pipe (line 214)

**Function**: `delete_pipe`
**Context**: API call to delete a pipe

```diff
 if !status.is_success() {
-    let error_body = response.text().await.unwrap_or_default();
+    let error_body = response.text().await.unwrap_or_else(|e| {
+        warn!(error = %e, status = %status, "Failed to read pipe deletion error response body");
+        "Unable to read error response".to_string()
+    });
     return Err(LangbaseError::Api {
         status: status.as_u16(),
         message: error_body,
     });
 }
```

---

## Verification

### Tests
- Existing tests should pass (no API changes)
- The warning log is only triggered on edge cases (body read failure)

### Manual Verification
1. Run `cargo test` - all tests should pass
2. Run `cargo clippy -- -D warnings` - no warnings
3. Test with actual API calls (error scenarios)

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing code | Very Low | None | Same return type, same error structure |
| Performance impact | Negligible | None | Only triggered on error paths |
| Log spam | Low | Low | Only logs when body read fails (rare) |

---

## Success Criteria

- [ ] All 3 occurrences updated
- [ ] All existing tests pass
- [ ] No clippy warnings
- [ ] Code compiles without errors
- [ ] Warning includes relevant context (error, status code)
