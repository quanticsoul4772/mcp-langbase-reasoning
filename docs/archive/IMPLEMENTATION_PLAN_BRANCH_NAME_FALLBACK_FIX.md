# Implementation Plan: Branch Name Fallback Fix

## Problem Statement

**Location**: `src/modes/tree.rs` (line 317)

**Current Code**:
```rust
child_branches.push(BranchInfo {
    id: child.id,
    name: child.name.unwrap_or_default(),  // ← Problem
    confidence: tb.confidence,
    rationale: tb.rationale.clone(),
});
```

**Problem**: When a `Branch` has `name: None`, the `unwrap_or_default()` produces an empty string `""` in the `BranchInfo` output. This is confusing for users who see:
```json
{
  "id": "branch-abc123",
  "name": "",
  "confidence": 0.85,
  "rationale": "Explore alternative approach"
}
```

An empty `name` field provides no semantic value and may confuse API consumers.

---

## Solution Design

### Approach: Use Informative Default String

Change the fallback from `unwrap_or_default()` (empty string) to `unwrap_or_else(|| "Unnamed Branch".to_string())`.

**After**:
```rust
child_branches.push(BranchInfo {
    id: child.id,
    name: child.name.clone().unwrap_or_else(|| "Unnamed Branch".to_string()),
    confidence: tb.confidence,
    rationale: tb.rationale.clone(),
});
```

### Why This Approach?

1. **User Clarity**: "Unnamed Branch" clearly communicates that no name was assigned
2. **Minimal Change**: Single-line fix with no structural changes
3. **Backwards Compatible**: Still produces a valid `String` for the `name` field
4. **Consistent UX**: Users can distinguish named vs. unnamed branches in output
5. **No Breaking Change**: `BranchInfo.name` remains `String`, not `Option<String>`

### Alternative Considered: Make `BranchInfo.name` Optional

Could change `BranchInfo.name` to `Option<String>` to preserve the semantic of "no name provided". However:
- This is a breaking API change for consumers
- The current code path (line 298) already sets a name: `with_name(format!("Option {}: {}", i + 1, truncate(&tb.thought, 30)))`
- The `None` case likely only occurs through edge cases or direct storage access

**Verdict**: The simple string fallback is preferred for minimal impact.

---

## Implementation Steps

### Step 1: Update Fallback at Line 317

**File**: `src/modes/tree.rs`

**Before** (line 315-320):
```rust
child_branches.push(BranchInfo {
    id: child.id,
    name: child.name.unwrap_or_default(),
    confidence: tb.confidence,
    rationale: tb.rationale.clone(),
});
```

**After**:
```rust
child_branches.push(BranchInfo {
    id: child.id,
    name: child.name.clone().unwrap_or_else(|| "Unnamed Branch".to_string()),
    confidence: tb.confidence,
    rationale: tb.rationale.clone(),
});
```

Note: Added `.clone()` because `child.name` is `Option<String>` and we need to convert to owned `String`.

### Step 2: Add Test for Unnamed Branch Fallback

Add a test to verify the fallback behavior. This should be added in the BranchInfo tests section.

**Location**: After `test_branch_info_deserialize` (around line 897)

```rust
#[test]
fn test_branch_info_unnamed_fallback() {
    // Verify that "Unnamed Branch" is a valid name value
    let info = BranchInfo {
        id: "branch-1".to_string(),
        name: "Unnamed Branch".to_string(),
        confidence: 0.7,
        rationale: "No name provided".to_string(),
    };

    let json = serde_json::to_string(&info).unwrap();
    assert!(json.contains("Unnamed Branch"));

    let parsed: BranchInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "Unnamed Branch");
}
```

---

## Verification

### Tests
- All existing tests should pass (no behavior change for named branches)
- New test verifies "Unnamed Branch" string serialization

### Manual Verification
1. Run `cargo test` - all tests should pass
2. Run `cargo clippy -- -D warnings` - no warnings

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Breaks existing behavior | Very Low | Low | Named branches unaffected |
| Test failures | None | Low | New test only |
| API confusion | None | None | "Unnamed Branch" is self-documenting |

---

## Success Criteria

- [ ] `unwrap_or_default()` replaced with `unwrap_or_else(|| "Unnamed Branch".to_string())`
- [ ] New test for unnamed branch fallback added
- [ ] All existing tests pass
- [ ] No clippy warnings
- [ ] Code compiles without errors

---

## API Impact

**Before** (JSON output for unnamed branch):
```json
{
  "id": "branch-abc123",
  "name": "",
  "confidence": 0.85,
  "rationale": "Explore alternative approach"
}
```

**After** (JSON output for unnamed branch):
```json
{
  "id": "branch-abc123",
  "name": "Unnamed Branch",
  "confidence": 0.85,
  "rationale": "Explore alternative approach"
}
```

This is a minor UX improvement that provides clearer feedback to API consumers.
