-- Phase 2 migration: branches and checkpoints for tree mode and state management
-- Creates tables for branching exploration and state snapshots

-- Branches table: stores tree-mode branching structures
CREATE TABLE IF NOT EXISTS branches (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    name TEXT,
    parent_branch_id TEXT,
    priority REAL DEFAULT 1.0,
    confidence REAL DEFAULT 0.8,
    state TEXT DEFAULT 'active',  -- active, completed, abandoned
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    metadata TEXT,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_branch_id) REFERENCES branches(id) ON DELETE SET NULL
);

-- Indexes for branches table
CREATE INDEX IF NOT EXISTS idx_branches_session ON branches(session_id);
CREATE INDEX IF NOT EXISTS idx_branches_parent ON branches(parent_branch_id);
CREATE INDEX IF NOT EXISTS idx_branches_state ON branches(state);

-- Add branch_id column to thoughts for tree mode association
ALTER TABLE thoughts ADD COLUMN branch_id TEXT REFERENCES branches(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_thoughts_branch ON thoughts(branch_id);

-- Checkpoints table: state snapshots for backtracking
CREATE TABLE IF NOT EXISTS checkpoints (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    branch_id TEXT,
    name TEXT NOT NULL,
    description TEXT,
    snapshot TEXT NOT NULL,  -- JSON blob of serialized state
    created_at TEXT NOT NULL,
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (branch_id) REFERENCES branches(id) ON DELETE SET NULL
);

-- Indexes for checkpoints table
CREATE INDEX IF NOT EXISTS idx_checkpoints_session ON checkpoints(session_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_branch ON checkpoints(branch_id);
CREATE INDEX IF NOT EXISTS idx_checkpoints_created ON checkpoints(created_at);

-- Cross-references table: links between branches for tree mode
CREATE TABLE IF NOT EXISTS cross_refs (
    id TEXT PRIMARY KEY NOT NULL,
    from_branch_id TEXT NOT NULL,
    to_branch_id TEXT NOT NULL,
    ref_type TEXT NOT NULL,  -- supports, contradicts, extends, etc.
    reason TEXT,
    strength REAL DEFAULT 1.0,
    created_at TEXT NOT NULL,
    FOREIGN KEY (from_branch_id) REFERENCES branches(id) ON DELETE CASCADE,
    FOREIGN KEY (to_branch_id) REFERENCES branches(id) ON DELETE CASCADE
);

-- Indexes for cross_refs table
CREATE INDEX IF NOT EXISTS idx_crossrefs_from ON cross_refs(from_branch_id);
CREATE INDEX IF NOT EXISTS idx_crossrefs_to ON cross_refs(to_branch_id);

-- Active branch tracking for sessions
ALTER TABLE sessions ADD COLUMN active_branch_id TEXT REFERENCES branches(id) ON DELETE SET NULL;
