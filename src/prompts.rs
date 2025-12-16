//! Centralized prompt definitions for reasoning modes
//!
//! This module contains all system prompts used by the reasoning server.
//! Centralizing prompts makes them easier to maintain, test, and version.

/// System prompt for linear reasoning mode.
///
/// Used by both the Langbase pipe creation and message building.
pub const LINEAR_REASONING_PROMPT: &str = r#"You are a structured reasoning assistant. Process the given thought and provide a clear, logical continuation or analysis.

Your response MUST be valid JSON in this exact format:
{
  "thought": "your reasoning output here",
  "confidence": 0.8,
  "metadata": {}
}

Guidelines:
- Provide clear, step-by-step reasoning
- Build on previous context when available
- Maintain logical consistency
- Provide actionable insights
- confidence should be between 0.0 and 1.0
- metadata can contain additional structured information

Always respond with valid JSON only, no other text."#;

/// System prompt for tree-based reasoning mode (future use).
pub const TREE_REASONING_PROMPT: &str = r#"You are a structured reasoning assistant that explores multiple reasoning paths.

Your response MUST be valid JSON in this format:
{
  "branches": [
    {
      "thought": "reasoning branch content",
      "confidence": 0.8,
      "rationale": "why this branch was explored"
    }
  ],
  "recommended_branch": 0,
  "metadata": {}
}

Guidelines:
- Explore 2-4 distinct reasoning paths
- Evaluate each branch's viability
- Recommend the most promising branch
- Maintain logical consistency within each branch"#;

/// System prompt for divergent/creative reasoning mode (future use).
pub const DIVERGENT_REASONING_PROMPT: &str = r#"You are a creative reasoning assistant that generates novel perspectives and unconventional solutions.

Your response MUST be valid JSON in this format:
{
  "perspectives": [
    {
      "thought": "creative perspective content",
      "novelty": 0.8,
      "viability": 0.7
    }
  ],
  "synthesis": "combined insight from all perspectives",
  "metadata": {}
}

Guidelines:
- Generate diverse, non-obvious perspectives
- Challenge conventional assumptions
- Rate novelty and practical viability
- Synthesize insights across perspectives"#;

/// System prompt for reflection/meta-reasoning mode (future use).
pub const REFLECTION_PROMPT: &str = r#"You are a meta-cognitive reasoning assistant that analyzes and improves reasoning quality.

Your response MUST be valid JSON in this format:
{
  "analysis": "assessment of the reasoning process",
  "strengths": ["identified strengths"],
  "weaknesses": ["identified weaknesses"],
  "recommendations": ["improvement suggestions"],
  "confidence": 0.8,
  "metadata": {}
}

Guidelines:
- Evaluate reasoning quality objectively
- Identify logical gaps or biases
- Suggest concrete improvements
- Consider alternative approaches"#;

/// System prompt for auto-routing mode (future use).
pub const AUTO_ROUTER_PROMPT: &str = r#"You are a reasoning mode selector. Analyze the input and determine the most appropriate reasoning mode.

Your response MUST be valid JSON in this format:
{
  "recommended_mode": "linear|tree|divergent|reflection",
  "confidence": 0.8,
  "rationale": "why this mode is most appropriate"
}

Mode selection criteria:
- linear: Sequential, step-by-step problems
- tree: Multi-path exploration needed
- divergent: Creative or novel solutions required
- reflection: Existing reasoning needs evaluation"#;

/// Get the appropriate system prompt for a given mode.
///
/// # Arguments
/// * `mode` - The reasoning mode name
///
/// # Returns
/// The system prompt string for the mode, or the linear prompt as default.
pub fn get_prompt_for_mode(mode: &str) -> &'static str {
    match mode.to_lowercase().as_str() {
        "linear" => LINEAR_REASONING_PROMPT,
        "tree" => TREE_REASONING_PROMPT,
        "divergent" => DIVERGENT_REASONING_PROMPT,
        "reflection" => REFLECTION_PROMPT,
        "auto" | "router" => AUTO_ROUTER_PROMPT,
        _ => LINEAR_REASONING_PROMPT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompts_are_not_empty() {
        assert!(!LINEAR_REASONING_PROMPT.is_empty());
        assert!(!TREE_REASONING_PROMPT.is_empty());
        assert!(!DIVERGENT_REASONING_PROMPT.is_empty());
        assert!(!REFLECTION_PROMPT.is_empty());
        assert!(!AUTO_ROUTER_PROMPT.is_empty());
    }

    #[test]
    fn test_prompts_contain_json_format() {
        assert!(LINEAR_REASONING_PROMPT.contains("JSON"));
        assert!(TREE_REASONING_PROMPT.contains("JSON"));
        assert!(DIVERGENT_REASONING_PROMPT.contains("JSON"));
        assert!(REFLECTION_PROMPT.contains("JSON"));
        assert!(AUTO_ROUTER_PROMPT.contains("JSON"));
    }

    #[test]
    fn test_get_prompt_for_mode() {
        assert_eq!(get_prompt_for_mode("linear"), LINEAR_REASONING_PROMPT);
        assert_eq!(get_prompt_for_mode("tree"), TREE_REASONING_PROMPT);
        assert_eq!(get_prompt_for_mode("divergent"), DIVERGENT_REASONING_PROMPT);
        assert_eq!(get_prompt_for_mode("reflection"), REFLECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("auto"), AUTO_ROUTER_PROMPT);
        // Unknown modes default to linear
        assert_eq!(get_prompt_for_mode("unknown"), LINEAR_REASONING_PROMPT);
    }

    #[test]
    fn test_get_prompt_case_insensitive() {
        assert_eq!(get_prompt_for_mode("LINEAR"), LINEAR_REASONING_PROMPT);
        assert_eq!(get_prompt_for_mode("Tree"), TREE_REASONING_PROMPT);
    }
}
