# API Reference

Complete API reference for mcp-langbase-reasoning MCP server.

## MCP Tools

### reasoning_linear

Single-pass sequential reasoning. Process a thought and get a logical continuation or analysis.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The thought content to process"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID for context continuity"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold (0.0-1.0)"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Reasoning output text",
  "confidence": 0.85,
  "previous_thought": "uuid | null"
}
```

---

### reasoning_tree

Branching exploration with multiple reasoning paths. Explores 2-4 distinct approaches and recommends the most promising one.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The thought content to explore"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "branch_id": {
      "type": "string",
      "description": "Optional branch ID to continue from"
    },
    "max_branches": {
      "type": "integer",
      "minimum": 2,
      "maximum": 10,
      "description": "Maximum branches to explore (default: 4)"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "branch_id": "uuid",
  "content": "Reasoning output",
  "confidence": 0.85,
  "branches_explored": 3,
  "recommended_branch": "uuid"
}
```

---

### reasoning_tree_focus

Focus on a specific branch, making it the active branch for the session.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    },
    "branch_id": {
      "type": "string",
      "description": "Branch ID to focus on"
    }
  },
  "required": ["session_id", "branch_id"]
}
```

---

### reasoning_tree_list

List all branches in a session.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    }
  },
  "required": ["session_id"]
}
```

---

### reasoning_tree_complete

Mark a branch as completed or abandoned.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    },
    "branch_id": {
      "type": "string",
      "description": "Branch ID to complete"
    },
    "state": {
      "type": "string",
      "enum": ["completed", "abandoned"],
      "description": "New state for the branch"
    }
  },
  "required": ["session_id", "branch_id", "state"]
}
```

---

### reasoning_divergent

Creative reasoning that generates novel perspectives and unconventional solutions. Challenges assumptions and synthesizes diverse viewpoints.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "The topic or problem to explore creatively"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "num_perspectives": {
      "type": "integer",
      "minimum": 2,
      "maximum": 10,
      "description": "Number of perspectives to generate (default: 3)"
    },
    "constraints": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Optional constraints to apply"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Synthesized creative output",
  "confidence": 0.85,
  "perspectives": [
    {
      "id": "uuid",
      "viewpoint": "Perspective description",
      "novelty_score": 0.8
    }
  ],
  "synthesis": "Integrated insight from all perspectives"
}
```

---

### reasoning_reflection

Meta-cognitive reasoning that analyzes and improves reasoning quality. Evaluates strengths, weaknesses, and provides recommendations.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "Content to reflect on (if not using thought_id)"
    },
    "thought_id": {
      "type": "string",
      "description": "Specific thought ID to evaluate"
    },
    "session_id": {
      "type": "string",
      "description": "Session ID for context"
    },
    "focus_areas": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Areas to focus reflection on"
    },
    "max_iterations": {
      "type": "integer",
      "minimum": 1,
      "maximum": 5,
      "description": "Maximum reflection iterations (default: 1)"
    }
  }
}
```

#### Response

```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Reflection analysis",
  "confidence": 0.85,
  "strengths": ["..."],
  "weaknesses": ["..."],
  "recommendations": ["..."],
  "improved_reasoning": "Enhanced version of original"
}
```

---

### reasoning_reflection_evaluate

Evaluate a session's overall reasoning quality, coherence, and provide recommendations.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID to evaluate"
    }
  },
  "required": ["session_id"]
}
```

---

### reasoning_auto

Automatically select the most appropriate reasoning mode based on content analysis.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "Content to analyze for mode selection"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "hints": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Optional hints about the problem type"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "recommended_mode": "tree",
  "confidence": 0.85,
  "rationale": "Content requires exploring multiple options",
  "complexity": 0.6,
  "alternative_modes": [
    {
      "mode": "divergent",
      "confidence": 0.5,
      "rationale": "Could also use creative exploration"
    }
  ]
}
```

---

### reasoning_checkpoint_create

Create a checkpoint at the current reasoning state for later backtracking.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    },
    "name": {
      "type": "string",
      "description": "Checkpoint name"
    },
    "description": {
      "type": "string",
      "description": "Optional description"
    }
  },
  "required": ["session_id", "name"]
}
```

