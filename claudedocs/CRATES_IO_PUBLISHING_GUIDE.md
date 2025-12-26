# Publishing to crates.io - Preparation Guide

## Executive Summary

This guide outlines the requirements and steps to publish `mcp-langbase-reasoning` to crates.io. Based on research of official Rust documentation and best practices.

**Current Readiness: ~70%** - Most infrastructure is in place, but several items need attention.

---

## Requirements Checklist

### Required (Blocking)

| Item | Status | Action Needed |
|------|--------|---------------|
| Unique crate name | TBD | Verify `mcp-langbase-reasoning` is available |
| `version` in Cargo.toml | Done | 0.1.0 is set |
| `description` in Cargo.toml | Done | Present but could be improved |
| `license` in Cargo.toml | Done | MIT is set |
| LICENSE file | Missing | Create LICENSE file with MIT text |
| crates.io account | TBD | Register and get API token |

### Strongly Recommended

| Item | Status | Action Needed |
|------|--------|---------------|
| `repository` URL | Missing | Add GitHub repository URL |
| `documentation` URL | Missing | Will auto-generate on docs.rs |
| `homepage` URL | Optional | Can use repository URL |
| `keywords` (max 5) | Missing | Add relevant keywords |
| `categories` (max 5) | Missing | Add category slugs |
| `readme` path | Missing | Add `readme = "README.md"` |
| `authors` | Done | Present |

### Quality Requirements

| Item | Status | Action Needed |
|------|--------|---------------|
| All tests pass | Done | 1913+ tests passing |
| No clippy warnings | Done | 0 warnings |
| Documentation builds | Partial | 19 doc warnings to fix |
| README.md | Done | Present and updated |
| Crate-level docs | Done | lib.rs has comprehensive docs |
| `#![warn(missing_docs)]` | Done | Already enabled |

---

## Detailed Actions

### 1. Create LICENSE File

Create `LICENSE` with MIT license text:

```
MIT License

Copyright (c) 2024 MCP Langbase Reasoning Team

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

### 2. Update Cargo.toml

Add missing metadata:

```toml
[package]
name = "mcp-langbase-reasoning"
version = "0.1.0"
edition = "2021"
authors = ["MCP Langbase Reasoning Team"]
description = "MCP server providing structured reasoning via Langbase Pipes - includes linear, tree, divergent, GoT, and decision framework modes"
license = "MIT"
repository = "https://github.com/quanticsoul4772/mcp-langbase-reasoning"
documentation = "https://docs.rs/mcp-langbase-reasoning"
readme = "README.md"
keywords = ["mcp", "reasoning", "langbase", "ai", "llm"]
categories = ["development-tools", "api-bindings"]

# Exclude non-essential files from crate
exclude = [
    "claudedocs/*",
    "docs/archive/*",
    ".env*",
    "data/*",
]
```

**Recommended Categories:**
- `development-tools` - Developer-facing tools
- `api-bindings` - Wrapper for Langbase API

**Recommended Keywords (max 5):**
- `mcp` - Model Context Protocol
- `reasoning` - Core functionality
- `langbase` - Backend service
- `ai` - Artificial intelligence domain
- `llm` - Large language models

### 3. Fix Documentation Warnings

The `cargo doc` command shows 19 warnings. Fix unresolved links in `src/lib.rs`:

- Replace `[execute_preset]` with proper links or remove brackets
- Fix other broken documentation links

### 4. Verify Package Size

crates.io has a 10MB limit. Check package size:

```bash
cargo package --list  # List files that will be included
cargo package         # Create .crate file and check size
```

### 5. Dry Run

Before publishing, always do a dry run:

```bash
cargo publish --dry-run
```

This validates the package without actually publishing.

### 6. Register and Publish

1. Create account at https://crates.io (via GitHub)
2. Get API token from https://crates.io/me
3. Login: `cargo login <token>`
4. Publish: `cargo publish`

---

## Pre-Publication Verification Commands

```bash
# Run all tests
cargo test

# Check for warnings
cargo clippy -- -D warnings

# Build documentation
cargo doc --no-deps

# Verify package contents
cargo package --list

# Dry run publish
cargo publish --dry-run
```

---

## Important Notes

### Permanent Publication
- **Versions are permanent** - once published, a version cannot be modified or deleted
- Plan version numbers carefully using Semantic Versioning
- Consider starting with 0.1.0 for initial release

### Name Availability
- Names are first-come, first-served
- Check availability: search on crates.io or attempt dry-run publish
- The name `mcp-langbase-reasoning` is long but descriptive

### Yanking vs Deleting
- You cannot delete a published version
- You can "yank" a version to prevent new dependencies
- Yanked versions still work for existing Cargo.lock files

---

## Sources

- [Publishing on crates.io - The Cargo Book](https://doc.rust-lang.org/cargo/reference/publishing.html)
- [Publishing a Crate to Crates.io - The Rust Programming Language](https://doc.rust-lang.org/book/ch14-02-publishing-to-crates-io.html)
- [The Manifest Format - The Cargo Book](https://doc.rust-lang.org/cargo/reference/manifest.html)
- [Rust API Guidelines - Checklist](https://rust-lang.github.io/api-guidelines/checklist.html)
- [How to Write Documentation - rustdoc book](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)
- [crates.io Category Slugs](https://crates.io/category_slugs)
