# Code Deduplication Plan: Session Management

## Problem Statement

The `get_or_create_session()` logic is duplicated across 8 mode files with identical implementation except for the mode name string:

| File | Line | Mode Name |
|------|------|-----------|
| `src/modes/linear.rs` | 85-104 | "linear" |
| `src/modes/tree.rs` | 367-384 | "tree" |
| `src/modes/divergent.rs` | 305-322 | "divergent" |
| `src/modes/reflection.rs` | 382-399 | "reflection" |
| `src/modes/decision.rs` | 700-717 | "decision" |
| `src/modes/evidence.rs` | 839-856 | "evidence" |
| `src/modes/got.rs` | 642-658 | "got" |
| `src/modes/auto.rs` | (inline) | "auto" |

### Impact
- ~20 lines duplicated × 8 files = ~160 lines of redundant code
- Changes to session creation logic require 8 synchronized updates
- Risk of inconsistency if one file is updated but others are not
- Violates DRY principle

---

## Solution Design

### Option A: Extension Trait on Storage (Recommended)

Add a `get_or_create_session` method to the `Storage` trait as a provided method with default implementation.

```rust
// In src/storage/mod.rs

#[async_trait]
pub trait Storage: Send + Sync {
    // ... existing methods ...

    // ========================================================================
    // Session convenience methods (provided implementations)
    // ========================================================================

    /// Get an existing session or create a new one.
    ///
    /// If `session_id` is `Some`, looks up the session:
    /// - If found, returns it
    /// - If not found, creates a new session with that ID
    ///
    /// If `session_id` is `None`, creates a new session with a generated ID.
    async fn get_or_create_session(
        &self,
        session_id: &Option<String>,
        mode: &str,
    ) -> StorageResult<Session>
    where
        Self: Sized,
    {
        match session_id {
            Some(id) => match self.get_session(id).await? {
                Some(session) => Ok(session),
                None => {
                    let mut new_session = Session::new(mode);
                    new_session.id = id.clone();
                    self.create_session(&new_session).await?;
                    Ok(new_session)
                }
            },
            None => {
                let session = Session::new(mode);
                self.create_session(&session).await?;
                Ok(session)
            }
        }
    }
}
```

### Why This Approach

1. **No new types needed**: Uses existing `Storage` trait and `Session` type
2. **Backward compatible**: Existing code continues to work
3. **Single source of truth**: Logic lives in one place
4. **Consistent behavior**: All modes use identical session handling
5. **Easy adoption**: Modes can migrate incrementally

---

## Implementation Plan

### Step 1: Add Extension Method to Storage Trait (~15 lines)

**File**: `src/storage/mod.rs`

Add the `get_or_create_session` method to the `Storage` trait as a provided method.

```rust
/// Get an existing session or create a new one.
async fn get_or_create_session(
    &self,
    session_id: &Option<String>,
    mode: &str,
) -> StorageResult<Session>
where
    Self: Sized,
{
    match session_id {
        Some(id) => match self.get_session(id).await? {
            Some(session) => Ok(session),
            None => {
                let mut new_session = Session::new(mode);
                new_session.id = id.clone();
                self.create_session(&new_session).await?;
                Ok(new_session)
            }
        },
        None => {
            let session = Session::new(mode);
            self.create_session(&session).await?;
            Ok(session)
        }
    }
}
```

### Step 2: Update Mode Files (8 files)

For each mode file, replace the local `get_or_create_session` method/inline code with a call to `self.storage.get_or_create_session()`.

#### Example Migration (tree.rs)

**Before**:
```rust
async fn get_or_create_session(&self, session_id: &Option<String>) -> AppResult<Session> {
    match session_id {
        Some(id) => match self.storage.get_session(id).await? {
            Some(s) => Ok(s),
            None => {
                let mut new_session = Session::new("tree");
                new_session.id = id.clone();
                self.storage.create_session(&new_session).await?;
                Ok(new_session)
            }
        },
        None => {
            let session = Session::new("tree");
            self.storage.create_session(&session).await?;
            Ok(session)
        }
    }
}
```

