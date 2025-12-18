//! Built-in workflow presets for common reasoning tasks.

use super::types::{ParamSpec, PresetStep, WorkflowPreset};
use serde_json::json;
use std::collections::HashMap;

/// Code review workflow using multiple reasoning modes.
///
/// Steps:
/// 1. Divergent analysis - Generate multiple perspectives
/// 2. Bias detection - Check for cognitive biases (optional)
/// 3. Fallacy detection - Check for logical fallacies (optional)
/// 4. Reflection - Synthesize findings
pub fn code_review_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "code-review".to_string(),
        name: "Code Review".to_string(),
        description:
            "Analyze code quality using divergent thinking, bias detection, and reflection"
                .to_string(),
        category: "code".to_string(),
        estimated_time: "2-3 minutes".to_string(),
        output_format: "structured_review".to_string(),
        tags: vec![
            "code".to_string(),
            "review".to_string(),
            "quality".to_string(),
        ],
        input_schema: HashMap::from([
            (
                "code".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "The code to review".to_string(),
                    examples: vec![json!("function example() { ... }")],
                },
            ),
            (
                "focus".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: false,
                    default: Some(json!("quality")),
                    description: "Review focus (quality, performance, security)".to_string(),
                    examples: vec![json!("performance"), json!("security")],
                },
            ),
        ]),
        steps: vec![
            PresetStep::new("divergent_analysis", "reasoning_divergent")
                .with_description("Generate multiple perspectives on the code")
                .with_input("content", "code")
                .with_static("num_perspectives", json!(3))
                .with_static("challenge_assumptions", json!(true))
                .store_as("perspectives"),
            PresetStep::new("bias_check", "reasoning_detect_biases")
                .with_description("Check for cognitive biases in reasoning")
                .with_input("content", "code")
                .store_as("biases")
                .depends_on(vec!["divergent_analysis".to_string()])
                .optional(),
            PresetStep::new("fallacy_check", "reasoning_detect_fallacies")
                .with_description("Check for logical fallacies")
                .with_input("content", "code")
                .store_as("fallacies")
                .depends_on(vec!["divergent_analysis".to_string()])
                .optional(),
            PresetStep::new("reflect", "reasoning_reflection")
                .with_description("Synthesize findings into final assessment")
                .with_input("content", "code")
                .with_static("quality_threshold", json!(0.7))
                .store_as("reflection")
                .depends_on(vec![
                    "divergent_analysis".to_string(),
                    "bias_check".to_string(),
                    "fallacy_check".to_string(),
                ]),
        ],
    }
}

/// Debug analysis workflow with hypothesis generation.
///
/// Steps:
/// 1. Linear analysis - Initial problem understanding
/// 2. Tree exploration - Generate hypothesis branches
/// 3. Checkpoint - Save state for backtracking (optional)
/// 4. Reflection - Evaluate and recommend solution
pub fn debug_analysis_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "debug-analysis".to_string(),
        name: "Debug Analysis".to_string(),
        description: "Systematic debugging with tree exploration and hypothesis generation"
            .to_string(),
        category: "code".to_string(),
        estimated_time: "3-5 minutes".to_string(),
        output_format: "debug_report".to_string(),
        tags: vec!["debug".to_string(), "analysis".to_string()],
        input_schema: HashMap::from([
            (
                "problem".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "Description of the bug or issue".to_string(),
                    examples: vec![json!("Function returns null unexpectedly")],
                },
            ),
            (
                "context".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    description: "Relevant context (recent changes, environment)".to_string(),
                    examples: vec![],
                },
            ),
        ]),
        steps: vec![
            PresetStep::new("linear_analysis", "reasoning_linear")
                .with_description("Initial problem analysis")
                .with_input("content", "problem")
                .with_static("confidence", json!(0.8))
                .store_as("initial"),
            PresetStep::new("hypothesis_tree", "reasoning_tree")
                .with_description("Generate hypothesis branches")
                .with_input("content", "problem")
                .with_static("num_branches", json!(3))
                .store_as("hypotheses")
                .depends_on(vec!["linear_analysis".to_string()]),
            PresetStep::new("checkpoint", "reasoning_checkpoint_create")
                .with_description("Save state for potential backtracking")
                .with_static("name", json!("debug-checkpoint"))
                .with_static("description", json!("After hypothesis generation"))
                .store_as("checkpoint")
                .depends_on(vec!["hypothesis_tree".to_string()])
                .optional(),
            PresetStep::new("reflect", "reasoning_reflection")
                .with_description("Evaluate hypotheses and recommend solution")
                .with_input("content", "problem")
                .with_static("quality_threshold", json!(0.75))
                .store_as("conclusion")
                .depends_on(vec!["hypothesis_tree".to_string()]),
        ],
    }
}

