# Development Environment Setup

## Prerequisites

### 1. Rust Toolchain

```powershell
# Install rustup (if not present)
winget install Rustlang.Rustup

# Or via official installer
Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile rustup-init.exe
.\rustup-init.exe

# Verify installation
rustup --version
cargo --version
rustc --version

# Add components
rustup component add rustfmt clippy rust-analyzer
```

### 2. SQLite

```powershell
# Windows - SQLite is bundled with sqlx, but CLI is useful for debugging
winget install SQLite.SQLite

# Verify
sqlite3 --version
```

### 3. Build Tools (Windows)

```powershell
# Visual Studio Build Tools (for native dependencies)
winget install Microsoft.VisualStudio.2022.BuildTools

# Or minimal: Install C++ build tools via VS Installer
# Required components: MSVC v143, Windows SDK
```

### 4. Langbase Account

1. Create account at https://langbase.com
2. Create a new Pipe for testing
3. Copy API key from Settings → API Keys
4. Note your org/pipe identifiers

## Project Initialization

```powershell
cd C:\Development\Projects\MCP\project-root\mcp-servers\mcp-langbase-reasoning

# Initialize Cargo project
cargo init --name mcp-langbase-reasoning

# Create directory structure
$dirs = @(
    "src/server",
    "src/langbase",
    "src/modes",
    "src/storage",
    "src/orchestration",
    "src/config",
    "src/error",
    "migrations",
    "tests/integration",
    "tests/mocks",
    "docs",
    "data"
)

foreach ($dir in $dirs) {
    New-Item -ItemType Directory -Path $dir -Force
}

# Create placeholder files
$files = @(
    "src/server/mod.rs",
    "src/server/mcp.rs",
    "src/server/handlers.rs",
    "src/langbase/mod.rs",
    "src/langbase/client.rs",
    "src/langbase/pipes.rs",
    "src/langbase/types.rs",
    "src/modes/mod.rs",
    "src/modes/linear.rs",
    "src/storage/mod.rs",
    "src/storage/sqlite.rs",
    "src/config/mod.rs",
    "src/error/mod.rs"
)

foreach ($file in $files) {
    New-Item -ItemType File -Path $file -Force
}
```

## Cargo.toml Configuration

```toml
[package]
name = "mcp-langbase-reasoning"
version = "0.1.0"
edition = "2021"
authors = ["Your Name <email@example.com>"]
description = "MCP server delegating reasoning to Langbase Pipes"
license = "MIT"
repository = "https://github.com/youruser/mcp-langbase-reasoning"

[dependencies]
# Async runtime
tokio = { version = "1.40", features = ["full"] }

# HTTP client
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Database
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "migrate"] }

# Configuration
dotenvy = "0.15"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Error handling
thiserror = "1.0"
anyhow = "1.0"

# MCP protocol
# Option A: Official SDK (when available)
# mcp-sdk = "0.1"

# Option B: Manual JSON-RPC
jsonrpc-core = "18.0"

# Utilities
uuid = { version = "1.10", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"

[dev-dependencies]
# Testing
tokio-test = "0.4"
mockall = "0.13"
wiremock = "0.6"
tempfile = "3.12"
pretty_assertions = "1.4"

[profile.release]
lto = true
codegen-units = 1
strip = true
```

## Environment Configuration

```powershell
# Create .env.example
@"
# Langbase Configuration (Required)
LANGBASE_API_KEY=pipe_your_api_key_here
LANGBASE_BASE_URL=https://api.langbase.com

# Database Configuration
DATABASE_PATH=./data/reasoning.db
DATABASE_MAX_CONNECTIONS=5

# Logging
LOG_LEVEL=debug
LOG_FORMAT=pretty

# Request Configuration
REQUEST_TIMEOUT_MS=30000
MAX_RETRIES=3
RETRY_DELAY_MS=1000

# Pipe Overrides (Optional)
# PIPE_LINEAR=linear-reasoning-v1
# PIPE_TREE=tree-reasoning-v1
# PIPE_DIVERGENT=divergent-reasoning-v1
# PIPE_REFLECTION=reflection-v1
# PIPE_AUTO=mode-router-v1
"@ | Out-File -FilePath ".env.example" -Encoding utf8

# Copy to .env for local development
Copy-Item .env.example .env
```

## Git Configuration

```powershell
# Create .gitignore
@"
# Rust
/target/
**/*.rs.bk
Cargo.lock

# Environment
.env
.env.local
.env.*.local

# Database
*.db
*.db-journal
*.db-wal
*.db-shm
/data/*.db

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Logs
*.log
logs/

# Build artifacts
*.exe
*.dll
*.so
*.dylib
"@ | Out-File -FilePath ".gitignore" -Encoding utf8

# Initialize git
git init
git add .
git commit -m "Initial project structure"
```

## VS Code Configuration