**After**:
```rust
// Delete the entire get_or_create_session method and replace calls with:
let session = self.storage.get_or_create_session(&params.session_id, "tree").await?;
```

### Step 3: Add Unit Tests

Add tests to `src/storage/mod.rs` or `tests/storage_tests.rs`:

```rust
#[tokio::test]
async fn test_get_or_create_session_new() {
    let storage = SqliteStorage::new_in_memory().await.unwrap();

    let session = storage.get_or_create_session(&None, "test").await.unwrap();

    assert_eq!(session.mode, "test");
    assert!(!session.id.is_empty());
}

#[tokio::test]
async fn test_get_or_create_session_existing() {
    let storage = SqliteStorage::new_in_memory().await.unwrap();

    // Create a session first
    let original = Session::new("test");
    storage.create_session(&original).await.unwrap();

    // Get it via get_or_create_session
    let retrieved = storage.get_or_create_session(&Some(original.id.clone()), "test").await.unwrap();

    assert_eq!(retrieved.id, original.id);
}

#[tokio::test]
async fn test_get_or_create_session_with_id_creates_new() {
    let storage = SqliteStorage::new_in_memory().await.unwrap();

    let custom_id = "custom-session-id".to_string();
    let session = storage.get_or_create_session(&Some(custom_id.clone()), "test").await.unwrap();

    assert_eq!(session.id, custom_id);
    assert_eq!(session.mode, "test");
}
```

---

## Files to Modify

| File | Change Type | Lines Removed | Lines Added |
|------|-------------|---------------|-------------|
| `src/storage/mod.rs` | Add method | 0 | ~25 |
| `src/modes/linear.rs` | Remove/replace | ~15 | 1 |
| `src/modes/tree.rs` | Remove/replace | ~18 | 1 |
| `src/modes/divergent.rs` | Remove/replace | ~18 | 1 |
| `src/modes/reflection.rs` | Remove/replace | ~18 | 1 |
| `src/modes/decision.rs` | Remove/replace | ~18 | 1 |
| `src/modes/evidence.rs` | Remove/replace | ~18 | 1 |
| `src/modes/got.rs` | Remove/replace | ~15 | 1 |
| `src/modes/auto.rs` | Simplify | ~10 | 1 |
| **Total** | | ~130 | ~33 |

**Net reduction**: ~97 lines

---

## Migration Order

1. **Add trait method** (storage/mod.rs) - Non-breaking
2. **Add tests** - Validate new method works
3. **Migrate modes one at a time** (in order of simplicity):
   - `linear.rs` (simplest, good pilot)
   - `divergent.rs`
   - `tree.rs`
   - `reflection.rs`
   - `decision.rs`
   - `evidence.rs`
   - `got.rs`
   - `auto.rs`
4. **Run full test suite** after each migration
5. **Run clippy** to catch unused code

---

## Error Handling Consideration

The trait method returns `StorageResult<Session>`, but modes currently return `AppResult<Session>`. The conversion is automatic via the `From` impl:

```rust
// Already exists in error/mod.rs
impl From<StorageError> for AppError { ... }
```

So mode code can use `?` directly:
```rust
let session = self.storage.get_or_create_session(&params.session_id, "tree").await?;
```

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Break existing behavior | Comprehensive tests before migration |
| Inconsistent mode string | Use ReasoningMode enum's `as_str()` instead of literal strings |
| Performance regression | Method is async + single DB call, same as before |

---

## Optional Enhancement: Use ReasoningMode Enum

For type safety, consider using the existing `ReasoningMode` enum:

```rust
async fn get_or_create_session(
    &self,
    session_id: &Option<String>,
    mode: ReasoningMode,
) -> StorageResult<Session>
```

Then in modes:
```rust
self.storage.get_or_create_session(&params.session_id, ReasoningMode::Tree).await?;
```

This prevents typos like "treee" and ensures consistency with the mode enum.

---

## Success Criteria

1. All 8 mode files use `storage.get_or_create_session()`
2. No remaining `get_or_create_session` methods in mode files
3. All 590+ tests pass
4. Clippy reports no new warnings
5. Net reduction of ~100 lines of code
