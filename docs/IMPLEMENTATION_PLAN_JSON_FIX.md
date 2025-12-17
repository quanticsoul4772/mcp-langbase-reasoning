# Implementation Plan: JSON Serialization Silent Failure Fix

## Problem Statement

**Location**: `src/storage/sqlite.rs` (13 occurrences)
**Lines**: 106, 145, 184, 272, 276, 305, 380, 481, 563, 671, 715, 811, 897

**Current Code Pattern**:
```rust
.map(|m| serde_json::to_string(m).unwrap_or_default());
```

**Problem**: If JSON serialization fails, an empty string is stored in the database instead of propagating the error. This causes:
- Data loss (metadata silently discarded)
- Corrupted database state
- Hard-to-debug issues when data is retrieved later

---

## Solution Design

### Approach 1: Add Serialization Error Variant to StorageError (Recommended)

Add a new error variant to `StorageError` specifically for serialization failures, then use `transpose()` to convert the nested `Result` into a propagatable error.

**Error Type Addition** (`src/error/mod.rs`):
```rust
/// Storage layer errors for database operations.
#[derive(Debug, Error)]
pub enum StorageError {
    // ... existing variants ...

    /// JSON serialization failed.
    #[error("Serialization failed: {message}")]
    Serialization {
        /// Description of the serialization issue.
        message: String,
    },
}
```

**Code Pattern Change**:
```rust
// Before (silent failure)
let metadata = session
    .metadata
    .as_ref()
    .map(|m| serde_json::to_string(m).unwrap_or_default());

// After (propagates error)
let metadata = session
    .metadata
    .as_ref()
    .map(|m| serde_json::to_string(m))
    .transpose()
    .map_err(|e| StorageError::Serialization {
        message: format!("Failed to serialize metadata: {}", e),
    })?;
```

### Why This Approach?

1. **Type Safety**: The error is captured in the type system
2. **Debuggability**: Clear error messages identify which field failed
3. **Consistency**: All serialization errors handled uniformly
4. **Minimal API Change**: Only adds a new error variant, existing code continues to work

---

## Implementation Steps

### Step 1: Add StorageError::Serialization Variant

**File**: `src/error/mod.rs`

Add after line 78 (before the closing brace of `StorageError` enum):

```rust
/// JSON serialization failed.
#[error("Serialization failed: {message}")]
Serialization {
    /// Description of the serialization issue.
    message: String,
},
```

### Step 2: Create Helper Function (Optional but Recommended)

**File**: `src/storage/sqlite.rs`

Add a private helper function to reduce boilerplate:

```rust
/// Serialize optional metadata to JSON string, propagating errors.
fn serialize_metadata<T: serde::Serialize>(
    metadata: &Option<T>,
    field_name: &str,
) -> StorageResult<Option<String>> {
    metadata
        .as_ref()
        .map(|m| serde_json::to_string(m))
        .transpose()
        .map_err(|e| StorageError::Serialization {
            message: format!("Failed to serialize {}: {}", field_name, e),
        })
}
```

### Step 3: Update All 13 Occurrences

| Line | Function | Field | Change |
|------|----------|-------|--------|
| 106 | `create_session` | `session.metadata` | Use helper |
| 145 | `update_session` | `session.metadata` | Use helper |
| 184 | `create_thought` | `thought.metadata` | Use helper |
| 272 | `log_invocation` | `invocation.input` | Direct conversion (required field) |
| 276 | `log_invocation` | `invocation.output` | Use helper |
| 305 | `create_branch` | `branch.metadata` | Use helper |
| 380 | `update_branch` | `branch.metadata` | Use helper |
| 481 | `create_checkpoint` | `checkpoint.snapshot` | Direct conversion (required field) |
| 563 | `create_graph_node` | `node.metadata` | Use helper |
| 671 | `update_graph_node` | `node.metadata` | Use helper |
| 715 | `create_graph_edge` | `edge.metadata` | Use helper |
| 811 | `create_snapshot` | `snapshot.state_data` | Direct conversion (required field) |
| 897 | `create_detection` | `detection.metadata` | Use helper |

### Step 4: Add Unit Tests

**File**: `src/error/mod.rs` (in test module)

```rust
#[test]
fn test_serialization_error_display() {
    let err = StorageError::Serialization {
        message: "invalid utf-8".to_string(),
    };
    assert_eq!(err.to_string(), "Serialization failed: invalid utf-8");
}
```

