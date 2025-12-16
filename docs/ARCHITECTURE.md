# Architecture

Technical architecture documentation for mcp-langbase-reasoning.

## System Overview

```
                              MCP Client
                         (Claude Desktop, etc.)
                                  |
                                  | stdio (JSON-RPC 2.0)
                                  v
+-------------------------------------------------------------------------+
|                        mcp-langbase-reasoning                            |
|  +-------------+  +---------------+  +-----------+  +-----------------+ |
|  |  MCP Layer  |  |    Modes      |  |  Storage  |  | Langbase Client | |
|  |             |  |               |  |           |  |                 | |
|  | - Protocol  |  | - Linear      |  | - Sessions|  | - HTTP Client   | |
|  | - Routing   |  | - Tree        |  | - Thoughts|  | - Retry Logic   | |
|  | - Schema    |  | - Divergent   |  | - Branches|  | - Response Parse| |
|  |             |  | - Reflection  |  | - Checks  |  |                 | |
|  |             |  | - Backtrack   |  | - Graphs  |  |                 | |
|  |             |  | - Auto        |  | - Invoc.  |  |                 | |
|  |             |  | - GoT         |  |           |  |                 | |
|  +-------------+  +---------------+  +-----------+  +-----------------+ |
|                           |                |                  |         |
|                           v                v                  |         |
|                    +---------------------------+              |         |
|                    |         SQLite DB          |              |         |
|                    | (sessions, thoughts,       |              |         |
|                    |  branches, checkpoints,    |              |         |
|                    |  graphs, invocations)      |              |         |
|                    +---------------------------+              |         |
+------------------------------------------------------------- | --------+
                                                               |
                                                               | HTTPS
                                                               v
                                                 +-------------------------+
                                                 |     Langbase API        |
                                                 |                         |
                                                 |  +-------------------+  |
                                                 |  | linear-reason-v1  |  |
                                                 |  | tree-reasoning-v1 |  |
                                                 |  | divergent-v1      |  |
                                                 |  | reflection-v1     |  |
                                                 |  | got-generate-v1   |  |
                                                 |  | got-score-v1      |  |
                                                 |  | got-aggregate-v1  |  |
                                                 |  | got-refine-v1     |  |
                                                 |  +-------------------+  |
                                                 +-------------------------+
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
│   ├── mod.rs           # Mode exports, ReasoningMode enum
│   ├── linear.rs        # Linear reasoning implementation
│   ├── tree.rs          # Tree/branching reasoning
│   ├── divergent.rs     # Divergent/creative reasoning
│   ├── reflection.rs    # Meta-cognitive reflection
│   ├── backtracking.rs  # Checkpoint and backtrack
│   ├── auto.rs          # Automatic mode selection
│   └── got.rs           # Graph-of-Thoughts operations
├── prompts.rs           # Centralized system prompts
├── server/
│   ├── mod.rs           # AppState, SharedState
│   ├── mcp.rs           # JSON-RPC protocol handling
│   └── handlers.rs      # Tool call routing
└── storage/
    ├── mod.rs           # Storage trait, domain types
    └── sqlite.rs        # SQLite implementation

tests/
├── config_env_test.rs   # Configuration tests
├── integration_test.rs  # Mode integration tests
├── langbase_test.rs     # HTTP client tests with mocks
├── mcp_protocol_test.rs # JSON-RPC compliance tests
├── modes_test.rs        # Mode-specific tests
└── storage_test.rs      # SQLite integration tests

migrations/
├── 20240101000001_initial_schema.sql       # Sessions, thoughts, invocations
├── 20240102000001_branches_checkpoints.sql # Branches, checkpoints, snapshots
└── 20240103000001_graphs.sql               # Graph nodes and edges
```

## Component Details

### MCP Layer (server/mcp.rs)

Handles JSON-RPC 2.0 protocol over async stdio.

Responsibilities:
- Parse incoming JSON-RPC requests
- Route to appropriate handlers
- Serialize responses
- Handle protocol-level errors
- Handle notifications (no response for notifications per JSON-RPC 2.0)

Key Types:
```rust
pub struct McpServer { state: SharedState }
pub struct JsonRpcRequest { jsonrpc, id, method, params }
pub struct JsonRpcResponse { jsonrpc, id, result, error }
```

Supported Methods:
| Method | Description |
|--------|-------------|
| `initialize` | MCP handshake |
| `initialized` | Acknowledge init (notification) |
| `tools/list` | List available tools |
| `tools/call` | Execute a tool |
| `ping` | Health check |

### Tool Handlers (server/handlers.rs)

Routes tool calls to mode implementations.

```rust
pub async fn handle_tool_call(
    state: &SharedState,
    tool_name: &str,
    arguments: Option<Value>,
) -> McpResult<Value>
```

Tool routing:
- `reasoning_linear` -> LinearMode
- `reasoning_tree`, `reasoning_tree_*` -> TreeMode
- `reasoning_divergent` -> DivergentMode
- `reasoning_reflection`, `reasoning_reflection_*` -> ReflectionMode
- `reasoning_backtrack`, `reasoning_checkpoint_*` -> BacktrackingMode
- `reasoning_auto` -> AutoMode
- `reasoning_got_*` -> GotMode

