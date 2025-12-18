-- Phase 5 migration: Decision Framework and Evidence Assessment support
-- Creates tables for storing decision analysis and evidence assessment results

-- ============================================================================
-- Decision Framework Tables
-- ============================================================================

-- Decisions table: stores multi-criteria decision analysis results
CREATE TABLE IF NOT EXISTS decisions (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    question TEXT NOT NULL,
    options TEXT NOT NULL,              -- JSON array of option strings
    criteria TEXT,                      -- JSON array of criteria with weights
    method TEXT NOT NULL,               -- 'weighted_sum', 'pairwise', 'topsis'
    recommendation TEXT NOT NULL,       -- JSON object with option, score, confidence, rationale
    scores TEXT NOT NULL,               -- JSON array of option scores
    sensitivity_analysis TEXT,          -- JSON object with robustness analysis
    trade_offs TEXT,                    -- JSON array of trade-off descriptions
    constraints_satisfied TEXT,         -- JSON object mapping options to boolean
    created_at TEXT NOT NULL,
    metadata TEXT,                      -- JSON for additional context
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    CHECK (method IN ('weighted_sum', 'pairwise', 'topsis'))
);

CREATE INDEX IF NOT EXISTS idx_decisions_session ON decisions(session_id);
CREATE INDEX IF NOT EXISTS idx_decisions_method ON decisions(method);
CREATE INDEX IF NOT EXISTS idx_decisions_created ON decisions(created_at);

-- Perspective analyses table: stores stakeholder analysis results
CREATE TABLE IF NOT EXISTS perspective_analyses (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    stakeholders TEXT NOT NULL,         -- JSON array of stakeholder analyses
    power_matrix TEXT,                  -- JSON object with quadrant categorizations
    conflicts TEXT,                     -- JSON array of identified conflicts
    alignments TEXT,                    -- JSON array of identified alignments
    synthesis TEXT NOT NULL,            -- JSON object with consensus/contentious areas
    confidence REAL NOT NULL,
    created_at TEXT NOT NULL,
    metadata TEXT,                      -- JSON for additional context
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    CHECK (confidence BETWEEN 0.0 AND 1.0)
);

CREATE INDEX IF NOT EXISTS idx_perspectives_session ON perspective_analyses(session_id);
CREATE INDEX IF NOT EXISTS idx_perspectives_topic ON perspective_analyses(topic);
CREATE INDEX IF NOT EXISTS idx_perspectives_created ON perspective_analyses(created_at);

-- ============================================================================
-- Evidence Assessment Tables
-- ============================================================================

-- Evidence assessments table: stores evidence evaluation results
CREATE TABLE IF NOT EXISTS evidence_assessments (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    claim TEXT NOT NULL,
    evidence TEXT NOT NULL,             -- JSON array of evidence items
    overall_support TEXT NOT NULL,      -- JSON object with level, confidence, explanation
    evidence_analysis TEXT NOT NULL,    -- JSON array of individual evidence analyses
    chain_analysis TEXT,                -- JSON object with inferential chain analysis
    contradictions TEXT,                -- JSON array of detected contradictions
    gaps TEXT,                          -- JSON array of identified evidence gaps
    recommendations TEXT,               -- JSON array of recommendations
    created_at TEXT NOT NULL,
    metadata TEXT,                      -- JSON for additional context
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_evidence_session ON evidence_assessments(session_id);
CREATE INDEX IF NOT EXISTS idx_evidence_claim ON evidence_assessments(claim);
CREATE INDEX IF NOT EXISTS idx_evidence_created ON evidence_assessments(created_at);

-- Probability updates table: stores Bayesian reasoning results
CREATE TABLE IF NOT EXISTS probability_updates (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL,
    hypothesis TEXT NOT NULL,
    prior REAL NOT NULL,                -- Prior probability (0-1)
    posterior REAL NOT NULL,            -- Posterior probability (0-1)
    confidence_lower REAL,              -- Lower bound of confidence interval
    confidence_upper REAL,              -- Upper bound of confidence interval
    confidence_level REAL,              -- Confidence interval level (e.g., 0.95)
    update_steps TEXT NOT NULL,         -- JSON array of Bayesian update steps
    uncertainty_analysis TEXT,          -- JSON object with entropy analysis
    sensitivity TEXT,                   -- JSON object with sensitivity analysis
    interpretation TEXT NOT NULL,       -- JSON object with verbal probability and recommendations
    created_at TEXT NOT NULL,
    metadata TEXT,                      -- JSON for additional context
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE,
    CHECK (prior BETWEEN 0.0 AND 1.0),
    CHECK (posterior BETWEEN 0.0 AND 1.0)
);

CREATE INDEX IF NOT EXISTS idx_probability_session ON probability_updates(session_id);
CREATE INDEX IF NOT EXISTS idx_probability_hypothesis ON probability_updates(hypothesis);
CREATE INDEX IF NOT EXISTS idx_probability_created ON probability_updates(created_at);
