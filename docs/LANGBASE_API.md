# Langbase API Usage Guide

Quick reference for working with Langbase Pipes API.

## Authentication

All requests require an API key in the Authorization header:
```
Authorization: Bearer {LANGBASE_API_KEY}
```

API keys are stored in `.env`:
```
LANGBASE_API_KEY=user_xxx...
LANGBASE_BASE_URL=https://api.langbase.com
```

## API Endpoints

### List Pipes
```bash
GET /v1/pipes

curl -s "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY"
```

Returns array of pipe objects with `name`, `owner_login`, `description`, `model`, `status`, etc.

### Create/Update Pipe
```bash
POST /v1/pipes

curl -X POST "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-pipe-name",
    "description": "Pipe description",
    "model": "openai:gpt-4o-mini",
    "upsert": true,
    "json": true,
    "stream": true,
    "store": true,
    "temperature": 0.7,
    "max_tokens": 2000,
    "messages": [
      {"role": "system", "content": "System prompt here"}
    ]
  }'
```

**Key parameters:**
- `name`: Pipe identifier (required)
- `model`: LLM to use (e.g., `openai:gpt-4o-mini`, `openai:gpt-4o`)
- `upsert`: If true, updates existing pipe with same name
- `json`: Enable JSON output mode
- `stream`: Enable streaming responses
- `store`: Store conversation history
- `temperature`: 0.0-1.0 (lower = deterministic, higher = creative)
- `max_tokens`: Maximum response length
- `messages`: System prompt and initial messages

### Run Pipe
```bash
POST /v1/pipes/run

curl -X POST "https://api.langbase.com/v1/pipes/run" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "my-pipe-name",
    "stream": false,
    "messages": [
      {"role": "user", "content": "Your input here"}
    ]
  }'
```

**Response:**
```json
{
  "success": true,
  "completion": "LLM response text",
  "threadId": "thread-xxx",
  "raw": {
    "model": "gpt-4o-mini",
    "usage": {
      "prompt_tokens": 100,
      "completion_tokens": 50,
      "total_tokens": 150
    }
  }
}
```

### Delete Pipe (Beta)
```bash
DELETE /beta/pipes/{owner_login}/{pipe_name}

curl -X DELETE "https://api.langbase.com/beta/pipes/rbsmith402733/my-pipe-name" \
  -H "Authorization: Bearer $LANGBASE_API_KEY"
```

**Note:** Delete is only available via the beta endpoint and requires the owner login.

## Rust Client Usage

### Creating a Pipe
```rust
use crate::langbase::{LangbaseClient, CreatePipeRequest, Message};

let request = CreatePipeRequest::new("pipe-name")
    .with_description("Description")
    .with_model("openai:gpt-4o-mini")
    .with_upsert(true)
    .with_json_output(true)
    .with_temperature(0.7)
    .with_max_tokens(2000)
    .with_messages(vec![Message::system("System prompt")]);

client.create_pipe(request).await?;
```

### Running a Pipe
```rust
use crate::langbase::{LangbaseClient, PipeRequest, Message};

let request = PipeRequest::new("pipe-name", vec![
    Message::system("System prompt"),
    Message::user("User input"),
]);

let response = client.call_pipe(request).await?;
println!("Response: {}", response.completion);
```

### Deleting a Pipe
```rust
client.delete_pipe("owner_login", "pipe-name").await?;
```

## Current Pipes

| Pipe Name | Purpose | Temperature | Max Tokens |
|-----------|---------|-------------|------------|
| `linear-reasoning-v1` | Sequential step-by-step reasoning | 0.7 | 2000 |
| `tree-reasoning-v1` | Branching exploration (2-4 paths) | 0.8 | 3000 |
| `divergent-reasoning-v1` | Creative perspectives | 0.9 | 3000 |
| `reflection-v1` | Meta-cognitive analysis | 0.6 | 2500 |

## Model Selection

Available models (prefix with provider):
- `openai:gpt-4o-mini` - Fast, cost-effective (default)
- `openai:gpt-4o` - Most capable
- `openai:gpt-3.5-turbo` - Legacy, fast
- `anthropic:claude-3-sonnet` - Alternative provider

## Configuration Tips

**Temperature by mode:**
- Linear (0.6-0.7): Consistent, logical
- Tree (0.7-0.8): Moderate exploration
- Divergent (0.8-0.9): Maximum creativity
- Reflection (0.5-0.6): Precise analysis

**JSON mode:** Always enable for structured responses. The pipe will return valid JSON matching your schema.

**Streaming:** Enable for real-time responses, disable for simpler synchronous handling.

## Error Handling

Common errors:
- `401`: Invalid API key
- `404`: Pipe not found
- `409`: Pipe already exists (without upsert)
- `429`: Rate limit exceeded

## Finding Your Owner Login

Your `owner_login` is in the response when listing pipes:
```bash
curl -s "https://api.langbase.com/v1/pipes" \
  -H "Authorization: Bearer $LANGBASE_API_KEY" | grep owner_login
```
