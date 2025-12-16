# mcp-langbase-reasoning Implementation Plan

## Overview

MCP server delegating structured reasoning to Langbase Pipes while maintaining local state, schemas, and orchestration. Based on unified-thinking architecture, targeting feature parity.

## Architecture Decisions

### Core Principle: Thin Proxy with Rich State

The server acts as a stateful orchestrator. All generative reasoning happens in Langbase Pipes. Local components handle: MCP protocol, state persistence, context management, error normalization.

### Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust (stable) | Performance, safety, MCP SDK availability |
| Persistence | SQLite + sqlx | Embedded, reliable, sufficient for state |
| HTTP Client | reqwest | Async, well-maintained |
| Async Runtime | tokio | Standard for Rust async |
| Logging | tracing | Structured, async-compatible |
| Config | Environment vars | 12-factor, secret-safe |

### Langbase Integration Pattern

```
[MCP Tool Call] 
    → [Local Orchestrator]
        → [Context Assembly]
            → [Langbase Pipe HTTP POST]
                ← [JSON Response]
            ← [Response Validation]
        ← [State Update]
    ← [MCP Response]
```

## Development Phases

### Phase 0: Environment Setup (Day 1)

**Prerequisites:**
- Rust toolchain (rustup)
- SQLite3
- Langbase account + API key
- VS Code with rust-analyzer

**Tasks:**
1. Initialize Cargo workspace
2. Configure dependencies in Cargo.toml
3. Set up .env template
4. Create database migrations structure
5. Configure CI basics (clippy, fmt, test)

### Phase 1: Foundation (v0.1) - Week 1

**Goal:** MCP server running, single tool working, SQLite operational.

**Deliverables:**
- MCP server bootstrap (stdio transport)
- `reasoning.linear` tool functional
- SQLite schema v1 (sessions, thoughts)
- Single Langbase Pipe integration
- Basic error handling

**Milestones:**
- [ ] `cargo run` starts MCP server
- [ ] Claude can call `reasoning.linear`
- [ ] Thoughts persist to SQLite
- [ ] Langbase Pipe receives/returns JSON

### Phase 2: Core Modes (v0.2) - Week 2-3

**Goal:** Tree, divergent, reflection modes operational.

**Deliverables:**
- `reasoning.tree` with branching
- `reasoning.divergent` with parallel paths
- `reasoning.reflect` with self-critique
- Checkpoint system
- Session restore capability

**Schema Extensions:**
- Branches table
- Checkpoints table
- Parent-child relationships

### Phase 3: Advanced Modes (v0.3) - Week 4

**Goal:** Backtracking, auto-routing, GoT.

**Deliverables:**
- `reasoning.backtrack` with state snapshots
- `reasoning.auto` mode router
- `reasoning.got.*` graph operations
- Cycle detection
- Graph traversal

**Schema Extensions:**
- Graph nodes table
- Graph edges table
- Snapshot blobs

### Phase 4: Polish (v1.0) - Week 5

**Goal:** Production-ready, documented.

**Deliverables:**
- Import/export compatibility
- Comprehensive test suite
- Performance optimization
- Full documentation
- Deployment guide

## Directory Structure

```
mcp-langbase-reasoning/
├── Cargo.toml
├── Cargo.lock
├── .env.example
├── .gitignore
├── README.md
├── IMPLEMENTATION_PLAN.md
├── migrations/
│   ├── 001_initial.sql
│   ├── 002_branches.sql
│   └── 003_graphs.sql
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── server/
│   │   ├── mod.rs
│   │   ├── mcp.rs
│   │   └── handlers.rs
│   ├── langbase/
│   │   ├── mod.rs
│   │   ├── client.rs
│   │   ├── pipes.rs
│   │   └── types.rs
│   ├── modes/
│   │   ├── mod.rs
│   │   ├── linear.rs
│   │   ├── tree.rs
│   │   ├── divergent.rs
│   │   ├── reflection.rs
│   │   ├── backtracking.rs
│   │   ├── auto.rs
│   │   └── got.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── sqlite.rs
│   │   ├── sessions.rs
│   │   ├── thoughts.rs
│   │   └── graphs.rs
│   ├── orchestration/
│   │   ├── mod.rs
│   │   ├── context.rs
│   │   └── workflow.rs
│   ├── config/
│   │   ├── mod.rs
│   │   └── settings.rs
│   └── error/
│       └── mod.rs
├── tests/
│   ├── integration/
│   └── mocks/
└── docs/
    ├── architecture.md
    ├── api.md
    └── langbase-pipes.md
```

## SQLite Schema Design

### Version 1 (Phase 1)

```sql
-- sessions: top-level reasoning contexts
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    mode TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata TEXT -- JSON blob
);

-- thoughts: individual reasoning steps
CREATE TABLE thoughts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    content TEXT NOT NULL,
    confidence REAL,
    mode TEXT NOT NULL,
    parent_id TEXT REFERENCES thoughts(id),
    created_at TEXT NOT NULL,
    metadata TEXT -- JSON blob
);

CREATE INDEX idx_thoughts_session ON thoughts(session_id);
CREATE INDEX idx_thoughts_parent ON thoughts(parent_id);
```

### Version 2 (Phase 2)

