# API Reference

Complete API reference for mcp-langbase-reasoning MCP server.

## MCP Tools

### reasoning.linear

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

#### Example

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "reasoning.linear",
    "arguments": {
      "content": "Analyze the trade-offs between SQL and NoSQL databases",
      "confidence": 0.8
    }
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{
      "type": "text",
      "text": "{\"thought_id\":\"...\",\"session_id\":\"...\",\"content\":\"...\",\"confidence\":0.85}"
    }]
  }
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
| `created_at` | `datetime` | ISO 8601 creation timestamp |
| `metadata` | `object?` | Optional arbitrary metadata |

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
| `SessionNotFound` | Referenced session doesn't exist |
| `ThoughtNotFound` | Referenced thought doesn't exist |
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

**Response:**
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

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "tools": [
      {
        "name": "reasoning.linear",
        "description": "Single-pass sequential reasoning...",
        "inputSchema": { ... }
      }
    ]
  }
}
```

### Call Tool

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "reasoning.linear",
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

**Response:**
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

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LANGBASE_API_KEY` | Yes | - | Langbase API key |
| `LANGBASE_BASE_URL` | No | `https://api.langbase.com` | API endpoint |
| `DATABASE_PATH` | No | `./data/reasoning.db` | SQLite database path |
| `DATABASE_MAX_CONNECTIONS` | No | `5` | Connection pool size |
| `LOG_LEVEL` | No | `info` | Logging level (`trace`, `debug`, `info`, `warn`, `error`) |
| `LOG_FORMAT` | No | `pretty` | Log format (`pretty`, `json`) |
| `REQUEST_TIMEOUT_MS` | No | `30000` | HTTP timeout (ms) |
| `MAX_RETRIES` | No | `3` | Max retry attempts |
| `RETRY_DELAY_MS` | No | `1000` | Initial retry delay |
| `PIPE_LINEAR` | No | `linear-reasoning-v1` | Linear reasoning pipe name |
