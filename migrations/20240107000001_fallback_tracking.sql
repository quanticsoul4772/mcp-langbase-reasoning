-- Migration to add fallback tracking columns to invocations table
-- Part of FALLBACK_REMOVAL_PLAN Phase 4: Metrics Enhancement

-- Add fallback tracking columns
ALTER TABLE invocations ADD COLUMN fallback_used INTEGER NOT NULL DEFAULT 0;
ALTER TABLE invocations ADD COLUMN fallback_type TEXT;

-- Create index for efficient fallback queries
CREATE INDEX IF NOT EXISTS idx_invocations_fallback ON invocations(fallback_used);