### Reasoning Modes (modes/)

Each mode implements a specific reasoning pattern.

#### Linear Mode (modes/linear.rs)
- Single-pass sequential reasoning
- Builds on previous thoughts in session
- Returns structured JSON output

```rust
pub struct LinearMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}
```

#### Tree Mode (modes/tree.rs)
- Branching exploration with multiple paths
- Branch management (focus, list, complete)
- Tracks branch state and confidence

```rust
pub struct TreeMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    pipe_name: String,
}
```

#### Divergent Mode (modes/divergent.rs)
- Creative reasoning with multiple perspectives
- Generates novel viewpoints
- Synthesizes diverse insights

#### Reflection Mode (modes/reflection.rs)
- Meta-cognitive analysis
- Evaluates reasoning quality
- Provides improvement recommendations

#### Backtracking Mode (modes/backtracking.rs)
- Checkpoint creation and management
- State restoration
- Alternative path exploration

#### Auto Mode (modes/auto.rs)
- Analyzes content for mode selection
- Local heuristics for common patterns
- Langbase-powered routing for complex cases

#### Graph-of-Thoughts Mode (modes/got.rs)
- Graph-based reasoning structure
- Node generation, scoring, aggregation
- Pruning and refinement operations
- Cycle detection and finalization

```rust
pub struct GotMode {
    storage: SqliteStorage,
    langbase: LangbaseClient,
    generate_pipe: String,
    score_pipe: String,
    aggregate_pipe: String,
    refine_pipe: String,
}
```

### Langbase Client (langbase/client.rs)

HTTP client for Langbase Pipes API with retry logic.

Features:
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
}
```

### Storage Layer (storage/)

SQLite-backed persistence with compile-time migrations.

Trait Definition:
```rust
#[async_trait]
pub trait Storage: Send + Sync {
    // Sessions
    async fn create_session(&self, session: &Session) -> StorageResult<()>;
    async fn get_session(&self, id: &str) -> StorageResult<Option<Session>>;
    async fn update_session(&self, session: &Session) -> StorageResult<()>;
    async fn delete_session(&self, id: &str) -> StorageResult<()>;

    // Thoughts
    async fn create_thought(&self, thought: &Thought) -> StorageResult<()>;
    async fn get_thought(&self, id: &str) -> StorageResult<Option<Thought>>;
    async fn get_session_thoughts(&self, session_id: &str) -> StorageResult<Vec<Thought>>;
    async fn get_latest_thought(&self, session_id: &str) -> StorageResult<Option<Thought>>;

    // Branches
    async fn create_branch(&self, branch: &Branch) -> StorageResult<()>;
    async fn get_branch(&self, id: &str) -> StorageResult<Option<Branch>>;
    async fn get_session_branches(&self, session_id: &str) -> StorageResult<Vec<Branch>>;
    async fn update_branch(&self, branch: &Branch) -> StorageResult<()>;

    // Checkpoints
    async fn create_checkpoint(&self, checkpoint: &Checkpoint) -> StorageResult<()>;
    async fn get_checkpoint(&self, id: &str) -> StorageResult<Option<Checkpoint>>;
    async fn get_session_checkpoints(&self, session_id: &str) -> StorageResult<Vec<Checkpoint>>;

    // Snapshots
    async fn create_snapshot(&self, snapshot: &StateSnapshot) -> StorageResult<()>;

    // Graph nodes and edges
    async fn create_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    async fn get_graph_node(&self, id: &str) -> StorageResult<Option<GraphNode>>;
    async fn get_graph_nodes(&self, graph_id: &str) -> StorageResult<Vec<GraphNode>>;
    async fn update_graph_node(&self, node: &GraphNode) -> StorageResult<()>;
    async fn delete_graph_node(&self, id: &str) -> StorageResult<()>;
    async fn create_graph_edge(&self, edge: &GraphEdge) -> StorageResult<()>;
    async fn get_graph_edges(&self, graph_id: &str) -> StorageResult<Vec<GraphEdge>>;

    // Invocations
    async fn log_invocation(&self, invocation: &Invocation) -> StorageResult<()>;
}
```

Database Schema:
```sql
-- Sessions: reasoning context groupings
sessions (id, mode, created_at, updated_at, metadata, active_branch_id)

