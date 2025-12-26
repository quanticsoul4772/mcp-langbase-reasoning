# Claude Code Instructions for mcp-langbase-reasoning

## Project Overview

MCP server providing structured reasoning capabilities via Langbase Pipes. Rust implementation with SQLite persistence.

**Status:** Production-ready with 1913+ tests, 0 clippy warnings

## Architecture

```
MCP Client → MCP Server (Rust) → Langbase Pipes (HTTPS)
     ↓              ↓
  JSON-RPC      SQLite DB
```

**Core Components:**
- `src/server/` - MCP JSON-RPC protocol handling
- `src/modes/` - 9 reasoning mode implementations
- `src/presets/` - Workflow preset system (5 built-in)
- `src/storage/` - SQLite persistence with compile-time verified queries
- `src/langbase/` - HTTP client with retry logic
- `src/self_improvement/` - Autonomous 4-phase improvement loop

## Key Documentation

| Document | Location | Purpose |
|----------|----------|---------|
| API Reference | `docs/API_REFERENCE.md` | Complete tool schemas and responses |
| Architecture | `docs/ARCHITECTURE.md` | System design and module structure |
| Langbase API | `docs/LANGBASE_API.md` | Pipe request/response formats |

## Coding Standards

- **Errors:** `thiserror` for types, `anyhow` for propagation
- **Async:** All I/O via `tokio`
- **Logging:** Structured with `tracing`
- **SQL:** `sqlx` with compile-time verification
- **Tests:** In `tests/` directory, `mockall` for mocking

## Common Commands

```bash
cargo build --release    # Build optimized
cargo test               # Run all tests
cargo clippy -- -D warnings  # Lint
cargo run                # Start server (needs LANGBASE_API_KEY)
```

## Environment Variables

**Required:**
- `LANGBASE_API_KEY` - Langbase Pipe API key

**Optional:**
- `DATABASE_PATH` - SQLite path (default: `./data/reasoning.db`)
- `LOG_LEVEL` - Logging level (default: `info`)
- `REQUEST_TIMEOUT_MS` - HTTP timeout (default: `30000`)
- `MAX_RETRIES` - API retry attempts (default: `3`)

## Self-Improvement System

The server includes an autonomous self-improvement loop:

```
Monitor → Analyzer → Executor → Learner → (loop)
```

**CLI Commands:**
```bash
cargo run -- self-improve status      # Current status
cargo run -- self-improve history     # Action history
cargo run -- self-improve enable      # Enable system
cargo run -- self-improve disable     # Disable system
cargo run -- self-improve pause --duration 1h  # Pause
```

## Implementation Patterns

### MCP Tool Handler
```rust
pub async fn handle_tool(params: Params, state: &AppState) -> Result<Response, ToolError> {
    let validated = validate_params(&params)?;
    let session = state.storage.get_or_create_session(&validated.session_id).await?;
    let response = state.langbase.call_pipe("pipe-name", context).await?;
    state.storage.save_result(&session.id, &response).await?;
    Ok(Response::success(response))
}
```

### Error Types
```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Langbase unavailable: {message}")]
    LangbaseUnavailable { message: String, retries: u32 },
    #[error("Invalid input: {field} - {reason}")]
    ValidationError { field: String, reason: String },
    #[error("Session not found: {session_id}")]
    SessionNotFound { session_id: String },
}
```

## Langbase Pipes (8 consolidated)

| Pipe | Purpose |
|------|---------|
| `linear-reasoning-v1` | Sequential reasoning |
| `tree-reasoning-v1` | Branching exploration |
| `divergent-reasoning-v1` | Creative multi-perspective |
| `reflection-v1` | Meta-cognitive analysis |
| `mode-router-v1` | Automatic mode selection |
| `got-reasoning-v1` | Graph-of-Thoughts operations |
| `detection-v1` | Bias and fallacy detection |
| `decision-framework-v1` | Decision, perspective, evidence, Bayesian |
