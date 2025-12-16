# Architecture

Technical architecture documentation for mcp-langbase-reasoning.

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              MCP Client                                      │
│                         (Claude Desktop, etc.)                               │
└─────────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ stdio (JSON-RPC 2.0)
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        mcp-langbase-reasoning                                │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  MCP Layer  │  │   Modes     │  │  Storage    │  │  Langbase Client    │ │
│  │             │  │             │  │             │  │                     │ │
│  │ - Protocol  │  │ - Linear    │  │ - Sessions  │  │ - HTTP Client       │ │
│  │ - Routing   │  │ - Tree*     │  │ - Thoughts  │  │ - Retry Logic       │ │
│  │ - Schema    │  │ - Divergent*│  │ - Invocatn  │  │ - Response Parse    │ │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│                           │                │                  │              │
│                           ▼                ▼                  │              │
│                    ┌─────────────────────────┐                │              │
│                    │       SQLite DB          │                │              │
│                    │   (sessions, thoughts,   │                │              │
│                    │    invocations)          │                │              │
│                    └─────────────────────────┘                │              │
└───────────────────────────────────────────────────────────────┼──────────────┘
                                                                │
                                                                │ HTTPS
                                                                ▼
                                                  ┌─────────────────────────┐
                                                  │     Langbase API        │
                                                  │                         │
                                                  │  ┌─────────────────┐   │
                                                  │  │ linear-reason.. │   │
                                                  │  │ tree-reasoning* │   │
                                                  │  │ divergent-rea*  │   │
                                                  │  └─────────────────┘   │
                                                  └─────────────────────────┘

* = Planned for future versions
```

## Module Structure

```
src/
├── main.rs              # Entry point, runtime setup
├── lib.rs               # Public API exports
├── config/
│   └── mod.rs           # Configuration loading from env
├── error/
│   └── mod.rs           # Error types (App, Storage, Langbase, MCP, Tool)
├── langbase/
│   ├── mod.rs           # Module exports
│   ├── client.rs        # HTTP client with retry logic
│   └── types.rs         # Request/response types
├── modes/
│   ├── mod.rs           # Mode exports
│   └── linear.rs        # Linear reasoning implementation
├── prompts/
│   └── mod.rs           # Centralized system prompts
├── server/
│   ├── mod.rs           # AppState, SharedState
│   ├── mcp.rs           # JSON-RPC protocol handling
│   └── handlers.rs      # Tool call routing
└── storage/
    ├── mod.rs           # Storage trait, domain types
    └── sqlite.rs        # SQLite implementation

tests/
├── mcp_protocol_test.rs # JSON-RPC compliance tests
├── storage_test.rs      # SQLite integration tests
└── langbase_test.rs     # HTTP client tests with mocks

migrations/
└── 20240101000001_initial_schema.sql  # Database schema
```

## Component Details

### MCP Layer (`server/mcp.rs`)

Handles JSON-RPC 2.0 protocol over async stdio.

**Responsibilities:**
- Parse incoming JSON-RPC requests
- Route to appropriate handlers
- Serialize responses
- Handle protocol-level errors

**Key Types:**
```rust
pub struct McpServer { state: SharedState }
pub struct JsonRpcRequest { jsonrpc, id, method, params }
pub struct JsonRpcResponse { jsonrpc, id, result, error }
```

**Supported Methods:**
| Method | Description |
|--------|-------------|
| `initialize` | MCP handshake |
| `initialized` | Acknowledge init |
| `tools/list` | List available tools |
| `tools/call` | Execute a tool |
| `ping` | Health check |

### Tool Handlers (`server/handlers.rs`)

Routes tool calls to mode implementations.

```rust
pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value>
```

### Reasoning Modes (`modes/`)

Each mode implements a specific reasoning pattern.

**Linear Mode (`modes/linear.rs`):**
- Single-pass sequential reasoning
- Builds on previous thoughts in session
- Returns structured JSON output

```rust
pub struct LinearMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}

impl LinearMode {
    pub async fn process(&self, params: LinearParams) -> AppResult<LinearResult>;
}
```

### Langbase Client (`langbase/client.rs`)

HTTP client for Langbase Pipes API with retry logic.

**Features:**
- Configurable timeout and retries
- Exponential backoff
- Request/response logging
- Pipe creation and management

```rust
pub struct LangbaseClient {
    client: Client,
    base_url: String,
    api_key: String,
    request_config: RequestConfig,
}

impl LangbaseClient {
    pub async fn call_pipe(&self, request: PipeRequest) -> LangbaseResult<PipeResponse>;
    pub async fn create_pipe(&self, request: CreatePipeRequest) -> LangbaseResult<CreatePipeResponse>;
    pub async fn ensure_linear_pipe(&self, pipe_name: &str) -> LangbaseResult<()>;
}
```

### Storage Layer (`storage/`)

SQLite-backed persistence with compile-time migrations.

**Trait Definition:**
```rust
#[async_trait]
pub trait Storage: Send + Sync {
    async fn create_session(&self, session: &Session) -> StorageResult<()>;
    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>>;
    async fn update_session(&self, session: &Session) -> StorageResult<()>;
    async fn delete_session(&self, id: &str) -> StorageResult<()>;