---

### reasoning_checkpoint_list

List all checkpoints available for a session.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "session_id": {
      "type": "string",
      "description": "Session ID"
    }
  },
  "required": ["session_id"]
}
```

---

### reasoning_backtrack

Restore from a checkpoint and explore alternative reasoning paths.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "checkpoint_id": {
      "type": "string",
      "description": "Checkpoint ID to restore from"
    },
    "new_direction": {
      "type": "string",
      "description": "Optional new direction to explore"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID verification"
    },
    "confidence": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Confidence threshold"
    }
  },
  "required": ["checkpoint_id"]
}
```

---

### reasoning_got_init

Initialize a new Graph-of-Thoughts reasoning graph with a root node.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "content": {
      "type": "string",
      "description": "Initial thought content for root node"
    },
    "session_id": {
      "type": "string",
      "description": "Optional session ID"
    },
    "problem": {
      "type": "string",
      "description": "Problem description for context"
    },
    "config": {
      "type": "object",
      "properties": {
        "max_depth": { "type": "integer" },
        "max_branches": { "type": "integer" },
        "prune_threshold": { "type": "number" }
      },
      "description": "Graph configuration"
    }
  },
  "required": ["content"]
}
```

#### Response

```json
{
  "graph_id": "uuid",
  "session_id": "uuid",
  "root_node_id": "uuid",
  "content": "Root node content",
  "confidence": 0.8
}
```

---

### reasoning_got_generate

Generate k diverse continuations from a node in the reasoning graph.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_id": {
      "type": "string",
      "description": "Node ID to generate from"
    },
    "k": {
      "type": "integer",
      "minimum": 1,
      "maximum": 10,
      "description": "Number of continuations to generate (default: 3)"
    },
    "problem": {
      "type": "string",
      "description": "Problem context"
    }
  },
  "required": ["graph_id", "node_id"]
}
```

---

### reasoning_got_score

Score a node's quality based on relevance, validity, depth, and novelty.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_id": {
      "type": "string",
      "description": "Node ID to score"
    },
    "problem": {
      "type": "string",
      "description": "Problem context for scoring"
    }
  },
  "required": ["graph_id", "node_id"]
}
```

---

### reasoning_got_aggregate

Merge multiple reasoning nodes into a unified insight.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_ids": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Node IDs to aggregate"
    },
    "problem": {
      "type": "string",
      "description": "Problem context"
    }
  },
  "required": ["graph_id", "node_ids"]
}
```

---

### reasoning_got_refine

Improve a reasoning node through self-critique and refinement.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "node_id": {
      "type": "string",
      "description": "Node ID to refine"
    },
    "problem": {
      "type": "string",
      "description": "Problem context"
    }
  },
  "required": ["graph_id", "node_id"]
}
```

---

### reasoning_got_prune

Remove low-scoring nodes from the reasoning graph.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "threshold": {
      "type": "number",
      "minimum": 0,
      "maximum": 1,
      "description": "Score threshold for pruning (default: 0.3)"
    }
  },
  "required": ["graph_id"]
}
```

---

### reasoning_got_finalize

