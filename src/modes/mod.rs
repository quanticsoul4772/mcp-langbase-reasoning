mod linear;

pub use linear::*;

use serde::{Deserialize, Serialize};

/// Reasoning mode types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningMode {
    Linear,
    Tree,
    Divergent,
    Reflection,
    Backtracking,
    Auto,
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