```powershell
# Create VS Code settings
New-Item -ItemType Directory -Path ".vscode" -Force

@"
{
    "rust-analyzer.cargo.features": "all",
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.inlayHints.parameterHints.enable": true,
    "rust-analyzer.inlayHints.typeHints.enable": true,
    "[rust]": {
        "editor.formatOnSave": true,
        "editor.defaultFormatter": "rust-lang.rust-analyzer"
    },
    "files.watcherExclude": {
        "**/target/**": true
    }
}
"@ | Out-File -FilePath ".vscode/settings.json" -Encoding utf8

# Launch configuration for debugging
@"
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug MCP Server",
            "cargo": {
                "args": ["build", "--bin=mcp-langbase-reasoning"]
            },
            "args": [],
            "cwd": "`${workspaceFolder}",
            "env": {
                "RUST_BACKTRACE": "1",
                "RUST_LOG": "debug"
            }
        },
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug Tests",
            "cargo": {
                "args": ["test", "--no-run"]
            },
            "cwd": "`${workspaceFolder}",
            "env": {
                "RUST_BACKTRACE": "1"
            }
        }
    ]
}
"@ | Out-File -FilePath ".vscode/launch.json" -Encoding utf8
```

## Initial Database Migration

```powershell
# Create initial migration
@"
-- Migration: 001_initial
-- Description: Core session and thought tables

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY NOT NULL,
    mode TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT
);

CREATE TABLE IF NOT EXISTS thoughts (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    content TEXT NOT NULL,
    confidence REAL DEFAULT 0.8,
    mode TEXT NOT NULL,
    parent_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES thoughts(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_thoughts_session ON thoughts(session_id);
CREATE INDEX IF NOT EXISTS idx_thoughts_parent ON thoughts(parent_id);

-- Invocation log for debugging
CREATE TABLE IF NOT EXISTS invocations (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT,
    tool_name TEXT NOT NULL,
    input TEXT NOT NULL,
    output TEXT,
    pipe_name TEXT,
    latency_ms INTEGER,
    success INTEGER NOT NULL DEFAULT 1,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_invocations_session ON invocations(session_id);
CREATE INDEX IF NOT EXISTS idx_invocations_created ON invocations(created_at);
"@ | Out-File -FilePath "migrations/001_initial.sql" -Encoding utf8
```

## Verification Commands

```powershell
# Verify Rust setup
cargo --version
rustc --version
rustup show

# Build project (will fail until code is written, but verifies deps)
cargo check

# Format code
cargo fmt

# Run clippy
cargo clippy

# Run tests
cargo test

# Build release
cargo build --release
```

## Langbase Pipe Testing

```powershell
# Test Langbase API connectivity (requires curl or Invoke-RestMethod)
$headers = @{
    "Authorization" = "Bearer $env:LANGBASE_API_KEY"
    "Content-Type" = "application/json"
}

$body = @{
    messages = @(
        @{
            role = "user"
            content = "Hello, test message"
        }
    )
} | ConvertTo-Json

# Replace with your pipe endpoint
Invoke-RestMethod -Uri "https://api.langbase.com/v1/pipes/run" `
    -Method Post `
    -Headers $headers `
    -Body $body
```

## Quick Start Script

Save this as `setup.ps1` in the project root:

```powershell
#!/usr/bin/env pwsh
# setup.ps1 - One-time development environment setup

Write-Host "Setting up mcp-langbase-reasoning development environment..." -ForegroundColor Cyan

# Check prerequisites
$prerequisites = @{
    "cargo" = "Rust toolchain (install via rustup)"
    "sqlite3" = "SQLite CLI (optional but recommended)"
}

foreach ($cmd in $prerequisites.Keys) {
    if (!(Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Write-Warning "Missing: $cmd - $($prerequisites[$cmd])"
    } else {
        Write-Host "✓ $cmd found" -ForegroundColor Green
    }
}

# Create directories
Write-Host "Creating directory structure..." -ForegroundColor Yellow
$dirs = @("data", "logs")
foreach ($dir in $dirs) {
    New-Item -ItemType Directory -Path $dir -Force | Out-Null
}

# Setup environment
if (!(Test-Path ".env")) {
    Write-Host "Creating .env from template..." -ForegroundColor Yellow
    Copy-Item .env.example .env
    Write-Warning "Edit .env and add your LANGBASE_API_KEY"
}

# Initialize database
Write-Host "Initializing database..." -ForegroundColor Yellow
if (Get-Command sqlite3 -ErrorAction SilentlyContinue) {
    sqlite3 data/reasoning.db < migrations/001_initial.sql
    Write-Host "✓ Database initialized" -ForegroundColor Green
} else {
    Write-Host "SQLite CLI not found, database will be created on first run" -ForegroundColor Yellow
}

# Build project
Write-Host "Building project..." -ForegroundColor Yellow
cargo build

Write-Host "`nSetup complete!" -ForegroundColor Green
Write-Host "Next steps:"
Write-Host "  1. Edit .env with your LANGBASE_API_KEY"
Write-Host "  2. Run: cargo run"
Write-Host "  3. Test with your MCP client"
```

## Claude Code Task for Project Scaffolding

When Claude Code creates this project, use these commands:

```
Task: Create mcp-langbase-reasoning Rust project

1. Navigate to: C:\Development\Projects\MCP\project-root\mcp-servers\mcp-langbase-reasoning

2. Initialize Cargo project with the Cargo.toml contents above

3. Create all directory structure and placeholder mod.rs files

4. Create .env.example, .gitignore, and migrations

5. Create minimal main.rs that:
   - Loads environment
   - Initializes tracing
   - Prints "MCP Langbase Reasoning Server starting..."
   - Exits cleanly

6. Verify with: cargo build
```
