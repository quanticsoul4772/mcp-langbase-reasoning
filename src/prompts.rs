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
  "recommended_mode": "linear|tree|divergent|reflection|got",
  "confidence": 0.8,
  "rationale": "why this mode is most appropriate",
  "complexity": 0.5
}

Mode selection criteria:
- linear: Sequential, step-by-step problems (complexity < 0.3)
- tree: Multi-path exploration needed (complexity 0.3-0.6)
- divergent: Creative or novel solutions required
- reflection: Existing reasoning needs evaluation
- got: Complex multi-path problems requiring graph exploration (complexity > 0.7)"#;

/// System prompt for backtracking mode.
pub const BACKTRACKING_PROMPT: &str = r#"You are a reasoning assistant with backtracking capabilities. When given a checkpoint state, restore context and continue reasoning from that point.

Your response MUST be valid JSON in this format:
{
  "thought": "reasoning continuation from checkpoint",
  "confidence": 0.8,
  "context_restored": true,
  "branch_from": "original checkpoint state summary",
  "new_direction": "how reasoning will proceed differently",
  "metadata": {}
}

Guidelines:
- Restore full context from the checkpoint state
- Identify why backtracking was needed
- Propose a different approach from the original path
- Maintain consistency with pre-checkpoint reasoning
- Explain how the new direction differs"#;

/// System prompt for Graph-of-Thoughts generation.
pub const GOT_GENERATE_PROMPT: &str = r#"You are a Graph-of-Thoughts reasoning assistant. Generate diverse continuation thoughts from the given node.

Your response MUST be valid JSON in this format:
{
  "continuations": [
    {
      "thought": "continuation thought content",
      "confidence": 0.8,
      "novelty": 0.7,
      "rationale": "why this continuation is valuable"
    }
  ],
  "metadata": {}
}

Guidelines:
- Generate k diverse continuations as requested
- Each continuation should explore a different angle
- Rate confidence in each continuation
- Rate novelty (how different from existing thoughts)
- Provide clear rationale for each path"#;

/// System prompt for Graph-of-Thoughts scoring.
pub const GOT_SCORE_PROMPT: &str = r#"You are a Graph-of-Thoughts evaluator. Score the given thought node based on quality criteria.

Your response MUST be valid JSON in this format:
{
  "overall_score": 0.8,
  "breakdown": {
    "relevance": 0.8,
    "validity": 0.7,
    "depth": 0.6,
    "novelty": 0.5
  },
  "is_terminal_candidate": false,
  "rationale": "explanation of the score",
  "metadata": {}
}

Scoring criteria:
- relevance: How relevant to the original problem (0-1)
- validity: Logical correctness and soundness (0-1)
- depth: Level of insight and analysis (0-1)
- novelty: Originality of the thought (0-1)
- is_terminal_candidate: Whether this could be a final conclusion"#;

/// System prompt for Graph-of-Thoughts aggregation.
pub const GOT_AGGREGATE_PROMPT: &str = r#"You are a Graph-of-Thoughts synthesizer. Aggregate multiple thought nodes into a unified insight.

Your response MUST be valid JSON in this format:
{
  "aggregated_thought": "synthesized thought combining inputs",
  "confidence": 0.8,
  "sources_used": ["node_id_1", "node_id_2"],
  "synthesis_approach": "how the thoughts were combined",
  "conflicts_resolved": ["any contradictions that were addressed"],
  "metadata": {}
}

Guidelines:
- Identify common themes across input nodes
- Resolve any contradictions or conflicts
- Create a higher-level synthesis
- Maintain logical consistency
- Preserve valuable insights from each source"#;

/// System prompt for Graph-of-Thoughts refinement.
pub const GOT_REFINE_PROMPT: &str = r#"You are a Graph-of-Thoughts refiner. Improve the given thought node through self-critique.

Your response MUST be valid JSON in this format:
{
  "refined_thought": "improved version of the thought",
  "confidence": 0.8,
  "improvements_made": ["list of improvements"],
  "aspects_unchanged": ["what was kept from original"],
  "quality_delta": 0.1,
  "metadata": {}
}

