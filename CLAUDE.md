# Claude Code Instructions for mcp-langbase-reasoning

## Project Context

MCP server delegating structured reasoning to Langbase Pipes. Rust implementation with SQLite persistence. Based on unified-thinking architecture at `../unified-thinking`.

## Key Files

- `IMPLEMENTATION_PLAN.md` - Full technical plan with phases, schemas, tool mappings
- `ENVIRONMENT_SETUP.md` - Development environment setup instructions
- `docs/PRD.md` - Original product requirements document

## Architecture Summary

```
MCP Client → MCP Server (Rust) → Langbase Pipes (HTTP)
                ↓
            SQLite (State)
```

Components:
- `src/server/` - MCP protocol handling, JSON-RPC
- `src/langbase/` - HTTP client, pipe abstractions
- `src/modes/` - Reasoning mode implementations
- `src/storage/` - SQLite persistence layer
- `src/orchestration/` - Context assembly, workflow coordination
- `src/config/` - Environment and settings
- `src/error/` - Error types and handling

## Development Phases

1. **v0.1** - MCP bootstrap + linear mode + SQLite
2. **v0.2** - Tree + divergent + reflection + checkpoints
3. **v0.3** - Backtracking + auto + GoT
4. **v1.0** - Polish, tests, documentation

## Reference Implementation

See `../unified-thinking/internal/modes/` for Go implementations of:
- linear.go
- tree.go
- divergent.go
- reflection.go
- backtracking.go
- auto.go
- graph.go (GoT)

Translate patterns to Rust, not direct ports.

## Coding Standards

- Use `thiserror` for error types
- Use `anyhow` for error propagation in application code
- All async operations via `tokio`
- Structured logging with `tracing`
- SQL via `sqlx` with compile-time verification
- Tests in `tests/` directory, use `mockall` for mocking

## Environment Variables

Required:
- `LANGBASE_API_KEY` - Langbase Pipe API key
- `LANGBASE_BASE_URL` - API endpoint (default: https://api.langbase.com)

Optional:
- `DATABASE_PATH` - SQLite path (default: ./data/reasoning.db)
- `LOG_LEVEL` - Logging level (default: info)

## Common Tasks

### Build
```bash
cargo build
cargo build --release
```

### Test
```bash
cargo test
cargo test --test integration
```

### Run
```bash
cargo run
```

### Lint
```bash
cargo fmt --check
cargo clippy -- -D warnings
```

## MCP Tool Implementation Pattern

```rust
// Each tool follows this pattern:
pub async fn handle_reasoning_linear(
    params: LinearParams,
    state: &AppState,
) -> Result<ToolResponse, ToolError> {
    // 1. Validate input
    let validated = validate_params(&params)?;
    
    // 2. Load/create session from SQLite
    let session = state.storage.get_or_create_session(&validated.session_id).await?;
    
    // 3. Assemble context for Langbase
    let context = assemble_context(&session, &validated)?;
    
    // 4. Call Langbase Pipe
    let response = state.langbase.call_pipe("linear-reasoning-v1", context).await?;
    
    // 5. Parse and validate response
    let thought = parse_thought_response(response)?;
    
    // 6. Persist to SQLite
    state.storage.save_thought(&session.id, &thought).await?;
    
    // 7. Return MCP response
    Ok(ToolResponse::success(thought))
}
```

## Langbase Pipe Request Format

```rust
#[derive(Serialize)]
struct PipeRequest {
    messages: Vec<Message>,
    variables: Option<HashMap<String, String>>,
    #[serde(rename = "threadId")]
    thread_id: Option<String>,
}

// POST to: https://api.langbase.com/v1/pipes/run
// Header: Authorization: Bearer {api_key}
```

## SQLite Patterns

Use sqlx macros for compile-time verification:

```rust
let thought = sqlx::query_as!(
    Thought,
    r#"
    SELECT id, session_id, content, confidence, mode, parent_id, created_at, metadata
    FROM thoughts
    WHERE session_id = ?
    ORDER BY created_at DESC
    LIMIT 1
    "#,
    session_id
)
.fetch_optional(&state.db)
.await?;
```

## Error Handling

Use structured errors for MCP responses:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Langbase unavailable: {message}")]
    LangbaseUnavailable { message: String, retries: u32 },
    
    #[error("Invalid input: {field} - {reason}")]
    ValidationError { field: String, reason: String },
    
    #[error("Session not found: {session_id}")]
    SessionNotFound { session_id: String },
    
    #[error("Storage error: {0}")]
    StorageError(#[from] sqlx::Error),
}
```

## Questions for Development

1. MCP Rust SDK availability? Use jsonrpc-core for now.
2. Streaming support from Langbase? Implement after v0.1.
3. Retry strategy specifics? Exponential backoff, max 3 retries.
