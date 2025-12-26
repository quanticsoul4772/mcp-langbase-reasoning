# crates.io Publication Implementation Plan

## Overview

**Objective:** Prepare `mcp-langbase-reasoning` for publication to crates.io
**Current Readiness:** ~70%
**Estimated Effort:** 1-2 hours
**Risk Level:** Low (no code changes required)

---

## Implementation Phases

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    PHASE 1: Required Files                              │
│  ┌─────────────┐                                                        │
│  │ LICENSE     │ ─── Create MIT license file (blocking requirement)     │
│  └─────────────┘                                                        │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    PHASE 2: Cargo.toml Metadata                         │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │ repository  │  │ keywords    │  │ categories  │  │ exclude     │    │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    PHASE 3: Documentation Fixes                         │
│  Fix 19 unresolved doc links in src/lib.rs                              │
│  ┌──────────────────────────────────────────────────────────────────┐  │
│  │ StorageError, LangbaseError, McpError, ToolError, ModeError      │  │
│  │ LinearMode, TreeMode, DivergentMode, ReflectionMode, ...         │  │
│  │ WorkflowPreset, PresetRegistry, execute_preset                   │  │
│  └──────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    PHASE 4: Validation                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │ cargo test  │  │ cargo doc   │  │ cargo       │  │ cargo       │    │
│  │             │  │ --no-deps   │  │ package     │  │ publish     │    │
│  │             │  │             │  │ --list      │  │ --dry-run   │    │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Create LICENSE File

**Priority:** Required (Blocking)
**Effort:** 2 minutes

### Task 1.1: Create LICENSE file

Create `LICENSE` in project root with MIT license text:

```
MIT License

Copyright (c) 2024-2025 MCP Langbase Reasoning Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

---

## Phase 2: Update Cargo.toml Metadata

**Priority:** Strongly Recommended
**Effort:** 5 minutes

### Task 2.1: Add metadata fields

Update `[package]` section in `Cargo.toml`:

```toml
[package]
name = "mcp-langbase-reasoning"
version = "0.1.0"
edition = "2021"
rust-version = "1.70"
authors = ["MCP Langbase Reasoning Contributors"]
description = "MCP server providing structured reasoning via Langbase Pipes - linear, tree, divergent, Graph-of-Thoughts, and decision framework modes"
license = "MIT"
repository = "https://github.com/quanticsoul4772/mcp-langbase-reasoning"
documentation = "https://docs.rs/mcp-langbase-reasoning"
readme = "README.md"
keywords = ["mcp", "reasoning", "langbase", "ai", "llm"]
categories = ["development-tools", "api-bindings"]
```

### Task 2.2: Add exclude patterns

Add exclusion rules to avoid packaging non-essential files:

```toml
exclude = [
    "claudedocs/*",
    "docs/archive/*",
    ".env*",
    ".env.example",
    "data/*",
    "*.db",
    "*.db-journal",
]
```

---

## Phase 3: Fix Documentation Warnings

**Priority:** Recommended
**Effort:** 15-20 minutes

### Task 3.1: Fix unresolved links in src/lib.rs

19 documentation warnings need fixing. The links reference types that exist but aren't properly linked.

**Pattern A: Error types** (5 items)
- `StorageError` → `crate::error::StorageError`
- `LangbaseError` → `crate::error::LangbaseError`
- `McpError` → `crate::error::McpError`
- `ToolError` → `crate::error::ToolError`
- `ModeError` → `crate::error::ModeError`

**Pattern B: Mode types** (10 items)
- `LinearMode` → `crate::modes::LinearMode`
- `TreeMode` → `crate::modes::TreeMode`
- `DivergentMode` → `crate::modes::DivergentMode`
- `ReflectionMode` → `crate::modes::ReflectionMode`
- `BacktrackingMode` → `crate::modes::BacktrackingMode`
- `AutoMode` → `crate::modes::AutoMode`
- `GotMode` → `crate::modes::GotMode`
- `DecisionMode` → `crate::modes::DecisionMode`
- `EvidenceMode` → `crate::modes::EvidenceMode`
- `DetectionMode` → `crate::modes::DetectionMode`

**Pattern C: Infrastructure types** (4 items)
- `ModeCore` → `crate::modes::ModeCore`
- `WorkflowPreset` → `crate::presets::WorkflowPreset`
- `PresetRegistry` → `crate::presets::PresetRegistry`
- `execute_preset` → Remove brackets or use full path

**Fix Strategy:** Update documentation links to use full crate paths:
```rust
/// - [`StorageError`](crate::error::StorageError): Database errors
```

Or escape brackets if not intended as links:
```rust
/// - \[execute_preset\]: Workflow execution
```

---

## Phase 4: Validation

**Priority:** Required
**Effort:** 10 minutes

### Task 4.1: Run test suite

```bash
cargo test
```
**Expected:** All 1913+ tests pass

### Task 4.2: Verify documentation builds cleanly

```bash
cargo doc --no-deps 2>&1 | grep -c "warning:"
```
**Expected:** 0 warnings

### Task 4.3: Check package contents and size

```bash
cargo package --list | wc -l  # Count files
cargo package                  # Build .crate file
ls -lh target/package/*.crate  # Check size < 10MB
```

### Task 4.4: Dry run publish

```bash
cargo publish --dry-run
```
**Expected:** No errors, package validates successfully

---

## Verification Checklist

### Before Publishing

- [ ] LICENSE file exists in project root
- [ ] Cargo.toml has all required metadata
- [ ] Cargo.toml has repository URL
- [ ] Cargo.toml has keywords and categories
- [ ] `cargo test` passes (1913+ tests)
- [ ] `cargo clippy -- -D warnings` passes (0 warnings)
- [ ] `cargo doc --no-deps` passes (0 warnings)
- [ ] `cargo package --list` shows expected files
- [ ] Package size < 10MB
- [ ] `cargo publish --dry-run` succeeds

### Publishing Steps

1. Create crates.io account (via GitHub)
2. Get API token from https://crates.io/me
3. Run `cargo login <token>`
4. Run `cargo publish`
5. Verify at https://crates.io/crates/mcp-langbase-reasoning

---

## Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Name taken | Low | High | Check availability early with dry-run |
| Package too large | Low | Medium | Exclude patterns already defined |
| Doc warnings break build | None | None | docs.rs tolerates warnings |
| API changes needed | None | N/A | No code changes in this plan |

---

## Post-Publication Tasks

1. **Verify docs.rs** - Documentation auto-generates within ~15 minutes
2. **Add badges to README** - crates.io version, docs.rs, license badges
3. **Announce** - Share on relevant forums/communities if desired
4. **Monitor** - Check for any issue reports

---

## Files Changed Summary

| File | Change Type | Description |
|------|-------------|-------------|
| `LICENSE` | Create | MIT license text |
| `Cargo.toml` | Modify | Add metadata, exclude patterns |
| `src/lib.rs` | Modify | Fix 19 documentation links |