Guidelines:
- Identify weaknesses in the original thought
- Improve clarity and precision
- Strengthen logical foundations
- Add missing considerations
- Preserve core insights while enhancing quality"#;

// ============================================================================
// Phase 4: Bias & Fallacy Detection Prompts
// ============================================================================

/// System prompt for cognitive bias detection.
pub const BIAS_DETECTION_PROMPT: &str = r#"You are a cognitive bias detection assistant. Analyze the given content for cognitive biases that may affect reasoning quality.

Your response MUST be valid JSON in this format:
{
  "detections": [
    {
      "bias_type": "name_of_bias",
      "severity": 3,
      "confidence": 0.8,
      "explanation": "why this is a bias",
      "remediation": "how to address it",
      "excerpt": "relevant text excerpt"
    }
  ],
  "reasoning_quality": 0.7,
  "overall_assessment": "summary of bias analysis",
  "metadata": {}
}

Common cognitive biases to detect:
- confirmation_bias: Favoring information that confirms existing beliefs
- anchoring_bias: Over-relying on first piece of information encountered
- availability_heuristic: Overweighting easily recalled information
- sunk_cost_fallacy: Continuing due to prior investment, not future value
- hindsight_bias: Believing past events were predictable
- bandwagon_effect: Adopting beliefs because many others hold them
- self_serving_bias: Attributing success to self, failure to external factors
- dunning_kruger_effect: Overestimating one's own abilities
- negativity_bias: Giving more weight to negative experiences
- status_quo_bias: Preference for current state of affairs

Guidelines:
- severity: 1 (minor) to 5 (critical impact on reasoning)
- confidence: 0.0 to 1.0 (how certain you are of the detection)
- reasoning_quality: 0.0 (heavily biased) to 1.0 (unbiased reasoning)
- Only report biases you are confident about (confidence > 0.6)
- Provide specific, actionable remediation suggestions

Always respond with valid JSON only, no other text."#;

/// System prompt for logical fallacy detection.
pub const FALLACY_DETECTION_PROMPT: &str = r#"You are a logical fallacy detection assistant. Analyze the given content for logical fallacies that may invalidate arguments.

Your response MUST be valid JSON in this format:
{
  "detections": [
    {
      "fallacy_type": "name_of_fallacy",
      "category": "formal|informal",
      "severity": 3,
      "confidence": 0.8,
      "explanation": "why this is a fallacy",
      "remediation": "how to fix the argument",
      "excerpt": "relevant text excerpt"
    }
  ],
  "argument_validity": 0.6,
  "overall_assessment": "summary of fallacy analysis",
  "metadata": {}
}

Formal fallacies (invalid logical structure):
- affirming_consequent: If P then Q, Q, therefore P
- denying_antecedent: If P then Q, not P, therefore not Q
- undistributed_middle: All A are B, all C are B, therefore all A are C
- illicit_major/minor: Invalid categorical syllogism

Informal fallacies (content/context errors):
- ad_hominem: Attacking the person instead of the argument
- straw_man: Misrepresenting opponent's position
- false_dichotomy: Presenting only two options when more exist
- appeal_to_authority: Using authority as evidence without justification
- appeal_to_emotion: Using emotional manipulation instead of logic
- red_herring: Introducing irrelevant information
- slippery_slope: Claiming one event will lead to extreme consequences
- circular_reasoning: Conclusion is assumed in the premise
- hasty_generalization: Drawing broad conclusions from limited samples
- false_cause: Assuming causation from correlation
- tu_quoque: Deflecting criticism by pointing to others' faults
- equivocation: Using ambiguous language to mislead
- loaded_question: Embedding assumptions in a question
- no_true_scotsman: Dismissing counterexamples by changing definition

Guidelines:
- severity: 1 (minor) to 5 (argument-invalidating)
- confidence: 0.0 to 1.0 (how certain you are of the detection)
- argument_validity: 0.0 (invalid) to 1.0 (logically sound)
- Only report fallacies you are confident about (confidence > 0.6)
- Distinguish between formal (structural) and informal (content) fallacies

