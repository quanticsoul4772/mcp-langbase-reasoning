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
// Phase 5: Decision Framework Prompts
// ============================================================================

/// System prompt for multi-criteria decision making.
pub const DECISION_MAKER_PROMPT: &str = r#"You are a structured decision analysis assistant. Evaluate options using multi-criteria decision analysis.

Your response MUST be valid JSON in this format:
{
  "recommendation": {
    "option": "best option",
    "score": 0.85,
    "confidence": 0.82,
    "rationale": "why this is the best choice"
  },
  "scores": [
    {
      "option": "option name",
      "total_score": 0.85,
      "criteria_scores": {
        "criterion_name": {
          "score": 0.9,
          "reasoning": "justification"
        }
      },
      "rank": 1
    }
  ],
  "sensitivity_analysis": {
    "robust": true,
    "critical_criteria": ["most impactful criteria"],
    "threshold_changes": {"criterion": 0.15}
  },
  "trade_offs": [
    {
      "between": ["option_a", "option_b"],
      "trade_off": "description of trade-off"
    }
  ],
  "constraints_satisfied": {"option": true}
}

Guidelines:
- Score each option against each criterion (0.0-1.0)
- Apply weights to calculate total scores
- Identify trade-offs between top options
- Perform sensitivity analysis on weights
- Check constraint satisfaction
- Provide clear rationale

Always respond with valid JSON only."#;

/// System prompt for stakeholder perspective analysis.
pub const PERSPECTIVE_ANALYZER_PROMPT: &str = r#"You are a stakeholder analysis assistant. Analyze perspectives, power dynamics, and alignment.

Your response MUST be valid JSON in this format:
{
  "stakeholders": [
    {
      "name": "stakeholder name",
      "role": "their role",
      "perspective": "their viewpoint on the topic",
      "interests": ["what they care about"],
      "concerns": ["what worries them"],
      "power_level": 0.8,
      "interest_level": 0.9,
      "quadrant": "key_player",
      "engagement_strategy": "recommended approach"
    }
  ],
  "power_matrix": {
    "key_players": ["names"],
    "keep_satisfied": ["names"],
    "keep_informed": ["names"],
    "minimal_effort": ["names"]
  },
  "conflicts": [
    {
      "stakeholders": ["name1", "name2"],
      "issue": "what causes conflict",
      "severity": 0.7,
      "resolution_approach": "how to resolve"
    }
  ],
  "alignments": [
    {
      "stakeholders": ["name1", "name2"],
      "shared_interest": "common ground"
    }
  ],
  "synthesis": {
    "consensus_areas": ["where stakeholders agree"],
    "contentious_areas": ["where they disagree"],
    "recommendation": "overall strategic recommendation"
  },
  "confidence": 0.82
}

Quadrant values: "key_player", "keep_satisfied", "keep_informed", "minimal_effort"

Guidelines:
- Infer stakeholders if not provided
- Assign power/interest levels objectively
- Categorize into power/interest quadrants
- Identify conflicts and their severity
- Find alignment opportunities
- Provide actionable engagement strategies

Always respond with valid JSON only."#;

// ============================================================================
// Phase 5: Evidence Assessment Prompts
// ============================================================================

/// System prompt for evidence assessment.
pub const EVIDENCE_ASSESSOR_PROMPT: &str = r#"You are an evidence assessment assistant. Evaluate evidence for relevance, credibility, and support for claims.

Your response MUST be valid JSON in this format:
{
  "overall_support": {
    "level": "strong",
    "confidence": 0.75,
    "explanation": "why this level"
  },
  "evidence_analysis": [
    {
      "evidence_id": "e1",
      "content_summary": "summary",
      "relevance": {
        "score": 0.85,
        "relevance_type": "direct",
        "explanation": "why relevant"
      },
      "credibility": {
        "score": 0.80,
        "factors": {
          "source_reliability": 0.9,
          "methodology": 0.7,
          "recency": 0.8,
          "corroboration": 0.75
        },
        "concerns": ["concerns"]
      },
      "weight": 0.68,
      "supports_claim": true,
      "inferential_distance": 2
    }
  ],
  "chain_analysis": {
    "primary_chain": ["e1", "inference", "claim"],
    "weak_links": [{"from": "a", "to": "b", "weakness": "desc", "impact": 0.3}],
    "redundancy": ["e2", "e3"],
    "synergies": [{"evidence_ids": ["e1", "e2"], "combined_strength": 0.9, "explanation": "why"}]
  },
  "contradictions": [
    {"evidence_a": "e1", "evidence_b": "e2", "nature": "desc", "resolution": "approach"}
  ],
  "gaps": [
    {"gap": "what is missing", "importance": 0.8, "suggested_evidence": "what to gather"}
  ],
  "recommendations": ["actionable recommendations"]
}

Support levels: "strong", "moderate", "weak", "insufficient", "contradictory"
Relevance types: "direct", "indirect", "tangential"

Guidelines:
- Evaluate each evidence item independently
- Consider source credibility factors
- Identify inferential chains
- Detect contradictions explicitly
- Note evidence gaps
- Provide actionable recommendations

Always respond with valid JSON only."#;

/// System prompt for Bayesian probability updates.
pub const BAYESIAN_UPDATER_PROMPT: &str = r#"You are a probabilistic reasoning assistant. Update beliefs using Bayesian inference.

