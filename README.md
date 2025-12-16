# mcp-langbase-reasoning

MCP-compliant server providing structured reasoning tools via Langbase Pipes.

## Overview

This server delegates all generative reasoning to versioned Langbase Pipes while maintaining local state, schemas, and orchestration. It provides the same tool interface as unified-thinking but externalizes prompt logic for better versioning, auditing, and experimentation.

## Features

- **MCP Compliant**: Drop-in replacement for existing MCP clients (Claude Desktop, etc.)
- **Langbase Backend**: Versioned prompts, structured JSON outputs, lifecycle management
- **Local State**: SQLite persistence for sessions, thoughts, and invocation logs
- **Async I/O**: Non-blocking stdio communication using Tokio
- **Retry Logic**: Configurable retries with exponential backoff for API calls
- **Structured Errors**: Comprehensive error types with proper JSON-RPC error codes

## Quick Start

### 1. Prerequisites

- Rust 1.70+ (with cargo)
- Langbase account with API key

### 2. Setup

```bash
# Clone and enter directory
git clone <repository-url>
cd mcp-langbase-reasoning

# Create environment file
cp .env.example .env

# Edit .env with your API key
# LANGBASE_API_KEY=your_key_here

# Build
cargo build --release
```

### 3. Configure MCP Client

Add to your Claude Desktop configuration (`claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "langbase-reasoning": {
      "command": "path/to/mcp-langbase-reasoning",
      "args": [],
      "env": {
        "LANGBASE_API_KEY": "your_api_key"
      }
    }
  }
}
```

### 4. Use

The `reasoning.linear` tool is now available in Claude:

```
Use the reasoning.linear tool to analyze the trade-offs
between microservices and monolithic architectures.
```

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `LANGBASE_API_KEY` | **Yes** | - | Your Langbase API key |
| `LANGBASE_BASE_URL` | No | `https://api.langbase.com` | API endpoint |
| `DATABASE_PATH` | No | `./data/reasoning.db` | SQLite database path |
| `DATABASE_MAX_CONNECTIONS` | No | `5` | Connection pool size |
| `LOG_LEVEL` | No | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `LOG_FORMAT` | No | `pretty` | `pretty` or `json` |
| `REQUEST_TIMEOUT_MS` | No | `30000` | HTTP timeout (ms) |
| `MAX_RETRIES` | No | `3` | Max retry attempts |
| `RETRY_DELAY_MS` | No | `1000` | Initial retry delay (ms) |

## MCP Tools

### reasoning.linear

Single-pass sequential reasoning with session continuity.

**Input:**
```json
{
  "content": "Your reasoning prompt (required)",
  "session_id": "Optional UUID for context continuity",
  "confidence": 0.8
}
```

**Output:**
```json
{
  "thought_id": "uuid",
  "session_id": "uuid",
  "content": "Reasoning output...",
  "confidence": 0.85,
  "previous_thought": "uuid or null"
}
```

**Example Usage:**
```
# First call - creates new session
reasoning.linear("Analyze REST vs GraphQL APIs")

# Continue in same session
reasoning.linear("Now consider performance implications", session_id="...")
```

### Planned Tools (v0.2+)

| Tool | Description |
|------|-------------|
| `reasoning.tree` | Branching exploration with multiple paths |
| `reasoning.divergent` | Parallel path generation |
| `reasoning.reflect` | Self-critique and improvement |
| `reasoning.backtrack` | State rollback and re-evaluation |
| `reasoning.auto` | Automatic mode selection |
| `reasoning.got.*` | Graph-of-Thoughts operations |

## Architecture

```
┌─────────────┐     ┌──────────────────────────┐     ┌───────────────┐
│  MCP Client │────▶│ mcp-langbase-reasoning   │────▶│ Langbase Pipe │
│  (Claude)   │◀────│  ┌─────────┐ ┌────────┐  │◀────│               │
└─────────────┘     │  │ SQLite  │ │ Modes  │  │     └───────────────┘
       stdio        │  └─────────┘ └────────┘  │         HTTPS
                    └──────────────────────────┘
```

**Key Components:**
- **MCP Layer**: JSON-RPC 2.0 over async stdio
- **Modes**: Reasoning implementations (linear, tree, etc.)
- **Storage**: SQLite with embedded migrations
- **Langbase Client**: HTTP client with retry logic

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed technical documentation.

## Development

### Build

```bash
cargo build              # Debug build
cargo build --release    # Release build
```

### Test

```bash
cargo test               # Run all tests
cargo test --test storage_test    # Run specific test file
```

### Lint

```bash
cargo fmt --check        # Check formatting
cargo clippy -- -D warnings   # Lint checks
```

### Run Locally

```bash
# With environment variables
LANGBASE_API_KEY=xxx cargo run

# Or with .env file
cargo run
```

### Test MCP Protocol

```bash
# Send initialize request
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | cargo run

# List tools
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' | cargo run

# Call tool
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"reasoning.linear","arguments":{"content":"Test"}}}' | cargo run
```

## Project Structure

```
mcp-langbase-reasoning/
├── src/
│   ├── main.rs          # Entry point
│   ├── lib.rs           # Public API
│   ├── config/          # Configuration loading
│   ├── error/           # Error types
│   ├── langbase/        # Langbase API client
│   ├── modes/           # Reasoning mode implementations
│   ├── prompts.rs       # Centralized system prompts
│   ├── server/          # MCP protocol handling
│   └── storage/         # SQLite persistence
├── tests/               # Integration tests
├── migrations/          # SQLite migrations
├── docs/
│   ├── API_REFERENCE.md # Complete API documentation
│   ├── ARCHITECTURE.md  # Technical architecture
│   └── PRD.md           # Product requirements
├── .env.example         # Environment template
├── Cargo.toml           # Dependencies
└── README.md            # This file
```

## Documentation

| Document | Description |
|----------|-------------|
| [API Reference](docs/API_REFERENCE.md) | Complete tool and type documentation |
| [Architecture](docs/ARCHITECTURE.md) | Technical design and data flow |
| [Implementation Plan](IMPLEMENTATION_PLAN.md) | Development roadmap |
| [Environment Setup](ENVIRONMENT_SETUP.md) | Development setup guide |

## Version History

| Version | Status | Features |
|---------|--------|----------|
| **v0.1** | ✅ Complete | MCP bootstrap, linear mode, SQLite |
| v0.2 | Planned | Tree, divergent, reflection modes |
| v0.3 | Planned | Backtracking, auto, Graph-of-Thoughts |
| v1.0 | Planned | Production ready, full test coverage |

## Troubleshooting

### Common Issues

**"LANGBASE_API_KEY is required"**
- Ensure the environment variable is set or `.env` file exists

**Connection timeout**
- Check network connectivity to api.langbase.com
- Increase `REQUEST_TIMEOUT_MS` if needed

**Database locked**
- Ensure only one instance is running
- Check `DATABASE_PATH` permissions

**Tool not found**
- Verify MCP client configuration
- Check server logs for initialization errors

### Debug Logging

```bash
LOG_LEVEL=debug cargo run
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make changes with tests
4. Run `cargo fmt` and `cargo clippy`
5. Submit a pull request

## License

MIT