**File**: `src/storage/sqlite.rs` (add test for helper if implemented)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_metadata_success() {
        let data: Option<serde_json::Value> = Some(serde_json::json!({"key": "value"}));
        let result = serialize_metadata(&data, "test");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(r#"{"key":"value"}"#.to_string()));
    }

    #[test]
    fn test_serialize_metadata_none() {
        let data: Option<serde_json::Value> = None;
        let result = serialize_metadata(&data, "test");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }
}
```

---

## Detailed Code Changes

### Change 1: error/mod.rs

```diff
 /// Storage layer errors for database operations.
 #[derive(Debug, Error)]
 pub enum StorageError {
     // ... existing variants ...

     /// Underlying SQLx error.
     #[error("SQLx error: {0}")]
     Sqlx(#[from] sqlx::Error),
+
+    /// JSON serialization failed.
+    #[error("Serialization failed: {message}")]
+    Serialization {
+        /// Description of the serialization issue.
+        message: String,
+    },
 }
```

### Change 2: storage/sqlite.rs - Add Helper Function

```diff
 use crate::error::{StorageError, StorageResult};

+/// Serialize optional data to JSON string, propagating errors.
+fn serialize_json<T: serde::Serialize>(
+    data: &Option<T>,
+    field_name: &str,
+) -> StorageResult<Option<String>> {
+    data.as_ref()
+        .map(|d| serde_json::to_string(d))
+        .transpose()
+        .map_err(|e| StorageError::Serialization {
+            message: format!("Failed to serialize {}: {}", field_name, e),
+        })
+}
+
+/// Serialize required data to JSON string, propagating errors.
+fn serialize_json_required<T: serde::Serialize>(
+    data: &T,
+    field_name: &str,
+) -> StorageResult<String> {
+    serde_json::to_string(data).map_err(|e| StorageError::Serialization {
+        message: format!("Failed to serialize {}: {}", field_name, e),
+    })
+}
+
 /// SQLite-backed storage implementation
```

### Change 3: Update create_session (line 106)

```diff
 async fn create_session(&self, session: &Session) -> StorageResult<()> {
-    let metadata = session
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&session.metadata, "session.metadata")?;
```

### Change 4: Update update_session (line 145)

```diff
 async fn update_session(&self, session: &Session) -> StorageResult<()> {
-    let metadata = session
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&session.metadata, "session.metadata")?;
```

### Change 5: Update create_thought (line 184)

```diff
 async fn create_thought(&self, thought: &Thought) -> StorageResult<()> {
-    let metadata = thought
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&thought.metadata, "thought.metadata")?;
```

### Change 6: Update log_invocation (lines 272, 276)

```diff
 async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()> {
-    let input = serde_json::to_string(&invocation.input).unwrap_or_default();
-    let output = invocation
-        .output
-        .as_ref()
-        .map(|o| serde_json::to_string(o).unwrap_or_default());
+    let input = serialize_json_required(&invocation.input, "invocation.input")?;
+    let output = serialize_json(&invocation.output, "invocation.output")?;
```

### Change 7: Update create_branch (line 305)

```diff
 async fn create_branch(&self, branch: &Branch) -> StorageResult<()> {
-    let metadata = branch
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&branch.metadata, "branch.metadata")?;
```

### Change 8: Update update_branch (line 380)

```diff
 async fn update_branch(&self, branch: &Branch) -> StorageResult<()> {
-    let metadata = branch
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&branch.metadata, "branch.metadata")?;
```

### Change 9: Update create_checkpoint (line 481)

```diff
 async fn create_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()> {
-    let snapshot = serde_json::to_string(&checkpoint.snapshot).unwrap_or_default();
+    let snapshot = serialize_json_required(&checkpoint.snapshot, "checkpoint.snapshot")?;
```

### Change 10: Update create_graph_node (line 563)

```diff
 async fn create_graph_node(&self, node: &GraphNode) -> StorageResult<()> {
-    let metadata = node
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&node.metadata, "graph_node.metadata")?;
```

### Change 11: Update update_graph_node (line 671)

```diff
 async fn update_graph_node(&self, node: &GraphNode) -> StorageResult<()> {
-    let metadata = node
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&node.metadata, "graph_node.metadata")?;
```

### Change 12: Update create_graph_edge (line 715)

```diff
 async fn create_graph_edge(&self, edge: &GraphEdge) -> StorageResult<()> {
-    let metadata = edge
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&edge.metadata, "graph_edge.metadata")?;
```

### Change 13: Update create_snapshot (line 811)

```diff
 async fn create_snapshot(&self, snapshot: &StateSnapshot) -> StorageResult<()> {
-    let state_data = serde_json::to_string(&snapshot.state_data).unwrap_or_default();
+    let state_data = serialize_json_required(&snapshot.state_data, "snapshot.state_data")?;
```

### Change 14: Update create_detection (line 897)

```diff
 async fn create_detection(&self, detection: &Detection) -> StorageResult<()> {
-    let metadata = detection
-        .metadata
-        .as_ref()
-        .map(|m| serde_json::to_string(m).unwrap_or_default());
+    let metadata = serialize_json(&detection.metadata, "detection.metadata")?;
```

---

## Testing Strategy

### Unit Tests
1. Test `StorageError::Serialization` error display
2. Test `serialize_json` helper with valid data
3. Test `serialize_json` helper with None
4. Test `serialize_json_required` helper with valid data

### Integration Tests
1. Verify existing tests still pass (no breaking changes)
2. Add test that intentionally fails serialization (if possible with custom type)

### Manual Verification
1. Run full test suite: `cargo test`
2. Verify clippy passes: `cargo clippy -- -D warnings`
3. Test with real database operations

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaking existing code | Low | Medium | All changes are in private implementation |
| Test failures | Low | Low | Fix any tests that relied on silent failures |
| Performance impact | Negligible | None | Same operations, just with error handling |

---

## Rollback Plan

If issues arise:
1. Revert the commit
2. All changes are localized to `src/error/mod.rs` and `src/storage/sqlite.rs`
3. No database schema changes required

---

## Success Criteria

- [ ] All 13 occurrences updated
- [ ] New `StorageError::Serialization` variant added
- [ ] Helper functions implemented
- [ ] All existing tests pass
- [ ] New tests for serialization error handling
- [ ] No clippy warnings
- [ ] Code compiles without errors