/// Architecture decision workflow with multi-criteria analysis.
///
/// Steps:
/// 1. Divergent exploration - Generate architectural options
/// 2. GoT init - Initialize decision graph
/// 3. GoT generate - Expand with evaluation criteria
/// 4. GoT score - Score options
/// 5. GoT finalize - Produce recommendation
pub fn architecture_decision_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "architecture-decision".to_string(),
        name: "Architecture Decision".to_string(),
        description:
            "Multi-criteria architectural analysis using divergent thinking and Graph-of-Thoughts"
                .to_string(),
        category: "architecture".to_string(),
        estimated_time: "4-6 minutes".to_string(),
        output_format: "adr".to_string(),
        tags: vec![
            "architecture".to_string(),
            "decision".to_string(),
            "adr".to_string(),
        ],
        input_schema: HashMap::from([
            (
                "question".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "The architectural decision to make".to_string(),
                    examples: vec![json!("Should we use microservices or monolith?")],
                },
            ),
            (
                "constraints".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    description: "Known constraints or requirements".to_string(),
                    examples: vec![json!("Must scale to 10k users, budget limited")],
                },
            ),
        ]),
        steps: vec![
            PresetStep::new("explore_options", "reasoning_divergent")
                .with_description("Generate architectural options")
                .with_input("content", "question")
                .with_static("num_perspectives", json!(4))
                .with_static("challenge_assumptions", json!(true))
                .with_static("force_rebellion", json!(true))
                .store_as("options"),
            PresetStep::new("got_init", "reasoning_got_init")
                .with_description("Initialize decision graph")
                .with_input("content", "question")
                .store_as("graph")
                .depends_on(vec!["explore_options".to_string()]),
            PresetStep::new("got_generate", "reasoning_got_generate")
                .with_description("Expand decision tree with criteria")
                .with_input("problem", "question")
                .with_static("k", json!(3))
                .store_as("expansions")
                .depends_on(vec!["got_init".to_string()]),
            PresetStep::new("got_score", "reasoning_got_score")
                .with_description("Score each option against criteria")
                .with_input("problem", "question")
                .store_as("scores")
                .depends_on(vec!["got_generate".to_string()]),
            PresetStep::new("got_finalize", "reasoning_got_finalize")
                .with_description("Finalize decision with recommendation")
                .store_as("decision")
                .depends_on(vec!["got_score".to_string()]),
        ],
    }
}

/// Strategic decision workflow using multi-criteria analysis and stakeholder perspectives.
///
/// Steps:
/// 1. Make decision - Multi-criteria decision analysis
/// 2. Analyze perspectives - Stakeholder power/interest mapping
/// 3. Bias detection - Check for cognitive biases (optional)
/// 4. Reflection - Synthesize findings
pub fn strategic_decision_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "strategic-decision".to_string(),
        name: "Strategic Decision".to_string(),
        description:
            "Multi-criteria decision analysis with stakeholder perspectives and bias detection"
                .to_string(),
        category: "decision".to_string(),
        estimated_time: "3-5 minutes".to_string(),
        output_format: "decision_report".to_string(),
        tags: vec![
            "decision".to_string(),
            "strategy".to_string(),
            "stakeholder".to_string(),
        ],
        input_schema: HashMap::from([
            (
                "question".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "The decision question to analyze".to_string(),
                    examples: vec![json!("Should we migrate to cloud or stay on-premise?")],
                },
            ),
            (
                "alternatives".to_string(),
                ParamSpec {
                    param_type: "array".to_string(),
                    required: true,
                    default: None,
                    description: "List of alternatives to evaluate".to_string(),
                    examples: vec![json!(["Option A", "Option B", "Option C"])],
                },
            ),
            (
                "topic".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: false,
                    default: None,
                    description: "Topic for stakeholder analysis (defaults to question)"
                        .to_string(),
                    examples: vec![],
                },
            ),
        ]),
        steps: vec![
            PresetStep::new("decision_analysis", "reasoning_make_decision")
                .with_description("Multi-criteria decision analysis")
                .with_input("question", "question")
                .with_input("alternatives", "alternatives")
                .with_static("method", json!("weighted_sum"))
                .store_as("decision"),
            PresetStep::new("stakeholder_analysis", "reasoning_analyze_perspectives")
                .with_description("Analyze stakeholder perspectives")
                .with_input("topic", "question")
                .store_as("perspectives")
                .depends_on(vec!["decision_analysis".to_string()]),
            PresetStep::new("bias_check", "reasoning_detect_biases")
                .with_description("Check for cognitive biases in decision reasoning")
                .with_input("content", "question")
                .store_as("biases")
                .depends_on(vec!["decision_analysis".to_string()])
                .optional(),
            PresetStep::new("synthesize", "reasoning_reflection")
                .with_description("Synthesize decision and stakeholder analysis")
                .with_input("content", "question")
                .with_static("quality_threshold", json!(0.75))
                .store_as("synthesis")
                .depends_on(vec![
                    "decision_analysis".to_string(),
                    "stakeholder_analysis".to_string(),
                    "bias_check".to_string(),
                ]),
        ],
    }
}

