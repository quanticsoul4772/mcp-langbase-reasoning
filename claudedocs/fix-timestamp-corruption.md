# Design: Fix Timestamp Corruption

## Problem Statement

When parsing timestamps from SQLite in `src/storage/sqlite.rs:1765-1779`, invalid timestamps are silently replaced with `chrono::Utc::now()`. This corrupts data integrity and creates misleading audit trails.

```rust
// Current problematic code
fn parse_timestamp_with_logging(ts_str: &str, context: &str) -> chrono::DateTime<chrono::Utc> {
    match DateTime::parse_from_rfc3339(ts_str) {
        Ok(dt) => dt.with_timezone(&chrono::Utc),
        Err(e) => {
            warn!(...);
            chrono::Utc::now()  // <-- Data corruption!
        }
    }
}
```

## Current Usage

The function is called 16+ times across various row conversions:
- SessionRow → Session (created_at, updated_at)
- ThoughtRow → Thought (created_at)
- BranchRow → Branch (created_at, updated_at)
- CheckpointRow → Checkpoint (created_at)
- GraphNodeRow → GraphNode (created_at)
- GraphEdgeRow → GraphEdge (created_at)
- DetectionRow → Detection (created_at)
- DecisionRow → Decision (created_at)
- PerspectiveRow → PerspectiveAnalysis (created_at)
- EvidenceAssessmentRow → EvidenceAssessment (created_at)
- ProbabilisticUpdateRow → ProbabilisticUpdate (created_at)

## Design Goals

1. **Preserve data integrity** - Never silently replace timestamps
2. **Maintain backward compatibility** - Existing API responses should work
3. **Enable detection** - Callers should know if timestamp was reconstructed
4. **Track metrics** - Integrate with fallback metrics system
5. **Minimize API surface changes** - Avoid changing all 12+ struct definitions

## Solution Options

### Option A: Return Result<DateTime, Error>

Change the parsing function to return a Result and propagate errors.

```rust
fn parse_timestamp(ts_str: &str) -> Result<DateTime<Utc>, StorageError> {
    DateTime::parse_from_rfc3339(ts_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| StorageError::TimestampParse {
            value: ts_str.to_string(),
            error: e.to_string()
        })
}
```

**Pros:**
- Clean, Rust-idiomatic approach
- Failures are explicit
- No data corruption possible

**Cons:**
- Breaks all From<Row> implementations
- One bad timestamp breaks entire query result
- Aggressive - may break production on legacy data

### Option B: Add timestamp_reconstructed Flag (Recommended)

Add a flag to track when timestamps were reconstructed, similar to the auto-mode fallback fix.

```rust
/// Wrapper for timestamps that may have been reconstructed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedTimestamp {
    /// The timestamp value (may be reconstructed)
    pub value: DateTime<Utc>,
    /// True if this timestamp was reconstructed due to parse failure
    #[serde(default)]
    pub reconstructed: bool,
    /// Original invalid value if reconstructed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_invalid: Option<String>,
}
```

**Pros:**
- Transparent to consumers
- Preserves data flow
- Similar pattern to auto-mode fix
- Backward compatible

**Cons:**
- Changes type of all timestamp fields
- More invasive changes to structs

### Option C: Return Option<DateTime> with Logging

Return None for invalid timestamps and let callers decide.

```rust
fn parse_timestamp_optional(ts_str: &str, context: &str) -> Option<DateTime<Utc>> {
    match DateTime::parse_from_rfc3339(ts_str) {
        Ok(dt) => Some(dt.with_timezone(&Utc)),
        Err(e) => {
            warn!(...);
            None
        }
    }
}
```

**Pros:**
- Simple change
- Explicit handling

**Cons:**
- Requires all structs to use Option<DateTime>
- Breaking API change

### Option D: Hybrid - Result with Fallback and Metrics (Recommended)

Keep the current return type but:
1. Track reconstruction in metrics
2. Return a tuple with reconstruction flag
3. Use a wrapper function for backward compatibility

```rust
/// Parse timestamp, returning both the value and whether it was reconstructed
fn parse_timestamp_tracked(ts_str: &str, context: &str) -> (DateTime<Utc>, bool) {
    match DateTime::parse_from_rfc3339(ts_str) {
        Ok(dt) => (dt.with_timezone(&Utc), false),
        Err(e) => {
            warn!(
                error = %e,
                timestamp = ts_str,
                context = context,
                "Failed to parse timestamp, using current time as fallback"
            );
            // Track in metrics
            TIMESTAMP_RECONSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed);
            (Utc::now(), true)
        }
    }
}

// For rows where we want to track reconstruction
struct SessionRow {
    // ... existing fields ...
}

impl From<SessionRow> for Session {
    fn from(row: SessionRow) -> Self {
        let (created_at, created_at_reconstructed) = parse_timestamp_tracked(...);
        let (updated_at, updated_at_reconstructed) = parse_timestamp_tracked(...);

        // Log if any timestamps were reconstructed
        if created_at_reconstructed || updated_at_reconstructed {
            warn!(
                session_id = %row.id,
                created_at_reconstructed,
                updated_at_reconstructed,
                "Session loaded with reconstructed timestamps"
            );
        }

        Self { ... }
    }
}
```

