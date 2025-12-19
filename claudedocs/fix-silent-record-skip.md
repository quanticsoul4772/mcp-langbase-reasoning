# Design: Fix Silent Record Skipping

## Problem Statement

In `src/storage/sqlite.rs`, the `filter_map` + `.ok()?` pattern silently discards database records that fail to parse. No logging occurs when records are skipped, leading to data loss without any notification.

**Affected locations:**

1. **get_all_pipe_summaries** (lines 359-396): Skips PipeUsageSummary records on timestamp parse failure
2. **get_pipe_summary** (lines 434-459): Returns None on timestamp parse failure
3. **get_invocations** (lines 521-559): Skips Invocation records on JSON or timestamp parse failures

```rust
// Current problematic pattern
.filter_map(|row| {
    // ...
    let input: serde_json::Value = serde_json::from_str(&input_str).ok()?;  // Silent skip
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .ok()?  // Silent skip
        .with_timezone(&Utc);
    // ...
})
```

## Design Goals

1. **Log all skipped records** - Every skipped record should produce a warning
2. **Track skip metrics** - Enable monitoring of data quality issues
3. **Preserve existing behavior** - Still skip corrupted records (don't break queries)
4. **Minimal code changes** - Use helper functions to avoid repetition
5. **Consistent with existing patterns** - Similar to timestamp reconstruction tracking

## Solution: Add Record Skip Counter + Logging Helper

### Approach

Create a helper macro/function that:
1. Logs a warning with context when a parse fails
2. Increments a global counter for metrics
3. Returns None to trigger the filter_map skip

### Implementation Plan

#### Step 1: Add Record Skip Counter

```rust
// In src/storage/sqlite.rs (near timestamp counter)
static RECORD_SKIP_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn get_record_skip_count() -> u64 {
    RECORD_SKIP_COUNT.load(Ordering::Relaxed)
}

pub fn reset_record_skip_count() -> u64 {
    RECORD_SKIP_COUNT.swap(0, Ordering::Relaxed)
}
```

#### Step 2: Create Logging Helper Functions

```rust
/// Parse JSON with logging on failure. Returns None and logs warning if parse fails.
fn parse_json_or_skip<T: serde::de::DeserializeOwned>(
    json_str: &str,
    record_id: &str,
    field_name: &str,
) -> Option<T> {
    match serde_json::from_str(json_str) {
        Ok(value) => Some(value),
        Err(e) => {
            RECORD_SKIP_COUNT.fetch_add(1, Ordering::Relaxed);
            warn!(
                error = %e,
                record_id = record_id,
                field = field_name,
                "RECORD SKIPPED: Failed to parse JSON field"
            );
            None
        }
    }
}

/// Parse timestamp with logging on failure. Returns None and logs warning if parse fails.
fn parse_timestamp_or_skip(
    ts_str: &str,
    record_id: &str,
    field_name: &str,
) -> Option<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(ts_str) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(e) => {
            RECORD_SKIP_COUNT.fetch_add(1, Ordering::Relaxed);
            warn!(
                error = %e,
                timestamp = ts_str,
                record_id = record_id,
                field = field_name,
                "RECORD SKIPPED: Failed to parse timestamp"
            );
            None
        }
    }
}
```

#### Step 3: Update get_invocations (lines 521-559)

```rust
let invocations = rows
    .into_iter()
    .filter_map(|row| {
        let id: String = row.get("id");
        let session_id: Option<String> = row.get("session_id");
        let tool_name: String = row.get("tool_name");
        let input_str: String = row.get("input");
        let output_str: Option<String> = row.get("output");
        let pipe_name: Option<String> = row.get("pipe_name");
        let latency_ms: Option<i64> = row.get("latency_ms");
        let success: bool = row.get("success");
        let error: Option<String> = row.get("error");
        let created_at_str: String = row.get("created_at");

        // Parse with logging on failure
        let input: serde_json::Value = parse_json_or_skip(&input_str, &id, "input")?;
        let output: Option<serde_json::Value> = output_str
            .and_then(|s| parse_json_or_skip(&s, &id, "output"));
        let created_at = parse_timestamp_or_skip(&created_at_str, &id, "created_at")?;

        // ... rest unchanged
    })
    .collect();
```

#### Step 4: Update get_all_pipe_summaries (lines 359-396)

```rust
let summaries = rows
    .into_iter()
    .filter_map(|row| {
        let pipe_name: String = row.get("pipe_name");
        // ... other fields ...

        let first_call_dt = parse_timestamp_or_skip(&first_call, &pipe_name, "first_call")?;
        let last_call_dt = parse_timestamp_or_skip(&last_call, &pipe_name, "last_call")?;

        // ... rest unchanged
    })
    .collect();
```

#### Step 5: Update get_pipe_summary (lines 434-459)

```rust
let summary = row.and_then(|row| {
    let pipe_name: String = row.get("pipe_name");
    // ... other fields ...

    let first_call_dt = parse_timestamp_or_skip(&first_call, &pipe_name, "first_call")?;
    let last_call_dt = parse_timestamp_or_skip(&last_call, &pipe_name, "last_call")?;

    // ... rest unchanged
});
```

#### Step 6: Add to FallbackMetricsSummary

```rust
pub struct FallbackMetricsSummary {
    // ... existing fields ...
    pub timestamp_reconstructions: u64,
    /// Number of database records skipped due to parse failures.
    pub records_skipped: u64,
}
```

#### Step 7: Export from storage mod

```rust
pub use sqlite::{
    get_timestamp_reconstruction_count,
    reset_timestamp_reconstruction_count,
    get_record_skip_count,
    reset_record_skip_count,
    SqliteStorage,
};
```

### Files to Modify

| File | Changes |
|------|---------|
| `src/storage/sqlite.rs` | Add counter, add helper functions, update 3 query methods |
| `src/storage/mod.rs` | Export new functions, add field to FallbackMetricsSummary |

### Test Cases

```rust
#[test]
fn test_record_skip_count_increments_on_json_parse_failure() {
    reset_record_skip_count();
    let result: Option<serde_json::Value> = parse_json_or_skip("invalid json", "test-id", "field");
    assert!(result.is_none());
    assert_eq!(get_record_skip_count(), 1);
}

#[test]
fn test_record_skip_count_increments_on_timestamp_parse_failure() {
    reset_record_skip_count();
    let result = parse_timestamp_or_skip("not-a-timestamp", "test-id", "created_at");
    assert!(result.is_none());
    assert_eq!(get_record_skip_count(), 1);
}

#[test]
fn test_valid_json_does_not_increment_skip_count() {
    reset_record_skip_count();
    let result: Option<serde_json::Value> = parse_json_or_skip(r#"{"key": "value"}"#, "test-id", "field");
    assert!(result.is_some());
    assert_eq!(get_record_skip_count(), 0);
}
```

## Effort Estimate

- Implementation: ~25 minutes
- Testing: ~10 minutes
- Total: ~35 minutes

## Verification Checklist

- [ ] Record skip counter added
- [ ] parse_json_or_skip helper function added
- [ ] parse_timestamp_or_skip helper function added
- [ ] get_invocations updated to use helpers
- [ ] get_all_pipe_summaries updated to use helpers
- [ ] get_pipe_summary updated to use helpers
- [ ] records_skipped added to FallbackMetricsSummary
- [ ] Functions exported from storage mod
- [ ] Unit tests pass
- [ ] cargo clippy passes

## Log Output Example

When a record is skipped, the log will show:
```
WARN RECORD SKIPPED: Failed to parse JSON field
    error: expected value at line 1 column 1
    record_id: inv-12345
    field: input
```

This makes it easy to:
1. Search logs for "RECORD SKIPPED" to find all issues
2. Identify which records have problems
3. Monitor the skip count via metrics endpoint