Mark terminal nodes and retrieve final conclusions from the reasoning graph.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    },
    "terminal_node_ids": {
      "type": "array",
      "items": { "type": "string" },
      "description": "Node IDs to mark as terminal"
    }
  },
  "required": ["graph_id"]
}
```

---

### reasoning_got_state

Get the current state of the reasoning graph including node counts and structure.

#### Input Schema

```json
{
  "type": "object",
  "properties": {
    "graph_id": {
      "type": "string",
      "description": "Graph ID"
    }
  },
  "required": ["graph_id"]
}
```

#### Response

```json
{
  "graph_id": "uuid",
  "session_id": "uuid",
  "node_count": 15,
  "edge_count": 20,
  "max_depth": 4,
  "active_nodes": 5,
  "terminal_nodes": 2,
  "has_cycle": false
}
```

---

## Data Types

### Session

Represents a reasoning context that groups related thoughts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique session identifier (UUID) |
| `mode` | `string` | Reasoning mode (`linear`, `tree`, etc.) |
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `updated_at` | `datetime` | ISO 8601 last update timestamp |
| `metadata` | `object?` | Optional arbitrary metadata |
| `active_branch_id` | `string?` | Currently active branch (tree mode) |

### Thought

Represents a single reasoning step within a session.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique thought identifier (UUID) |
| `session_id` | `string` | Parent session ID |
| `content` | `string` | Reasoning content text |
| `confidence` | `number` | Confidence score (0.0-1.0) |
| `mode` | `string` | Reasoning mode used |
| `parent_id` | `string?` | Parent thought ID (for branching) |
| `branch_id` | `string?` | Branch ID (for tree mode) |
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `metadata` | `object?` | Optional arbitrary metadata |

### Branch

Represents a reasoning branch in tree mode.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique branch identifier (UUID) |
| `session_id` | `string` | Parent session ID |
| `name` | `string?` | Optional branch name |
| `parent_id` | `string?` | Parent branch ID |
| `state` | `string` | `active`, `completed`, or `abandoned` |
| `confidence` | `number` | Branch confidence score |
| `priority` | `integer` | Branch priority |
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `updated_at` | `datetime` | ISO 8601 last update timestamp |

### Checkpoint

Represents a saved reasoning state for backtracking.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique checkpoint identifier (UUID) |
| `session_id` | `string` | Parent session ID |
| `branch_id` | `string?` | Associated branch ID |
| `name` | `string` | Checkpoint name |
| `description` | `string?` | Optional description |
| `snapshot` | `object` | Serialized state data |
| `created_at` | `datetime` | ISO 8601 creation timestamp |

### GraphNode

Represents a node in a Graph-of-Thoughts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique node identifier (UUID) |
| `graph_id` | `string` | Parent graph ID |
| `content` | `string` | Node content |
| `node_type` | `string` | `thought`, `hypothesis`, `conclusion`, `aggregation`, `root`, `refinement`, `terminal` |
| `score` | `number` | Quality score (0.0-1.0) |
| `depth` | `integer` | Depth in graph |
| `is_active` | `boolean` | Whether node is active |
| `is_terminal` | `boolean` | Whether node is terminal |
| `created_at` | `datetime` | ISO 8601 creation timestamp |

### GraphEdge

Represents an edge between nodes in a Graph-of-Thoughts.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique edge identifier (UUID) |
| `graph_id` | `string` | Parent graph ID |
| `from_node` | `string` | Source node ID |
| `to_node` | `string` | Target node ID |
| `edge_type` | `string` | `generates`, `refines`, `aggregates`, `supports`, `contradicts` |
| `weight` | `number` | Edge weight (0.0-1.0) |
| `created_at` | `datetime` | ISO 8601 creation timestamp |

### Invocation

Logs API calls for debugging and auditing.

| Field | Type | Description |
|-------|------|-------------|
| `id` | `string` | Unique invocation identifier |
| `session_id` | `string?` | Associated session ID |
| `tool_name` | `string` | Tool that was invoked |
| `input` | `object` | Input parameters |
| `output` | `object?` | Response data |
| `pipe_name` | `string?` | Langbase pipe used |
| `latency_ms` | `integer?` | Request latency |
| `success` | `boolean` | Whether invocation succeeded |
| `error` | `string?` | Error message if failed |
| `created_at` | `datetime` | ISO 8601 timestamp |

---

## Error Handling

### JSON-RPC Error Codes

| Code | Name | Description |
|------|------|-------------|
| `-32700` | Parse Error | Invalid JSON received |
| `-32601` | Method Not Found | Unknown method |
| `-32602` | Invalid Params | Invalid method parameters |
| `-32603` | Internal Error | Server-side error |

### Application Errors

Errors are returned in the tool result with `isError: true`:

```json
{
  "content": [{
    "type": "text",
    "text": "Error: Validation failed: content - Content cannot be empty"
  }],
  "isError": true
}
```

#### Error Types

| Type | Description |
|------|-------------|
| `Validation` | Input validation failed |
| `SessionNotFound` | Referenced session does not exist |
| `ThoughtNotFound` | Referenced thought does not exist |
| `BranchNotFound` | Referenced branch does not exist |
| `CheckpointNotFound` | Referenced checkpoint does not exist |
| `GraphNotFound` | Referenced graph does not exist |
| `NodeNotFound` | Referenced node does not exist |
| `LangbaseUnavailable` | Langbase API unreachable after retries |
| `ApiError` | Langbase API returned error |
| `Timeout` | Request timed out |

---

## MCP Protocol

### Initialize

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {},
    "clientInfo": {
      "name": "claude-desktop",
      "version": "1.0.0"
    }
  }
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "tools": {
        "listChanged": false
      }
    },
    "serverInfo": {
      "name": "mcp-langbase-reasoning",
      "version": "0.1.0"
    }
  }
}
```