/// Evidence-based conclusion workflow using evidence assessment and Bayesian reasoning.
///
/// Steps:
/// 1. Assess evidence - Evaluate evidence quality and credibility
/// 2. Probabilistic - Bayesian probability updates
/// 3. Fallacy detection - Check for logical fallacies (optional)
/// 4. Reflection - Synthesize findings into conclusion
pub fn evidence_based_conclusion_preset() -> WorkflowPreset {
    WorkflowPreset {
        id: "evidence-based-conclusion".to_string(),
        name: "Evidence-Based Conclusion".to_string(),
        description:
            "Evidence quality assessment with Bayesian probability updates and fallacy detection"
                .to_string(),
        category: "research".to_string(),
        estimated_time: "3-4 minutes".to_string(),
        output_format: "evidence_report".to_string(),
        tags: vec![
            "evidence".to_string(),
            "research".to_string(),
            "bayesian".to_string(),
        ],
        input_schema: HashMap::from([
            (
                "claim".to_string(),
                ParamSpec {
                    param_type: "string".to_string(),
                    required: true,
                    default: None,
                    description: "The claim or hypothesis to evaluate".to_string(),
                    examples: vec![json!("The new feature improves user engagement by 20%")],
                },
            ),
            (
                "evidence".to_string(),
                ParamSpec {
                    param_type: "array".to_string(),
                    required: true,
                    default: None,
                    description: "Array of evidence items with content and optional source info"
                        .to_string(),
                    examples: vec![json!([
                        {"content": "A/B test results", "source_type": "primary"},
                        {"content": "User feedback", "source_type": "anecdotal"}
                    ])],
                },
            ),
            (
                "prior".to_string(),
                ParamSpec {
                    param_type: "number".to_string(),
                    required: false,
                    default: Some(json!(0.5)),
                    description: "Prior probability (0-1) for Bayesian analysis".to_string(),
                    examples: vec![json!(0.3), json!(0.7)],
                },
            ),
        ]),
        steps: vec![
            PresetStep::new("evidence_assessment", "reasoning_assess_evidence")
                .with_description("Assess evidence quality and credibility")
                .with_input("claim", "claim")
                .with_input("evidence", "evidence")
                .store_as("assessment"),
            PresetStep::new("bayesian_update", "reasoning_probabilistic")
                .with_description("Bayesian probability update based on evidence")
                .with_input("hypothesis", "claim")
                .with_input("prior", "prior")
                .with_input("evidence", "evidence")
                .store_as("probability")
                .depends_on(vec!["evidence_assessment".to_string()]),
            PresetStep::new("fallacy_check", "reasoning_detect_fallacies")
                .with_description("Check for logical fallacies in reasoning")
                .with_input("content", "claim")
                .store_as("fallacies")
                .depends_on(vec!["evidence_assessment".to_string()])
                .optional(),
            PresetStep::new("conclude", "reasoning_reflection")
                .with_description("Synthesize evidence into final conclusion")
                .with_input("content", "claim")
                .with_static("quality_threshold", json!(0.8))
                .store_as("conclusion")
                .depends_on(vec![
                    "evidence_assessment".to_string(),
                    "bayesian_update".to_string(),
                    "fallacy_check".to_string(),
                ]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_review_preset() {
        let preset = code_review_preset();
        assert_eq!(preset.id, "code-review");
        assert_eq!(preset.category, "code");
        assert_eq!(preset.steps.len(), 4);
        assert!(preset.input_schema.contains_key("code"));
        assert!(preset.input_schema.get("code").unwrap().required);
    }

    #[test]
    fn test_debug_analysis_preset() {
        let preset = debug_analysis_preset();
        assert_eq!(preset.id, "debug-analysis");
        assert_eq!(preset.category, "code");
        assert_eq!(preset.steps.len(), 4);
        assert!(preset.input_schema.contains_key("problem"));
    }

    #[test]
    fn test_architecture_decision_preset() {
        let preset = architecture_decision_preset();
        assert_eq!(preset.id, "architecture-decision");
        assert_eq!(preset.category, "architecture");
        assert_eq!(preset.steps.len(), 5);
        assert!(preset.input_schema.contains_key("question"));
    }

    #[test]
    fn test_preset_step_dependencies() {
        let preset = code_review_preset();

        // First step has no dependencies
        assert!(preset.steps[0].depends_on.is_empty());

        // Later steps depend on earlier ones
        assert!(preset.steps[1]
            .depends_on
            .contains(&"divergent_analysis".to_string()));
        assert!(preset.steps[2]
            .depends_on
            .contains(&"divergent_analysis".to_string()));

        // Final step depends on multiple
        assert!(preset.steps[3].depends_on.len() >= 2);
    }

    #[test]
    fn test_preset_optional_steps() {
        let preset = code_review_preset();

        // bias_check and fallacy_check are optional
        assert!(preset.steps[1].optional);
        assert!(preset.steps[2].optional);

        // Main steps are not optional
        assert!(!preset.steps[0].optional);
        assert!(!preset.steps[3].optional);
    }

    #[test]
    fn test_strategic_decision_preset() {
        let preset = strategic_decision_preset();
        assert_eq!(preset.id, "strategic-decision");
        assert_eq!(preset.category, "decision");
        assert_eq!(preset.steps.len(), 4);
        assert!(preset.input_schema.contains_key("question"));
        assert!(preset.input_schema.contains_key("alternatives"));
        assert!(preset.input_schema.get("question").unwrap().required);
        assert!(preset.input_schema.get("alternatives").unwrap().required);
    }

    #[test]
    fn test_evidence_based_conclusion_preset() {
        let preset = evidence_based_conclusion_preset();
        assert_eq!(preset.id, "evidence-based-conclusion");
        assert_eq!(preset.category, "research");
        assert_eq!(preset.steps.len(), 4);
        assert!(preset.input_schema.contains_key("claim"));
        assert!(preset.input_schema.contains_key("evidence"));
        assert!(preset.input_schema.get("claim").unwrap().required);
        assert!(preset.input_schema.get("evidence").unwrap().required);
    }

    #[test]
    fn test_strategic_decision_step_dependencies() {
        let preset = strategic_decision_preset();

        // First step has no dependencies
        assert!(preset.steps[0].depends_on.is_empty());

        // Later steps depend on decision_analysis
        assert!(preset.steps[1]
            .depends_on
            .contains(&"decision_analysis".to_string()));
        assert!(preset.steps[2]
            .depends_on
            .contains(&"decision_analysis".to_string()));

        // Final step depends on multiple
        assert!(preset.steps[3].depends_on.len() >= 2);
    }

    #[test]
    fn test_evidence_based_conclusion_step_dependencies() {
        let preset = evidence_based_conclusion_preset();

        // First step has no dependencies
        assert!(preset.steps[0].depends_on.is_empty());

        // bayesian_update depends on evidence_assessment
        assert!(preset.steps[1]
            .depends_on
            .contains(&"evidence_assessment".to_string()));

        // Final step depends on multiple
        assert!(preset.steps[3].depends_on.len() >= 2);
    }

    #[test]
    fn test_strategic_decision_optional_steps() {
        let preset = strategic_decision_preset();

        // bias_check is optional
        assert!(preset.steps[2].optional);

        // Main steps are not optional
        assert!(!preset.steps[0].optional);
        assert!(!preset.steps[1].optional);
        assert!(!preset.steps[3].optional);
    }

    #[test]
    fn test_evidence_based_conclusion_optional_steps() {
        let preset = evidence_based_conclusion_preset();

        // fallacy_check is optional
        assert!(preset.steps[2].optional);

        // Main steps are not optional
        assert!(!preset.steps[0].optional);
        assert!(!preset.steps[1].optional);
        assert!(!preset.steps[3].optional);
    }
}
