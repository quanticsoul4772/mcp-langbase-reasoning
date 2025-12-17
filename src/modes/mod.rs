//! Reasoning mode implementations.
//!
//! This module provides different reasoning modes:
//! - [`LinearMode`]: Sequential step-by-step reasoning
//! - [`TreeMode`]: Branching exploration with multiple paths
//! - [`DivergentMode`]: Creative exploration with multiple perspectives
//! - [`ReflectionMode`]: Meta-cognitive analysis
//! - [`BacktrackingMode`]: Checkpoint-based state restoration
//! - [`AutoMode`]: Intelligent mode selection
//! - [`GotMode`]: Graph-of-Thoughts reasoning

mod auto;
mod backtracking;
mod divergent;
mod got;
mod linear;
mod reflection;
mod tree;

pub use auto::*;
pub use backtracking::*;
pub use divergent::*;
pub use got::*;
pub use linear::*;
pub use reflection::*;
pub use tree::*;

use serde::{Deserialize, Serialize};
use tracing::warn;

// ============================================================================
// Shared Utilities
// ============================================================================

/// Serialize a value to JSON for logging, with warning on failure.
///
/// This helper is used across all reasoning modes for invocation logging.
/// Instead of panicking or silently failing on serialization errors,
/// it logs a warning and returns an error object.
pub(crate) fn serialize_for_log<T: serde::Serialize>(value: &T, context: &str) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|e| {
        warn!(
            error = %e,
            context = %context,
            "Failed to serialize value for invocation log"
        );
        serde_json::json!({
            "serialization_error": e.to_string(),
            "context": context
        })
    })
}

/// Extract JSON from a completion string, handling markdown code blocks.
///
/// Attempts extraction in this order:
/// 1. Try parsing as raw JSON first (fast path)
/// 2. Extract from ```json ... ``` code blocks
/// 3. Extract from ``` ... ``` code blocks
/// 4. Return error if none work
///
/// This helper is used by modes that parse structured responses from Langbase.
pub(crate) fn extract_json_from_completion(completion: &str) -> Result<&str, String> {
    // Fast path: raw JSON
    let trimmed = completion.trim();
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return Ok(trimmed);
    }

    // Try ```json ... ``` blocks
    if completion.contains("```json") {
        return completion
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Found ```json block but content was empty or malformed".to_string());
    }

    // Try ``` ... ``` blocks
    if completion.contains("```") {
        return completion
            .split("```")
            .nth(1)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Found ``` block but content was empty or malformed".to_string());
    }

    Err(format!(
        "No JSON found in response. First 100 chars: '{}'",
        completion.chars().take(100).collect::<String>()
    ))
}

/// Reasoning mode types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningMode {
    /// Sequential step-by-step reasoning.
    Linear,
    /// Branching exploration with multiple paths.
    Tree,
    /// Creative exploration with multiple perspectives.
    Divergent,
    /// Meta-cognitive analysis and quality improvement.
    Reflection,
    /// Checkpoint-based state restoration.
    Backtracking,
    /// Automatic mode selection based on content.
    Auto,
    /// Graph-of-Thoughts reasoning.
    Got,
}

impl ReasoningMode {
    /// Get the mode name as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            ReasoningMode::Linear => "linear",
            ReasoningMode::Tree => "tree",
            ReasoningMode::Divergent => "divergent",
            ReasoningMode::Reflection => "reflection",
            ReasoningMode::Backtracking => "backtracking",
            ReasoningMode::Auto => "auto",
            ReasoningMode::Got => "got",
        }
    }
}

impl std::fmt::Display for ReasoningMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ReasoningMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "linear" => Ok(ReasoningMode::Linear),
            "tree" => Ok(ReasoningMode::Tree),
            "divergent" => Ok(ReasoningMode::Divergent),
            "reflection" => Ok(ReasoningMode::Reflection),
            "backtracking" => Ok(ReasoningMode::Backtracking),
            "auto" => Ok(ReasoningMode::Auto),
            "got" => Ok(ReasoningMode::Got),
            _ => Err(format!("Unknown reasoning mode: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reasoning_mode_as_str() {
        assert_eq!(ReasoningMode::Linear.as_str(), "linear");
        assert_eq!(ReasoningMode::Tree.as_str(), "tree");
        assert_eq!(ReasoningMode::Divergent.as_str(), "divergent");
        assert_eq!(ReasoningMode::Reflection.as_str(), "reflection");
        assert_eq!(ReasoningMode::Backtracking.as_str(), "backtracking");
        assert_eq!(ReasoningMode::Auto.as_str(), "auto");
        assert_eq!(ReasoningMode::Got.as_str(), "got");
    }

    #[test]
    fn test_reasoning_mode_display() {
        assert_eq!(format!("{}", ReasoningMode::Linear), "linear");
        assert_eq!(format!("{}", ReasoningMode::Tree), "tree");
        assert_eq!(format!("{}", ReasoningMode::Divergent), "divergent");
        assert_eq!(format!("{}", ReasoningMode::Reflection), "reflection");
        assert_eq!(format!("{}", ReasoningMode::Backtracking), "backtracking");
        assert_eq!(format!("{}", ReasoningMode::Auto), "auto");
        assert_eq!(format!("{}", ReasoningMode::Got), "got");
    }

    #[test]
    fn test_reasoning_mode_from_str_valid() {
        assert_eq!(
            "linear".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Linear
        );
        assert_eq!(
            "tree".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Tree
        );
        assert_eq!(
            "divergent".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Divergent
        );
        assert_eq!(
            "reflection".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Reflection
        );
        assert_eq!(
            "backtracking".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Backtracking
        );
        assert_eq!(
            "auto".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Auto
        );
        assert_eq!("got".parse::<ReasoningMode>().unwrap(), ReasoningMode::Got);
    }

    #[test]
    fn test_reasoning_mode_from_str_case_insensitive() {
        assert_eq!(
            "LINEAR".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Linear
        );
        assert_eq!(
            "Tree".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Tree
        );
        assert_eq!(
            "DIVERGENT".parse::<ReasoningMode>().unwrap(),
            ReasoningMode::Divergent
        );
    }

    #[test]
    fn test_reasoning_mode_from_str_invalid() {
        let result = "invalid".parse::<ReasoningMode>();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Unknown reasoning mode: invalid");
    }

    #[test]
    fn test_reasoning_mode_equality() {
        assert_eq!(ReasoningMode::Linear, ReasoningMode::Linear);
        assert_ne!(ReasoningMode::Linear, ReasoningMode::Tree);
    }

    #[test]
    fn test_reasoning_mode_clone() {
        let mode = ReasoningMode::Divergent;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_reasoning_mode_copy() {
        let mode = ReasoningMode::Auto;
        let copied = mode; // Copy, not move
        assert_eq!(mode, copied);
    }
}