    async fn create_thought(&self, thought: &Thought) -> StorageResult<()>;
    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>>;
    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>>;
    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>>;

    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;
}
```

**Database Schema:**
```sql
-- Sessions: reasoning context groupings
sessions (id, mode, created_at, updated_at, metadata)

-- Thoughts: individual reasoning steps
thoughts (id, session_id, content, confidence, mode, parent_id, created_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> thoughts.id (SET NULL)

-- Invocations: API call audit log
invocations (id, session_id, tool_name, input, output, pipe_name, latency_ms, success, error, created_at)
  FK: session_id -> sessions.id (SET NULL)
```

### Prompts (`prompts.rs`)

Centralized system prompts for all reasoning modes.

```rust
pub const LINEAR_REASONING_PROMPT: &str = r#"..."#;
pub const TREE_REASONING_PROMPT: &str = r#"..."#;
pub const DIVERGENT_REASONING_PROMPT: &str = r#"..."#;
pub const REFLECTION_PROMPT: &str = r#"..."#;
pub const AUTO_ROUTER_PROMPT: &str = r#"..."#;

pub fn get_prompt_for_mode(mode: &str) -> &'static str;
```

### Error Handling (`error/mod.rs`)

Hierarchical error types with conversions.

```
AppError (top-level)
├── Config { message }
├── Storage(StorageError)
│   ├── Connection { message }
│   ├── Query { message }
│   ├── SessionNotFound { session_id }
│   ├── ThoughtNotFound { thought_id }
│   ├── Migration { message }
│   └── Sqlx(sqlx::Error)
├── Langbase(LangbaseError)
│   ├── Unavailable { message, retries }
│   ├── Api { status, message }
│   ├── InvalidResponse { message }
│   ├── Timeout { timeout_ms }
│   └── Http(reqwest::Error)
├── Mcp(McpError)
│   ├── InvalidRequest { message }
│   ├── UnknownTool { tool_name }
│   ├── InvalidParameters { tool_name, message }
│   ├── ExecutionFailed { message }
│   └── Json(serde_json::Error)
└── Internal { message }

ToolError (tool-specific)
├── Validation { field, reason }
├── Session(String)
└── Reasoning { message }
```

## Data Flow

### Tool Call Flow

```
1. MCP Client sends JSON-RPC request over stdio
2. McpServer::run() reads line asynchronously
3. Parse as JsonRpcRequest
4. Route to handle_request() by method
5. For tools/call:
   a. Parse ToolCallParams
   b. Route to handle_tool_call() by tool name
   c. Deserialize arguments to mode-specific params
   d. Call mode.process()
6. Mode processing:
   a. Validate input
   b. Get/create session from storage
   c. Load previous thoughts for context
   d. Build messages array
   e. Call Langbase pipe
   f. Parse reasoning response
   g. Store thought in SQLite
   h. Log invocation
   i. Return structured result
7. Serialize result to JSON-RPC response
8. Write to stdout with newline
9. Flush stdout
```

### Session Continuity

```
First call (no session_id):
  ├── Create new Session with UUID
  ├── Store in SQLite
  └── Return session_id in response

Subsequent calls (with session_id):
  ├── Load Session from SQLite
  ├── Load previous Thoughts
  ├── Include in Langbase context
  └── Link new Thought to session
```

## Configuration

Configuration is loaded from environment variables at startup:

```rust
pub struct Config {
    pub langbase: LangbaseConfig,   // API key, base URL
    pub database: DatabaseConfig,    // Path, max connections
    pub logging: LoggingConfig,      // Level, format
    pub request: RequestConfig,      // Timeout, retries
    pub pipes: PipeConfig,           // Pipe names per mode
}
```

## Concurrency Model

- Single async Tokio runtime
- Async stdio for non-blocking I/O
- SQLite connection pool (default 5 connections)
- HTTP client with connection pooling
- Shared state via `Arc<AppState>`

## Security Considerations

- API key stored in environment variable only
- No secrets logged (tracing configured appropriately)
- SQLite file permissions follow filesystem defaults
- HTTPS required for Langbase API
- Input validation before processing

## Testing Strategy

| Layer | Test Type | Location |
|-------|-----------|----------|
| MCP Protocol | Unit/Integration | `tests/mcp_protocol_test.rs` |
| Storage | Integration | `tests/storage_test.rs` |
| Langbase Client | Integration (mocked) | `tests/langbase_test.rs` |
| Prompts | Unit | `src/prompts.rs` (inline) |

## Future Extensions

Planned components for v0.2+:

- **Tree Mode**: Branching reasoning with parent/child relationships
- **Divergent Mode**: Parallel path generation
- **Reflection Mode**: Self-critique and improvement
- **Backtracking**: Checkpoint restore functionality
- **Auto Mode**: Automatic mode selection based on input
- **Graph-of-Thoughts**: Complex graph-based reasoning