Your response MUST be valid JSON in this format:
{
  "prior": 0.5,
  "posterior": 0.73,
  "confidence_interval": {"lower": 0.65, "upper": 0.81, "level": 0.95},
  "update_steps": [
    {
      "evidence": "description",
      "prior_before": 0.5,
      "likelihood_ratio": 2.5,
      "posterior_after": 0.71,
      "explanation": "how this evidence updates belief"
    }
  ],
  "uncertainty_analysis": {
    "entropy_before": 1.0,
    "entropy_after": 0.83,
    "information_gained": 0.17,
    "remaining_uncertainty": "what remains uncertain"
  },
  "sensitivity": {
    "most_influential_evidence": "which evidence matters most",
    "robustness": 0.8,
    "critical_assumptions": ["key assumptions"]
  },
  "interpretation": {
    "verbal_probability": "likely",
    "recommendation": "what to do",
    "caveats": ["important caveats"]
  }
}

Verbal probability scale: "almost_certain", "highly_likely", "likely", "possible", "unlikely", "highly_unlikely", "almost_impossible"

Guidelines:
- Apply Bayes' rule correctly: P(H|E) = P(E|H) * P(H) / P(E)
- Estimate likelihood ratios when not provided
- Calculate entropy and information gain
- Provide verbal probability interpretations
- Note critical assumptions

Always respond with valid JSON only."#;

// ============================================================================
// Phase 4: Bias & Fallacy Detection Prompts
// ============================================================================

// ============================================================================
// Phase 6: Time Machine Prompts (Timeline, MCTS, Counterfactual)
// ============================================================================

/// System prompt for timeline-based reasoning exploration.
pub const TIMELINE_REASONING_PROMPT: &str = r#"You are a temporal reasoning assistant that explores and compares alternative reasoning paths through time.

Your response MUST be valid JSON in this format:
{
  "summary": "overview of timeline exploration",
  "current_path": {
    "thought": "current reasoning state",
    "confidence": 0.8,
    "depth": 3
  },
  "alternatives": [
    {
      "thought": "alternative reasoning path",
      "confidence": 0.7,
      "divergence_point": "where this path diverged",
      "rationale": "why this alternative exists"
    }
  ],
  "recommended_action": "continue|branch|backtrack|merge",
  "metadata": {}
}

Guidelines:
- Track reasoning state across temporal exploration
- Identify key decision points where paths diverge
- Compare parallel reasoning trajectories
- Recommend optimal navigation through the reasoning space
- Support branching, merging, and backtracking operations"#;

/// System prompt for MCTS-guided reasoning exploration.
pub const MCTS_EXPLORATION_PROMPT: &str = r#"You are a Monte Carlo Tree Search reasoning assistant that uses UCB1-guided exploration to find optimal reasoning paths.

Your response MUST be valid JSON in this format:
{
  "node_evaluation": {
    "content": "current reasoning node content",
    "value": 0.75,
    "visit_count": 5,
    "ucb_score": 1.42,
    "is_promising": true
  },
  "expansion": [
    {
      "thought": "expanded child node content",
      "prior": 0.6,
      "rationale": "why this expansion is valuable"
    }
  ],
  "simulation_result": {
    "outcome_value": 0.8,
    "path_taken": ["step1", "step2", "step3"],
    "terminal_state": "conclusion reached"
  },
  "recommendation": {
    "action": "select|expand|simulate|backpropagate",
    "target_node": "node_id or null",
    "confidence": 0.82
  },
  "metadata": {}
}

UCB1 Formula: Q(s,a) + c * sqrt(ln(N_parent) / N(s,a))
- Q(s,a): Average value of node (exploitation)
- c: Exploration constant (typically sqrt(2))
- N_parent: Parent visit count
- N(s,a): Node visit count

Guidelines:
- Balance exploration vs exploitation using UCB1
- Expand promising nodes with high UCB scores
- Simulate rollouts to estimate node values
- Backpropagate results to update ancestor values
- Select nodes with highest UCB for further exploration"#;

/// System prompt for counterfactual "what if" reasoning.
pub const COUNTERFACTUAL_ANALYSIS_PROMPT: &str = r#"You are a counterfactual reasoning assistant using Pearl's Ladder of Causation for "what if" analysis.

Your response MUST be valid JSON in this format:
{
  "summary": "brief summary of counterfactual analysis",
  "counterfactual_outcome": "what would have happened differently",
  "actual_outcome": "what actually happened/was concluded",
  "outcome_delta": 0.3,
  "differences": ["key difference 1", "key difference 2"],
  "changed_factors": ["factor that would change"],
  "unchanged_factors": ["factor that stays the same"],
  "causal_attribution": 0.75,
  "confidence": 0.82,
  "insights": ["key insight from the analysis"]
}

Pearl's Ladder of Causation:
1. Association (Seeing): P(Y|X) - What correlations exist?
2. Intervention (Doing): P(Y|do(X)) - What if we intervene?
3. Counterfactual (Imagining): P(Y_x|X',Y') - What if X had been different?

Intervention Types:
- CHANGE: Modify an existing element
- REMOVE: Eliminate an element entirely
- REPLACE: Substitute with something different
- INJECT: Add a new element at a decision point

Guidelines:
- Apply causal reasoning rigorously
- Distinguish correlation from causation
- Consider both direct and indirect effects
- Estimate causal attribution (how much intervention caused change)
- Identify what would and wouldn't change
- Provide actionable insights from the analysis"#;

/// System prompt for auto-backtracking decision.
pub const AUTO_BACKTRACK_PROMPT: &str = r#"You are a reasoning quality monitor that decides when to automatically backtrack to a better reasoning state.