### List Tools

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/list"
}
```

### Call Tool

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "reasoning_linear",
    "arguments": {
      "content": "Your reasoning prompt"
    }
  }
}
```

### Ping

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "ping"
}
```

Response:
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "result": {}
}
```

---

## Langbase Integration

### Pipe Request Format

The server sends requests to Langbase in this format:

```json
{
  "name": "linear-reasoning-v1",
  "messages": [
    {"role": "system", "content": "System prompt..."},
    {"role": "user", "content": "User input"}
  ],
  "stream": false,
  "threadId": "session-uuid"
}
```

### Expected Pipe Response

Langbase pipes should return JSON in this format:

```json
{
  "thought": "Reasoning output text",
  "confidence": 0.85,
  "metadata": {}
}
```

If the pipe returns non-JSON, the entire response is treated as the thought content with default confidence (0.8).

---

## Configuration Reference

### General Settings

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LANGBASE_API_KEY` | Yes | - | Langbase API key |
| `LANGBASE_BASE_URL` | No | `https://api.langbase.com` | API endpoint |
| `DATABASE_PATH` | No | `./data/reasoning.db` | SQLite database path |
| `DATABASE_MAX_CONNECTIONS` | No | `5` | Connection pool size |
| `LOG_LEVEL` | No | `info` | Logging level |
| `LOG_FORMAT` | No | `pretty` | Log format (`pretty`, `json`) |
| `REQUEST_TIMEOUT_MS` | No | `30000` | HTTP timeout (ms) |
| `MAX_RETRIES` | No | `3` | Max retry attempts |
| `RETRY_DELAY_MS` | No | `1000` | Initial retry delay |

### Pipe Names

| Variable | Default | Description |
|----------|---------|-------------|
| `PIPE_LINEAR` | `linear-reasoning-v1` | Linear reasoning pipe |
| `PIPE_TREE` | `tree-reasoning-v1` | Tree reasoning pipe |
| `PIPE_DIVERGENT` | `divergent-reasoning-v1` | Divergent reasoning pipe |
| `PIPE_REFLECTION` | `reflection-v1` | Reflection pipe |
| `PIPE_AUTO` | `mode-router-v1` | Auto mode router pipe |
| `PIPE_BACKTRACKING` | `backtracking-reasoning-v1` | Backtracking pipe |
| `PIPE_GOT_GENERATE` | `got-generate-v1` | GoT generate pipe |
| `PIPE_GOT_SCORE` | `got-score-v1` | GoT score pipe |
| `PIPE_GOT_AGGREGATE` | `got-aggregate-v1` | GoT aggregate pipe |
| `PIPE_GOT_REFINE` | `got-refine-v1` | GoT refine pipe |