## Recommended Solution: Option D (Hybrid)

Implement Option D with the following approach:

### Phase 1: Track and Log (Minimal Changes)

1. Add atomic counter for timestamp reconstructions
2. Update `parse_timestamp_with_logging` to return tuple
3. Log warnings when reconstructed timestamps are used
4. Add metrics endpoint to expose reconstruction count

### Phase 2: Add Struct Fields (Optional Future Enhancement)

1. Add optional `timestamps_reconstructed: bool` to high-value structs (Session, Thought)
2. Serialize only when true (skip_serializing_if)

## Implementation Plan

### Step 1: Add Metrics Counter

```rust
// In src/storage/sqlite.rs or src/storage/mod.rs
use std::sync::atomic::{AtomicU64, Ordering};

static TIMESTAMP_RECONSTRUCTION_COUNT: AtomicU64 = AtomicU64::new(0);

pub fn get_timestamp_reconstruction_count() -> u64 {
    TIMESTAMP_RECONSTRUCTION_COUNT.load(Ordering::Relaxed)
}

pub fn reset_timestamp_reconstruction_count() -> u64 {
    TIMESTAMP_RECONSTRUCTION_COUNT.swap(0, Ordering::Relaxed)
}
```

### Step 2: Update Parsing Function

```rust
/// Parse timestamp with reconstruction tracking
fn parse_timestamp_with_logging(ts_str: &str, context: &str) -> DateTime<Utc> {
    match DateTime::parse_from_rfc3339(ts_str) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(e) => {
            // Increment reconstruction counter
            TIMESTAMP_RECONSTRUCTION_COUNT.fetch_add(1, Ordering::Relaxed);

            warn!(
                error = %e,
                timestamp = ts_str,
                context = context,
                total_reconstructions = TIMESTAMP_RECONSTRUCTION_COUNT.load(Ordering::Relaxed),
                "TIMESTAMP CORRUPTION: Failed to parse timestamp, using current time"
            );
            Utc::now()
        }
    }
}
```

### Step 3: Add Metrics Endpoint

Add to `reasoning_metrics_summary` or create new `reasoning_timestamp_health`:

```rust
async fn handle_timestamp_health(state: &SharedState) -> McpResult<Value> {
    let reconstruction_count = get_timestamp_reconstruction_count();

    Ok(json!({
        "timestamp_reconstructions": reconstruction_count,
        "status": if reconstruction_count == 0 { "healthy" } else { "degraded" },
        "recommendation": if reconstruction_count > 0 {
            "Database contains corrupted timestamps. Consider data migration."
        } else {
            "All timestamps are valid."
        }
    }))
}
```

### Step 4: Add to Fallback Metrics

Integrate with existing `FallbackMetricsSummary`:

```rust
pub struct FallbackMetricsSummary {
    // ... existing fields ...

    /// Number of timestamps that were reconstructed due to parse failures
    pub timestamp_reconstructions: u64,
}
```

### Files to Modify

| File | Changes |
|------|---------|
| `src/storage/sqlite.rs` | Add atomic counter, update parsing function |
| `src/storage/mod.rs` | Export metrics functions |
| `src/server/handlers.rs` | Add to debug/metrics output |

### No Changes Required

- All struct definitions (Session, Thought, etc.) remain unchanged
- All From<Row> implementations keep same signature
- API responses unchanged

## Effort Estimate

- Implementation: ~20 minutes
- Testing: ~10 minutes
- Total: ~30 minutes

## Verification Checklist

- [ ] Atomic counter added for reconstructions
- [ ] Parse function logs with TIMESTAMP CORRUPTION prefix
- [ ] Reconstruction count exposed in metrics
- [ ] Existing tests pass
- [ ] cargo clippy passes

## Future Considerations

1. **Data Migration**: Add CLI command to scan and report corrupted timestamps
2. **Strict Mode**: In STRICT_MODE, return error instead of reconstructing
3. **Struct Fields**: Add `timestamps_valid: bool` to high-value structs if needed
4. **Alerting**: Add threshold-based alerting for reconstruction count

## Test Cases

```rust
#[test]
fn test_timestamp_reconstruction_increments_counter() {
    reset_timestamp_reconstruction_count();
    let _ = parse_timestamp_with_logging("invalid", "test");
    assert_eq!(get_timestamp_reconstruction_count(), 1);
}

#[test]
fn test_valid_timestamp_no_reconstruction() {
    reset_timestamp_reconstruction_count();
    let _ = parse_timestamp_with_logging("2024-01-01T00:00:00Z", "test");
    assert_eq!(get_timestamp_reconstruction_count(), 0);
}
```