Your response MUST be valid JSON in this format:
{
  "current_assessment": {
    "quality_score": 0.4,
    "confidence": 0.6,
    "issues_detected": ["issue 1", "issue 2"]
  },
  "should_backtrack": true,
  "recommended_checkpoint": "checkpoint_id or null",
  "backtrack_rationale": "why backtracking is recommended",
  "alternative_directions": [
    {
      "direction": "alternative approach description",
      "expected_improvement": 0.3,
      "confidence": 0.7
    }
  ],
  "threshold_analysis": {
    "quality_threshold": 0.6,
    "confidence_threshold": 0.7,
    "current_meets_threshold": false
  },
  "metadata": {}
}

Backtracking Triggers:
- Quality score below threshold
- Confidence dropping consistently
- Detected reasoning loops or contradictions
- Dead-end reached with no viable continuations
- Better alternative path discovered

Guidelines:
- Monitor reasoning quality continuously
- Compare current state to checkpoints
- Recommend backtracking when quality degrades significantly
- Suggest alternative directions after backtracking
- Preserve good reasoning segments while discarding problematic ones"#;

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
        // Phase 5: Decision Framework & Evidence Assessment
        "decision" | "make_decision" | "decision-maker" => DECISION_MAKER_PROMPT,
        "perspective" | "analyze_perspectives" | "perspective-analyzer" => {
            PERSPECTIVE_ANALYZER_PROMPT
        }
        "evidence" | "assess_evidence" | "evidence-assessor" => EVIDENCE_ASSESSOR_PROMPT,
        "probabilistic" | "bayesian" | "bayesian-updater" => BAYESIAN_UPDATER_PROMPT,
        // Phase 6: Time Machine (Timeline, MCTS, Counterfactual)
        "timeline" | "timeline_reasoning" | "temporal" => TIMELINE_REASONING_PROMPT,
        "mcts" | "mcts_exploration" | "monte_carlo" => MCTS_EXPLORATION_PROMPT,
        "counterfactual" | "what_if" | "causal" => COUNTERFACTUAL_ANALYSIS_PROMPT,
        "autobacktrack" | "auto_backtrack" | "backtrack_decision" => AUTO_BACKTRACK_PROMPT,
        _ => LINEAR_REASONING_PROMPT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::const_is_empty)] // Intentional test that constants are populated
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

    // ========================================================================
    // Phase 5: Decision Framework & Evidence Assessment Prompt Tests
    // ========================================================================

    #[test]
    #[allow(clippy::const_is_empty)] // Intentional test that constants are populated
    fn test_decision_framework_prompts_not_empty() {
        assert!(!DECISION_MAKER_PROMPT.is_empty());
        assert!(!PERSPECTIVE_ANALYZER_PROMPT.is_empty());
        assert!(!EVIDENCE_ASSESSOR_PROMPT.is_empty());
        assert!(!BAYESIAN_UPDATER_PROMPT.is_empty());
    }

    #[test]
    fn test_decision_framework_prompts_contain_json() {
        assert!(DECISION_MAKER_PROMPT.contains("JSON"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("JSON"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("JSON"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("JSON"));
    }

    #[test]
    fn test_decision_maker_prompt_content() {
        assert!(DECISION_MAKER_PROMPT.contains("decision analysis"));
        assert!(DECISION_MAKER_PROMPT.contains("recommendation"));
        assert!(DECISION_MAKER_PROMPT.contains("sensitivity_analysis"));
        assert!(DECISION_MAKER_PROMPT.contains("trade_offs"));
        assert!(DECISION_MAKER_PROMPT.contains("constraints_satisfied"));
    }

    #[test]
    fn test_perspective_analyzer_prompt_content() {
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("stakeholder"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("power_matrix"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("key_player"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("conflicts"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("alignments"));
    }

    #[test]
    fn test_evidence_assessor_prompt_content() {
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("evidence"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("overall_support"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("credibility"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("chain_analysis"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("contradictions"));
    }

    #[test]
    fn test_bayesian_updater_prompt_content() {
        assert!(BAYESIAN_UPDATER_PROMPT.contains("Bayesian"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("prior"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("posterior"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("likelihood_ratio"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("entropy"));
    }

    #[test]
    fn test_get_prompt_for_decision_modes() {
        assert_eq!(get_prompt_for_mode("decision"), DECISION_MAKER_PROMPT);
        assert_eq!(get_prompt_for_mode("make_decision"), DECISION_MAKER_PROMPT);
        assert_eq!(get_prompt_for_mode("decision-maker"), DECISION_MAKER_PROMPT);

        assert_eq!(
            get_prompt_for_mode("perspective"),
            PERSPECTIVE_ANALYZER_PROMPT
        );
        assert_eq!(
            get_prompt_for_mode("analyze_perspectives"),
            PERSPECTIVE_ANALYZER_PROMPT
        );

        assert_eq!(get_prompt_for_mode("evidence"), EVIDENCE_ASSESSOR_PROMPT);
        assert_eq!(
            get_prompt_for_mode("assess_evidence"),
            EVIDENCE_ASSESSOR_PROMPT
        );

        assert_eq!(
            get_prompt_for_mode("probabilistic"),
            BAYESIAN_UPDATER_PROMPT
        );
        assert_eq!(get_prompt_for_mode("bayesian"), BAYESIAN_UPDATER_PROMPT);
    }

    #[test]
    fn test_decision_prompts_case_insensitive() {
        assert_eq!(get_prompt_for_mode("DECISION"), DECISION_MAKER_PROMPT);
        assert_eq!(
            get_prompt_for_mode("PERSPECTIVE"),
            PERSPECTIVE_ANALYZER_PROMPT
        );
        assert_eq!(get_prompt_for_mode("EVIDENCE"), EVIDENCE_ASSESSOR_PROMPT);
        assert_eq!(get_prompt_for_mode("BAYESIAN"), BAYESIAN_UPDATER_PROMPT);
    }

    // ========================================================================
    // Additional Comprehensive Tests
    // ========================================================================

    // Test 1: Verify all GoT prompts are non-empty
    #[test]
    #[allow(clippy::const_is_empty)] // Intentional test that constants are populated
    fn test_got_prompts_not_empty() {
        assert!(!GOT_GENERATE_PROMPT.is_empty());
        assert!(!GOT_SCORE_PROMPT.is_empty());
        assert!(!GOT_AGGREGATE_PROMPT.is_empty());
        assert!(!GOT_REFINE_PROMPT.is_empty());
    }

    // Test 2: Verify backtracking prompt is non-empty
    #[test]
    #[allow(clippy::const_is_empty)] // Intentional test that constants are populated
    fn test_backtracking_prompt_not_empty() {
        assert!(!BACKTRACKING_PROMPT.is_empty());
    }

    // Test 3: Verify all GoT prompts contain JSON format hints
    #[test]
    fn test_got_prompts_contain_json() {
        assert!(GOT_GENERATE_PROMPT.contains("JSON"));
        assert!(GOT_SCORE_PROMPT.contains("JSON"));
        assert!(GOT_AGGREGATE_PROMPT.contains("JSON"));
        assert!(GOT_REFINE_PROMPT.contains("JSON"));
    }

    // Test 4: Verify backtracking prompt contains JSON format hints
    #[test]
    fn test_backtracking_prompt_contains_json() {
        assert!(BACKTRACKING_PROMPT.contains("JSON"));
    }

    // Test 5: Verify linear prompt contains required keywords
    #[test]
    fn test_linear_prompt_keywords() {
        assert!(LINEAR_REASONING_PROMPT.contains("reasoning"));
        assert!(LINEAR_REASONING_PROMPT.contains("thought"));
        assert!(LINEAR_REASONING_PROMPT.contains("confidence"));
        assert!(LINEAR_REASONING_PROMPT.contains("metadata"));
    }

    // Test 6: Verify tree prompt contains required keywords
    #[test]
    fn test_tree_prompt_keywords() {
        assert!(TREE_REASONING_PROMPT.contains("branches"));
        assert!(TREE_REASONING_PROMPT.contains("reasoning paths"));
        assert!(TREE_REASONING_PROMPT.contains("recommended_branch"));
        assert!(TREE_REASONING_PROMPT.contains("rationale"));
    }

    // Test 7: Verify divergent prompt contains required keywords
    #[test]
    fn test_divergent_prompt_keywords() {
        assert!(DIVERGENT_REASONING_PROMPT.contains("creative"));
        assert!(DIVERGENT_REASONING_PROMPT.contains("perspectives"));
        assert!(DIVERGENT_REASONING_PROMPT.contains("novelty"));
        assert!(DIVERGENT_REASONING_PROMPT.contains("synthesis"));
    }

    // Test 8: Verify reflection prompt contains required keywords
    #[test]
    fn test_reflection_prompt_keywords() {
        assert!(REFLECTION_PROMPT.contains("meta-cognitive"));
        assert!(REFLECTION_PROMPT.contains("analysis"));
        assert!(REFLECTION_PROMPT.contains("strengths"));
        assert!(REFLECTION_PROMPT.contains("weaknesses"));
        assert!(REFLECTION_PROMPT.contains("recommendations"));
    }

    // Test 9: Verify auto router prompt contains required keywords
    #[test]
    fn test_auto_router_prompt_keywords() {
        assert!(AUTO_ROUTER_PROMPT.contains("mode selector"));
        assert!(AUTO_ROUTER_PROMPT.contains("recommended_mode"));
        assert!(AUTO_ROUTER_PROMPT.contains("complexity"));
        assert!(AUTO_ROUTER_PROMPT.contains("linear|tree|divergent|reflection|got"));
    }

    // Test 10: Verify backtracking prompt contains required keywords
    #[test]
    fn test_backtracking_prompt_keywords() {
        assert!(BACKTRACKING_PROMPT.contains("backtracking"));
        assert!(BACKTRACKING_PROMPT.contains("checkpoint"));
        assert!(BACKTRACKING_PROMPT.contains("context_restored"));
        assert!(BACKTRACKING_PROMPT.contains("new_direction"));
    }

    // Test 11: Verify GoT generate prompt contains required keywords
    #[test]
    fn test_got_generate_prompt_keywords() {
        assert!(GOT_GENERATE_PROMPT.contains("Graph-of-Thoughts"));
        assert!(GOT_GENERATE_PROMPT.contains("continuations"));
        assert!(GOT_GENERATE_PROMPT.contains("novelty"));
        assert!(GOT_GENERATE_PROMPT.contains("diverse"));
    }

    // Test 12: Verify GoT score prompt contains required keywords
    #[test]
    fn test_got_score_prompt_keywords() {
        assert!(GOT_SCORE_PROMPT.contains("evaluator"));
        assert!(GOT_SCORE_PROMPT.contains("overall_score"));
        assert!(GOT_SCORE_PROMPT.contains("relevance"));
        assert!(GOT_SCORE_PROMPT.contains("validity"));
        assert!(GOT_SCORE_PROMPT.contains("is_terminal_candidate"));
    }

    // Test 13: Verify GoT aggregate prompt contains required keywords
    #[test]
    fn test_got_aggregate_prompt_keywords() {
        assert!(GOT_AGGREGATE_PROMPT.contains("synthesizer"));
        assert!(GOT_AGGREGATE_PROMPT.contains("aggregated_thought"));
        assert!(GOT_AGGREGATE_PROMPT.contains("synthesis_approach"));
        assert!(GOT_AGGREGATE_PROMPT.contains("conflicts_resolved"));
    }

    // Test 14: Verify GoT refine prompt contains required keywords
    #[test]
    fn test_got_refine_prompt_keywords() {
        assert!(GOT_REFINE_PROMPT.contains("refiner"));
        assert!(GOT_REFINE_PROMPT.contains("refined_thought"));
        assert!(GOT_REFINE_PROMPT.contains("improvements_made"));
        assert!(GOT_REFINE_PROMPT.contains("quality_delta"));
    }

    // Test 15: Verify prompts have reasonable minimum lengths
    #[test]
    fn test_prompts_minimum_lengths() {
        assert!(LINEAR_REASONING_PROMPT.len() > 300);
        assert!(TREE_REASONING_PROMPT.len() > 300);
        assert!(DIVERGENT_REASONING_PROMPT.len() > 300);
        assert!(REFLECTION_PROMPT.len() > 300);
        assert!(AUTO_ROUTER_PROMPT.len() > 300);
        assert!(BACKTRACKING_PROMPT.len() > 300);
    }

    // Test 16: Verify GoT prompts have reasonable minimum lengths
    #[test]
    fn test_got_prompts_minimum_lengths() {
        assert!(GOT_GENERATE_PROMPT.len() > 300);
        assert!(GOT_SCORE_PROMPT.len() > 300);
        assert!(GOT_AGGREGATE_PROMPT.len() > 300);
        assert!(GOT_REFINE_PROMPT.len() > 300);
    }

    // Test 17: Verify detection prompts have reasonable minimum lengths
    #[test]
    fn test_detection_prompts_minimum_lengths() {
        assert!(BIAS_DETECTION_PROMPT.len() > 800);
        assert!(FALLACY_DETECTION_PROMPT.len() > 800);
    }

    // Test 18: Verify decision framework prompts have reasonable minimum lengths
    #[test]
    fn test_decision_framework_prompts_minimum_lengths() {
        assert!(DECISION_MAKER_PROMPT.len() > 800);
        assert!(PERSPECTIVE_ANALYZER_PROMPT.len() > 800);
        assert!(EVIDENCE_ASSESSOR_PROMPT.len() > 800);
        assert!(BAYESIAN_UPDATER_PROMPT.len() > 800);
    }

    // Test 19: Verify prompts don't contain each other (no copy-paste errors)
    #[test]
    fn test_prompts_uniqueness() {
        let prompts = vec![
            LINEAR_REASONING_PROMPT,
            TREE_REASONING_PROMPT,
            DIVERGENT_REASONING_PROMPT,
            REFLECTION_PROMPT,
            AUTO_ROUTER_PROMPT,
            BACKTRACKING_PROMPT,
            GOT_GENERATE_PROMPT,
            GOT_SCORE_PROMPT,
            GOT_AGGREGATE_PROMPT,
            GOT_REFINE_PROMPT,
        ];

        // Check each pair of prompts
        for (i, prompt1) in prompts.iter().enumerate() {
            for (j, prompt2) in prompts.iter().enumerate() {
                if i != j {
                    // No prompt should contain the entire content of another
                    assert!(
                        !prompt1.contains(prompt2),
                        "Prompt {} contains prompt {}",
                        i,
                        j
                    );
                }
            }
        }
    }

    // Test 20: Verify bias detection prompt contains expected bias types
    #[test]
    fn test_bias_detection_prompt_bias_types() {
        assert!(BIAS_DETECTION_PROMPT.contains("confirmation_bias"));
        assert!(BIAS_DETECTION_PROMPT.contains("anchoring_bias"));
        assert!(BIAS_DETECTION_PROMPT.contains("availability_heuristic"));
        assert!(BIAS_DETECTION_PROMPT.contains("sunk_cost_fallacy"));
        assert!(BIAS_DETECTION_PROMPT.contains("hindsight_bias"));
        assert!(BIAS_DETECTION_PROMPT.contains("bandwagon_effect"));
    }

    // Test 21: Verify fallacy detection prompt contains expected fallacy types
    #[test]
    fn test_fallacy_detection_prompt_fallacy_types() {
        assert!(FALLACY_DETECTION_PROMPT.contains("ad_hominem"));
        assert!(FALLACY_DETECTION_PROMPT.contains("straw_man"));
        assert!(FALLACY_DETECTION_PROMPT.contains("false_dichotomy"));
        assert!(FALLACY_DETECTION_PROMPT.contains("circular_reasoning"));
        assert!(FALLACY_DETECTION_PROMPT.contains("slippery_slope"));
        assert!(FALLACY_DETECTION_PROMPT.contains("hasty_generalization"));
    }

    // Test 22: Verify all prompts end with valid JSON instruction
    #[test]
    fn test_prompts_json_instruction_ending() {
        let prompts_with_endings = vec![
            (LINEAR_REASONING_PROMPT, "valid JSON"),
            (BIAS_DETECTION_PROMPT, "valid JSON"),
            (FALLACY_DETECTION_PROMPT, "valid JSON"),
            (DECISION_MAKER_PROMPT, "valid JSON"),
            (PERSPECTIVE_ANALYZER_PROMPT, "valid JSON"),
            (EVIDENCE_ASSESSOR_PROMPT, "valid JSON"),
            (BAYESIAN_UPDATER_PROMPT, "valid JSON"),
        ];

        for (prompt, expected_phrase) in prompts_with_endings {
            assert!(
                prompt.contains(expected_phrase),
                "Prompt should contain '{}'",
                expected_phrase
            );
        }
    }

    // Test 23: Verify get_prompt_for_mode handles GoT variations
    #[test]
    fn test_get_prompt_for_got_modes() {
        assert_eq!(get_prompt_for_mode("got_generate"), GOT_GENERATE_PROMPT);
        assert_eq!(get_prompt_for_mode("got-generate"), GOT_GENERATE_PROMPT);
        assert_eq!(get_prompt_for_mode("got_score"), GOT_SCORE_PROMPT);
        assert_eq!(get_prompt_for_mode("got-score"), GOT_SCORE_PROMPT);
        assert_eq!(get_prompt_for_mode("got_aggregate"), GOT_AGGREGATE_PROMPT);
        assert_eq!(get_prompt_for_mode("got-aggregate"), GOT_AGGREGATE_PROMPT);
        assert_eq!(get_prompt_for_mode("got_refine"), GOT_REFINE_PROMPT);
        assert_eq!(get_prompt_for_mode("got-refine"), GOT_REFINE_PROMPT);
    }

    // Test 24: Verify get_prompt_for_mode handles backtracking
    #[test]
    fn test_get_prompt_for_backtracking_mode() {
        assert_eq!(get_prompt_for_mode("backtracking"), BACKTRACKING_PROMPT);
    }

    // Test 25: Verify confidence range is mentioned in relevant prompts
    #[test]
    fn test_prompts_mention_confidence_range() {
        let prompts_with_confidence = vec![
            LINEAR_REASONING_PROMPT,
            TREE_REASONING_PROMPT,
            REFLECTION_PROMPT,
            AUTO_ROUTER_PROMPT,
            BACKTRACKING_PROMPT,
        ];

        for prompt in prompts_with_confidence {
            // Should mention confidence between 0 and 1 or similar
            assert!(
                prompt.contains("0.0") || prompt.contains("0.") || prompt.contains("confidence"),
                "Prompt should mention confidence scoring"
            );
        }
    }

    // Test 26: Verify decision maker prompt contains scoring elements
    #[test]
    fn test_decision_maker_prompt_scoring_elements() {
        assert!(DECISION_MAKER_PROMPT.contains("score"));
        assert!(DECISION_MAKER_PROMPT.contains("criteria_scores"));
        assert!(DECISION_MAKER_PROMPT.contains("rank"));
        assert!(DECISION_MAKER_PROMPT.contains("weights"));
    }

    // Test 27: Verify perspective analyzer prompt contains power matrix
    #[test]
    fn test_perspective_analyzer_prompt_power_matrix() {
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("power_level"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("interest_level"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("quadrant"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("keep_satisfied"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("keep_informed"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("minimal_effort"));
    }

    // Test 28: Verify evidence assessor prompt contains assessment criteria
    #[test]
    fn test_evidence_assessor_prompt_criteria() {
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("relevance"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("credibility"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("chain_analysis"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("inferential_distance"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("weak_links"));
    }

    // Test 29: Verify Bayesian updater prompt contains probability concepts
    #[test]
    fn test_bayesian_updater_prompt_probability_concepts() {
        assert!(BAYESIAN_UPDATER_PROMPT.contains("prior"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("posterior"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("likelihood_ratio"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("Bayes"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("P(H|E)"));
    }

    // Test 30: Verify evidence support levels are defined
    #[test]
    fn test_evidence_support_levels_defined() {
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("strong"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("moderate"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("weak"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("insufficient"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("contradictory"));
    }

    // Test 31: Verify Bayesian verbal probability scale is defined
    #[test]
    fn test_bayesian_verbal_probability_scale() {
        assert!(BAYESIAN_UPDATER_PROMPT.contains("almost_certain"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("highly_likely"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("likely"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("unlikely"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("almost_impossible"));
    }

    // Test 32: Verify all prompts contain response format braces
    #[test]
    fn test_prompts_contain_json_format_braces() {
        let all_prompts = vec![
            LINEAR_REASONING_PROMPT,
            TREE_REASONING_PROMPT,
            DIVERGENT_REASONING_PROMPT,
            REFLECTION_PROMPT,
            AUTO_ROUTER_PROMPT,
            BACKTRACKING_PROMPT,
            GOT_GENERATE_PROMPT,
            GOT_SCORE_PROMPT,
            GOT_AGGREGATE_PROMPT,
            GOT_REFINE_PROMPT,
            BIAS_DETECTION_PROMPT,
            FALLACY_DETECTION_PROMPT,
            DECISION_MAKER_PROMPT,
            PERSPECTIVE_ANALYZER_PROMPT,
            EVIDENCE_ASSESSOR_PROMPT,
            BAYESIAN_UPDATER_PROMPT,
        ];

        for prompt in all_prompts {
            assert!(prompt.contains("{"), "Prompt should contain opening brace");
            assert!(prompt.contains("}"), "Prompt should contain closing brace");
        }
    }

    // Test 33: Verify fallacy categories are specified
    #[test]
    fn test_fallacy_categories_specified() {
        assert!(FALLACY_DETECTION_PROMPT.contains("formal"));
        assert!(FALLACY_DETECTION_PROMPT.contains("informal"));
        assert!(FALLACY_DETECTION_PROMPT.contains("category"));
    }

    // Test 34: Verify GoT score prompt has scoring dimensions
    #[test]
    fn test_got_score_prompt_dimensions() {
        assert!(GOT_SCORE_PROMPT.contains("relevance"));
        assert!(GOT_SCORE_PROMPT.contains("validity"));
        assert!(GOT_SCORE_PROMPT.contains("depth"));
        assert!(GOT_SCORE_PROMPT.contains("novelty"));
        assert!(GOT_SCORE_PROMPT.contains("breakdown"));
    }

    // Test 35: Verify severity scales are consistent
    #[test]
    fn test_severity_scales_consistent() {
        assert!(BIAS_DETECTION_PROMPT.contains("severity: 1"));
        assert!(BIAS_DETECTION_PROMPT.contains("5"));
        assert!(FALLACY_DETECTION_PROMPT.contains("severity: 1"));
        assert!(FALLACY_DETECTION_PROMPT.contains("5"));
    }

    // Test 36: Verify guidelines sections exist in all prompts
    #[test]
    fn test_guidelines_sections_exist() {
        let prompts_with_guidelines = vec![
            LINEAR_REASONING_PROMPT,
            TREE_REASONING_PROMPT,
            DIVERGENT_REASONING_PROMPT,
            REFLECTION_PROMPT,
            BACKTRACKING_PROMPT,
            GOT_GENERATE_PROMPT,
            GOT_AGGREGATE_PROMPT,
            GOT_REFINE_PROMPT,
        ];

        for prompt in prompts_with_guidelines {
            assert!(
                prompt.contains("Guidelines:") || prompt.contains("criteria:"),
                "Prompt should contain guidelines or criteria"
            );
        }
    }

    // Test 37: Verify auto router prompt mentions all modes
    #[test]
    fn test_auto_router_mentions_all_modes() {
        assert!(AUTO_ROUTER_PROMPT.contains("linear"));
        assert!(AUTO_ROUTER_PROMPT.contains("tree"));
        assert!(AUTO_ROUTER_PROMPT.contains("divergent"));
        assert!(AUTO_ROUTER_PROMPT.contains("reflection"));
        assert!(AUTO_ROUTER_PROMPT.contains("got"));
    }

    // Test 38: Verify decision framework prompt has constraint satisfaction
    #[test]
    fn test_decision_framework_constraints() {
        assert!(DECISION_MAKER_PROMPT.contains("constraints_satisfied"));
        assert!(DECISION_MAKER_PROMPT.contains("constraint"));
    }

    // Test 39: Verify perspective analyzer has engagement strategies
    #[test]
    fn test_perspective_analyzer_engagement() {
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("engagement_strategy"));
        assert!(PERSPECTIVE_ANALYZER_PROMPT.contains("engagement"));
    }

    // Test 40: Verify evidence assessor has gap analysis
    #[test]
    fn test_evidence_assessor_gaps() {
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("gaps"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("suggested_evidence"));
        assert!(EVIDENCE_ASSESSOR_PROMPT.contains("recommendations"));
    }

    // Test 41: Verify Bayesian updater has uncertainty analysis
    #[test]
    fn test_bayesian_updater_uncertainty() {
        assert!(BAYESIAN_UPDATER_PROMPT.contains("uncertainty_analysis"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("entropy"));
        assert!(BAYESIAN_UPDATER_PROMPT.contains("information_gained"));
    }

    // Test 42: Verify get_prompt_for_mode default fallback
    #[test]
    fn test_get_prompt_for_mode_default_fallback() {
        // Unknown modes should fall back to linear
        assert_eq!(
            get_prompt_for_mode("completely_unknown_mode"),
            LINEAR_REASONING_PROMPT
        );
        assert_eq!(get_prompt_for_mode(""), LINEAR_REASONING_PROMPT);
        assert_eq!(get_prompt_for_mode("xyz123"), LINEAR_REASONING_PROMPT);
    }

    // Test 43: Verify router mode aliases work
    #[test]
    fn test_router_mode_aliases() {
        assert_eq!(get_prompt_for_mode("auto"), AUTO_ROUTER_PROMPT);
        assert_eq!(get_prompt_for_mode("router"), AUTO_ROUTER_PROMPT);
    }

    // Test 44: Verify prompts don't have common typos
    #[test]
    fn test_prompts_no_common_typos() {
        let all_prompts = vec![
            LINEAR_REASONING_PROMPT,
            TREE_REASONING_PROMPT,
            DIVERGENT_REASONING_PROMPT,
            REFLECTION_PROMPT,
            AUTO_ROUTER_PROMPT,
            BACKTRACKING_PROMPT,
            GOT_GENERATE_PROMPT,
            GOT_SCORE_PROMPT,
            GOT_AGGREGATE_PROMPT,
            GOT_REFINE_PROMPT,
        ];

        for prompt in all_prompts {
            // Check for common typos
            assert!(!prompt.contains("teh "));
            assert!(!prompt.contains("recieve"));
            assert!(!prompt.contains("occured"));
            assert!(!prompt.contains("seperate"));
        }
    }

    // Test 45: Verify bias detection has remediation guidance
    #[test]
    fn test_bias_detection_remediation() {
        assert!(BIAS_DETECTION_PROMPT.contains("remediation"));
        assert!(BIAS_DETECTION_PROMPT.contains("address it"));
    }

    // Test 46: Verify fallacy detection has remediation guidance
    #[test]
    fn test_fallacy_detection_remediation() {
        assert!(FALLACY_DETECTION_PROMPT.contains("remediation"));
        assert!(FALLACY_DETECTION_PROMPT.contains("fix the argument"));
    }

    // ========================================================================
    // Phase 6: Time Machine Prompt Tests
    // ========================================================================

    #[test]
    #[allow(clippy::const_is_empty)]
    fn test_time_machine_prompts_not_empty() {
        assert!(!TIMELINE_REASONING_PROMPT.is_empty());
        assert!(!MCTS_EXPLORATION_PROMPT.is_empty());
        assert!(!COUNTERFACTUAL_ANALYSIS_PROMPT.is_empty());
        assert!(!AUTO_BACKTRACK_PROMPT.is_empty());
    }

    #[test]
    fn test_time_machine_prompts_contain_json() {
        assert!(TIMELINE_REASONING_PROMPT.contains("JSON"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("JSON"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("JSON"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("JSON"));
    }

    #[test]
    fn test_timeline_prompt_keywords() {
        assert!(TIMELINE_REASONING_PROMPT.contains("temporal"));
        assert!(TIMELINE_REASONING_PROMPT.contains("alternatives"));
        assert!(TIMELINE_REASONING_PROMPT.contains("divergence_point"));
        assert!(TIMELINE_REASONING_PROMPT.contains("recommended_action"));
        assert!(TIMELINE_REASONING_PROMPT.contains("branch"));
        assert!(TIMELINE_REASONING_PROMPT.contains("merge"));
    }

    #[test]
    fn test_mcts_prompt_keywords() {
        assert!(MCTS_EXPLORATION_PROMPT.contains("Monte Carlo Tree Search"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("UCB1"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("exploration"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("exploitation"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("ucb_score"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("backpropagate"));
    }

    #[test]
    fn test_counterfactual_prompt_keywords() {
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("Pearl's Ladder"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("counterfactual"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("causal_attribution"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("Intervention"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("CHANGE"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("REMOVE"));
    }

    #[test]
    fn test_auto_backtrack_prompt_keywords() {
        assert!(AUTO_BACKTRACK_PROMPT.contains("backtrack"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("quality_score"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("should_backtrack"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("checkpoint"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("threshold"));
    }

    #[test]
    fn test_get_prompt_for_time_machine_modes() {
        // Timeline variations
        assert_eq!(get_prompt_for_mode("timeline"), TIMELINE_REASONING_PROMPT);
        assert_eq!(
            get_prompt_for_mode("timeline_reasoning"),
            TIMELINE_REASONING_PROMPT
        );
        assert_eq!(get_prompt_for_mode("temporal"), TIMELINE_REASONING_PROMPT);

        // MCTS variations
        assert_eq!(get_prompt_for_mode("mcts"), MCTS_EXPLORATION_PROMPT);
        assert_eq!(
            get_prompt_for_mode("mcts_exploration"),
            MCTS_EXPLORATION_PROMPT
        );
        assert_eq!(get_prompt_for_mode("monte_carlo"), MCTS_EXPLORATION_PROMPT);

        // Counterfactual variations
        assert_eq!(
            get_prompt_for_mode("counterfactual"),
            COUNTERFACTUAL_ANALYSIS_PROMPT
        );
        assert_eq!(get_prompt_for_mode("what_if"), COUNTERFACTUAL_ANALYSIS_PROMPT);
        assert_eq!(get_prompt_for_mode("causal"), COUNTERFACTUAL_ANALYSIS_PROMPT);

        // Auto-backtrack variations
        assert_eq!(get_prompt_for_mode("autobacktrack"), AUTO_BACKTRACK_PROMPT);
        assert_eq!(get_prompt_for_mode("auto_backtrack"), AUTO_BACKTRACK_PROMPT);
        assert_eq!(
            get_prompt_for_mode("backtrack_decision"),
            AUTO_BACKTRACK_PROMPT
        );
    }

    #[test]
    fn test_time_machine_prompts_case_insensitive() {
        assert_eq!(get_prompt_for_mode("TIMELINE"), TIMELINE_REASONING_PROMPT);
        assert_eq!(get_prompt_for_mode("MCTS"), MCTS_EXPLORATION_PROMPT);
        assert_eq!(
            get_prompt_for_mode("COUNTERFACTUAL"),
            COUNTERFACTUAL_ANALYSIS_PROMPT
        );
        assert_eq!(get_prompt_for_mode("AUTOBACKTRACK"), AUTO_BACKTRACK_PROMPT);
    }

    #[test]
    fn test_time_machine_prompts_minimum_lengths() {
        assert!(TIMELINE_REASONING_PROMPT.len() > 400);
        assert!(MCTS_EXPLORATION_PROMPT.len() > 600);
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.len() > 600);
        assert!(AUTO_BACKTRACK_PROMPT.len() > 500);
    }

    #[test]
    fn test_mcts_prompt_ucb_formula() {
        assert!(MCTS_EXPLORATION_PROMPT.contains("Q(s,a)"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("sqrt"));
        assert!(MCTS_EXPLORATION_PROMPT.contains("N_parent"));
    }

    #[test]
    fn test_counterfactual_pearl_ladder_levels() {
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("Association"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("Intervention"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("Counterfactual"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("P(Y|X)"));
        assert!(COUNTERFACTUAL_ANALYSIS_PROMPT.contains("do(X)"));
    }

    #[test]
    fn test_auto_backtrack_triggers() {
        assert!(AUTO_BACKTRACK_PROMPT.contains("Quality score below threshold"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("Confidence dropping"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("Dead-end"));
        assert!(AUTO_BACKTRACK_PROMPT.contains("alternative path"));
    }
}
