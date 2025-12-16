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
