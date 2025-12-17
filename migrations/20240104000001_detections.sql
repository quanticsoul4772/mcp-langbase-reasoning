-- Phase 4 migration: Bias and Fallacy Detection support
-- Creates tables for storing detection results from cognitive analysis

-- Detections table: stores bias and fallacy detection results
CREATE TABLE IF NOT EXISTS detections (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT,
    thought_id TEXT,
    detection_type TEXT NOT NULL,  -- 'bias' or 'fallacy'
    detected_issue TEXT NOT NULL,  -- specific bias/fallacy name (e.g., 'confirmation_bias', 'ad_hominem')
    severity INTEGER NOT NULL,     -- 1-5 scale (1=minor, 5=critical)
    confidence REAL NOT NULL,      -- 0.0-1.0 detection confidence
    explanation TEXT NOT NULL,     -- why this is a bias/fallacy
    remediation TEXT,              -- how to correct the reasoning
    created_at TEXT NOT NULL,
    metadata TEXT,                 -- JSON for additional context
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (thought_id) REFERENCES thoughts(id) ON DELETE SET NULL,
    CHECK (detection_type IN ('bias', 'fallacy')),
    CHECK (severity BETWEEN 1 AND 5),
    CHECK (confidence BETWEEN 0.0 AND 1.0)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_detections_session ON detections(session_id);
CREATE INDEX IF NOT EXISTS idx_detections_thought ON detections(thought_id);
CREATE INDEX IF NOT EXISTS idx_detections_type ON detections(detection_type);
CREATE INDEX IF NOT EXISTS idx_detections_severity ON detections(severity);
CREATE INDEX IF NOT EXISTS idx_detections_issue ON detections(detected_issue);