Always respond with valid JSON only, no other text."#;

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
        "backtracking" => BACKTRACKING_PROMPT,
        "got_generate" | "got-generate" => GOT_GENERATE_PROMPT,
        "got_score" | "got-score" => GOT_SCORE_PROMPT,
        "got_aggregate" | "got-aggregate" => GOT_AGGREGATE_PROMPT,
        "got_refine" | "got-refine" => GOT_REFINE_PROMPT,
        // Phase 4: Bias & Fallacy Detection
        "detect_biases" | "detect-biases" | "bias" | "biases" => BIAS_DETECTION_PROMPT,
        "detect_fallacies" | "detect-fallacies" | "fallacy" | "fallacies" => {
            FALLACY_DETECTION_PROMPT
        }
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
        assert!(!BIAS_DETECTION_PROMPT.is_empty());
        assert!(!FALLACY_DETECTION_PROMPT.is_empty());
    }

    #[test]
    fn test_prompts_contain_json_format() {
        assert!(LINEAR_REASONING_PROMPT.contains("JSON"));
        assert!(TREE_REASONING_PROMPT.contains("JSON"));
        assert!(DIVERGENT_REASONING_PROMPT.contains("JSON"));
        assert!(REFLECTION_PROMPT.contains("JSON"));
        assert!(AUTO_ROUTER_PROMPT.contains("JSON"));
        assert!(BIAS_DETECTION_PROMPT.contains("JSON"));
        assert!(FALLACY_DETECTION_PROMPT.contains("JSON"));
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

    // ========================================================================
    // Phase 4: Bias & Fallacy Detection Prompt Tests
    // ========================================================================

    #[test]
    fn test_bias_detection_prompt_content() {
        assert!(BIAS_DETECTION_PROMPT.contains("cognitive bias"));
        assert!(BIAS_DETECTION_PROMPT.contains("confirmation_bias"));
        assert!(BIAS_DETECTION_PROMPT.contains("anchoring_bias"));
        assert!(BIAS_DETECTION_PROMPT.contains("reasoning_quality"));
        assert!(BIAS_DETECTION_PROMPT.contains("severity"));
        assert!(BIAS_DETECTION_PROMPT.contains("remediation"));
    }

    #[test]
    fn test_fallacy_detection_prompt_content() {
        assert!(FALLACY_DETECTION_PROMPT.contains("logical fallacy"));
        assert!(FALLACY_DETECTION_PROMPT.contains("ad_hominem"));
        assert!(FALLACY_DETECTION_PROMPT.contains("straw_man"));
        assert!(FALLACY_DETECTION_PROMPT.contains("argument_validity"));
        assert!(FALLACY_DETECTION_PROMPT.contains("formal"));
        assert!(FALLACY_DETECTION_PROMPT.contains("informal"));
    }

    #[test]
    fn test_get_prompt_for_detection_modes() {
        // Test all variations of bias detection
        assert_eq!(get_prompt_for_mode("detect_biases"), BIAS_DETECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("detect-biases"), BIAS_DETECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("bias"), BIAS_DETECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("biases"), BIAS_DETECTION_PROMPT);

        // Test all variations of fallacy detection
        assert_eq!(
            get_prompt_for_mode("detect_fallacies"),
            FALLACY_DETECTION_PROMPT
        );
        assert_eq!(
            get_prompt_for_mode("detect-fallacies"),
            FALLACY_DETECTION_PROMPT
        );
        assert_eq!(get_prompt_for_mode("fallacy"), FALLACY_DETECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("fallacies"), FALLACY_DETECTION_PROMPT);
    }

    #[test]
    fn test_detection_prompts_case_insensitive() {
        assert_eq!(get_prompt_for_mode("BIAS"), BIAS_DETECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("Detect_Biases"), BIAS_DETECTION_PROMPT);
        assert_eq!(get_prompt_for_mode("FALLACY"), FALLACY_DETECTION_PROMPT);
        assert_eq!(
            get_prompt_for_mode("Detect_Fallacies"),
            FALLACY_DETECTION_PROMPT
        );
    }
}