```sql
-- branches: tree mode branching
CREATE TABLE branches (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    name TEXT,
    parent_branch_id TEXT REFERENCES branches(id),
    created_at TEXT NOT NULL,
    metadata TEXT
);

-- checkpoints: state snapshots for backtracking
CREATE TABLE checkpoints (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    branch_id TEXT REFERENCES branches(id),
    name TEXT NOT NULL,
    snapshot TEXT NOT NULL, -- serialized state
    created_at TEXT NOT NULL
);
```

### Version 3 (Phase 3)

```sql
-- graph nodes: GoT vertices
CREATE TABLE graph_nodes (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    content TEXT NOT NULL,
    node_type TEXT,
    score REAL,
    is_terminal INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    metadata TEXT
);

-- graph edges: GoT connections
CREATE TABLE graph_edges (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    from_node TEXT NOT NULL REFERENCES graph_nodes(id),
    to_node TEXT NOT NULL REFERENCES graph_nodes(id),
    edge_type TEXT,
    weight REAL DEFAULT 1.0,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_edges_from ON graph_edges(from_node);
CREATE INDEX idx_edges_to ON graph_edges(to_node);
```

## Langbase Pipe Mapping

| MCP Tool | Langbase Pipe | Variables |
|----------|---------------|-----------|
| reasoning.linear | `linear-reasoning-v1` | context, prompt, history |
| reasoning.tree | `tree-reasoning-v1` | context, branch_context, prompt |
| reasoning.divergent | `divergent-reasoning-v1` | context, num_paths, prompt |
| reasoning.reflect | `reflection-v1` | thought, critique_focus |
| reasoning.auto | `mode-router-v1` | context, problem_type |
| reasoning.got.generate | `got-generate-v1` | node_content, k |
| reasoning.got.score | `got-score-v1` | node_content, criteria |
| reasoning.got.aggregate | `got-aggregate-v1` | nodes, strategy |

## MCP Tool Schemas

### reasoning.linear

```json
{
  "name": "reasoning.linear",
  "description": "Single-pass sequential reasoning",
  "inputSchema": {
    "type": "object",
    "properties": {
      "content": { "type": "string", "description": "Thought to process" },
      "session_id": { "type": "string" },
      "confidence": { "type": "number", "minimum": 0, "maximum": 1 }
    },
    "required": ["content"]
  }
}
```

### reasoning.tree

```json
{
  "name": "reasoning.tree",
  "description": "Branching tree-structured reasoning",
  "inputSchema": {
    "type": "object",
    "properties": {
      "content": { "type": "string" },
      "session_id": { "type": "string" },
      "branch_id": { "type": "string" },
      "create_branch": { "type": "boolean" }
    },
    "required": ["content"]
  }
}
```

## Configuration

### Environment Variables

```bash
# Required
LANGBASE_API_KEY=pipe_xxx
LANGBASE_BASE_URL=https://api.langbase.com

# Optional
DATABASE_PATH=./data/reasoning.db
LOG_LEVEL=info
REQUEST_TIMEOUT_MS=30000
MAX_RETRIES=3

# Pipe-specific (optional overrides)
PIPE_LINEAR=linear-reasoning-v1
PIPE_TREE=tree-reasoning-v1
PIPE_DIVERGENT=divergent-reasoning-v1
```

## Error Handling Strategy

### Error Categories

1. **Network Errors** → Retry with backoff, then structured error
2. **Langbase API Errors** → Normalize to MCP error format
3. **Validation Errors** → Return immediately with details
4. **Storage Errors** → Log, attempt recovery, then error
5. **State Corruption** → Isolate session, return error

### MCP Error Response Format

```json
{
  "error": {
    "code": "LANGBASE_UNAVAILABLE",
    "message": "Langbase Pipe unreachable after 3 retries",
    "details": {
      "pipe": "linear-reasoning-v1",
      "last_attempt": "2025-01-15T10:30:00Z"
    }
  }
}
```

## Testing Strategy

### Unit Tests
- Each mode module
- Storage operations
- Langbase client (mocked)
- Error handling paths

### Integration Tests
- Full MCP flow with mock Langbase
- SQLite persistence round-trips
- Session lifecycle
- Mode transitions

### Contract Tests
- Langbase API contracts
- MCP protocol compliance

## Observability

### Structured Logging

```rust
tracing::info!(
    session_id = %session_id,
    mode = %mode,
    pipe = %pipe_name,
    latency_ms = %latency,
    "Langbase pipe call completed"
);
```

### Metrics (Future)

- Request latency histograms
- Pipe call success/failure rates
- Session counts
- Storage size

## Open Questions (from PRD)

1. **GoT Schema Standardization** → Use unified-thinking's GoT schema as baseline
2. **Neo4j Support** → Defer to v2.0, SQLite-only for v1
3. **Langbase Memory Bindings** → Evaluate post-v1.0

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Langbase API changes | Version pipes, pin API version |
| Latency concerns | Aggressive caching, connection pooling |
| State corruption | Transaction isolation, backup on checkpoint |
| Feature drift | Weekly parity checks against unified-thinking |

## Success Criteria

- [ ] MCP clients connect without modification
- [ ] All v1 reasoning modes functional
- [ ] Langbase pipes swappable via config
- [ ] Sub-second response for cached operations
- [ ] Zero data loss on graceful shutdown
- [ ] Clear error messages for all failure modes