-- Thoughts: individual reasoning steps
thoughts (id, session_id, content, confidence, mode, parent_id, branch_id, created_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> thoughts.id (SET NULL)
  FK: branch_id -> branches.id (SET NULL)

-- Branches: tree mode exploration paths
branches (id, session_id, name, parent_id, state, confidence, priority, created_at, updated_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> branches.id (SET NULL)

-- Cross-references: branch relationships
cross_refs (id, from_branch, to_branch, ref_type, reason, strength, created_at)
  FK: from_branch -> branches.id (CASCADE DELETE)
  FK: to_branch -> branches.id (CASCADE DELETE)

-- Checkpoints: saved states for backtracking
checkpoints (id, session_id, branch_id, name, description, snapshot, created_at)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: branch_id -> branches.id (SET NULL)

-- State snapshots: detailed state captures
state_snapshots (id, session_id, parent_id, snapshot_type, description, data, created_at)
  FK: session_id -> sessions.id (CASCADE DELETE)
  FK: parent_id -> state_snapshots.id (SET NULL)

-- Graph nodes: GoT reasoning nodes
graph_nodes (id, graph_id, session_id, content, node_type, score, depth, is_active, is_terminal, created_at, metadata)
  FK: session_id -> sessions.id (CASCADE DELETE)

-- Graph edges: GoT node relationships
graph_edges (id, graph_id, from_node, to_node, edge_type, weight, created_at, metadata)
  FK: from_node -> graph_nodes.id (CASCADE DELETE)
  FK: to_node -> graph_nodes.id (CASCADE DELETE)

-- Invocations: API call audit log
invocations (id, session_id, tool_name, input, output, pipe_name, latency_ms, success, error, created_at)
  FK: session_id -> sessions.id (SET NULL)
```

### Prompts (prompts.rs)

Centralized system prompts for all reasoning modes.

```rust
pub const LINEAR_REASONING_PROMPT: &str = r#"..."#;
pub const TREE_REASONING_PROMPT: &str = r#"..."#;
pub const DIVERGENT_REASONING_PROMPT: &str = r#"..."#;
pub const REFLECTION_PROMPT: &str = r#"..."#;
pub const AUTO_ROUTER_PROMPT: &str = r#"..."#;
pub const BACKTRACKING_PROMPT: &str = r#"..."#;
pub const GOT_GENERATE_PROMPT: &str = r#"..."#;
pub const GOT_SCORE_PROMPT: &str = r#"..."#;
pub const GOT_AGGREGATE_PROMPT: &str = r#"..."#;
pub const GOT_REFINE_PROMPT: &str = r#"..."#;

pub fn get_prompt_for_mode(mode: &str) -> &'static str;
```

### Error Handling (error/mod.rs)

Hierarchical error types with conversions.

```
AppError (top-level)
├── Config { message }
├── Storage(StorageError)
│   ├── Connection { message }
│   ├── Query { message }
│   ├── SessionNotFound { session_id }
│   ├── ThoughtNotFound { thought_id }
│   ├── BranchNotFound { branch_id }
│   ├── CheckpointNotFound { checkpoint_id }
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
├── Branch(String)
├── Checkpoint(String)
├── Graph(String)
└── Reasoning { message }
```

## Data Flow

### Tool Call Flow

```
1. MCP Client sends JSON-RPC request over stdio
2. McpServer::run() reads line asynchronously
3. Parse as JsonRpcRequest
4. Check if notification (no id) - if so, process without response
5. Route to handle_request() by method
6. For tools/call:
   a. Parse ToolCallParams
   b. Route to handle_tool_call() by tool name
   c. Deserialize arguments to mode-specific params
   d. Call mode.process()
7. Mode processing:
   a. Validate input
   b. Get/create session from storage
   c. Load previous thoughts/branches for context
   d. Build messages array
   e. Call Langbase pipe
   f. Parse reasoning response
   g. Store thought/branch/node in SQLite
   h. Log invocation
   i. Return structured result
8. Serialize result to JSON-RPC response
9. Write to stdout with newline
10. Flush stdout
```

### Session Continuity

```
First call (no session_id):
  ├── Create new Session with UUID
  ├── Store in SQLite
  └── Return session_id in response

Subsequent calls (with session_id):
  ├── Load Session from SQLite
  ├── Load previous Thoughts/Branches
  ├── Include in Langbase context
  └── Link new Thought to session
```

### Graph-of-Thoughts Flow

```
1. Initialize graph (got_init):
   ├── Create session if needed
   ├── Create root GraphNode
   └── Return graph_id

2. Generate continuations (got_generate):
   ├── Load source node
   ├── Call Langbase for k continuations
   ├── Create child GraphNodes
   ├── Create GraphEdges
   └── Return new node IDs

3. Score nodes (got_score):
   ├── Load node content
   ├── Call Langbase for quality assessment
   ├── Update node score
   └── Return breakdown

4. Aggregate nodes (got_aggregate):
   ├── Load source nodes
   ├── Call Langbase for synthesis
   ├── Create aggregation node
   └── Create aggregation edges

5. Refine node (got_refine):
   ├── Load node
   ├── Call Langbase for improvement
   ├── Create refinement node
   └── Create refine edge

6. Prune low-scoring (got_prune):
   ├── Load all nodes
   ├── Filter by threshold
   ├── Delete below-threshold nodes
   └── Return pruned count

7. Finalize (got_finalize):
   ├── Mark terminal nodes
   ├── Collect conclusions
   └── Return final insights
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
    pub got_pipes: GotPipeConfig,    // GoT-specific pipes
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
| Config | Unit | `tests/config_env_test.rs` |
| Modes | Unit/Integration | `tests/modes_test.rs`, `tests/integration_test.rs` |
| Prompts | Unit | `src/prompts.rs` (inline) |

Total test count: 258 tests across all modules.
