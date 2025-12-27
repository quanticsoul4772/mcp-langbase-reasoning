//! Counterfactual reasoning mode - "What if?" analysis and causal reasoning.
//!
//! This module provides counterfactual reasoning capabilities:
//! - Intervention-based analysis (change, remove, replace, inject)
//! - Causal attribution scoring
//! - Comparison of actual vs counterfactual outcomes
//! - Pearl's Ladder of Causation integration

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::info;

use super::{extract_json_from_completion, serialize_for_log, ModeCore};
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{LangbaseClient, Message, PipeRequest};
use crate::storage::{
    Branch, CounterfactualAnalysis, InterventionType, Invocation, SqliteStorage, Storage, Thought,
};

/// Input parameters for counterfactual analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualParams {
    /// The counterfactual question (e.g., "What if we had chosen approach B?")
    pub question: String,
    /// The branch to analyze
    pub branch_id: String,
    /// Type of intervention
    #[serde(default)]
    pub intervention_type: CounterfactualInterventionType,
    /// Specific intervention description
    pub intervention: String,
    /// Optional target thought ID within the branch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_thought_id: Option<String>,
    /// Optional timeline ID to associate with
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_id: Option<String>,
}

/// Intervention type for counterfactual analysis
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterfactualInterventionType {
    /// Change an existing element
    #[default]
    Change,
    /// Remove an element
    Remove,
    /// Replace with something different
    Replace,
    /// Inject a new element
    Inject,
}

impl From<CounterfactualInterventionType> for InterventionType {
    fn from(t: CounterfactualInterventionType) -> Self {
        match t {
            CounterfactualInterventionType::Change => InterventionType::Change,
            CounterfactualInterventionType::Remove => InterventionType::Remove,
            CounterfactualInterventionType::Replace => InterventionType::Replace,
            CounterfactualInterventionType::Inject => InterventionType::Inject,
        }
    }
}

/// Result of counterfactual analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualResult {
    /// Analysis ID
    pub analysis_id: String,
    /// The original question
    pub question: String,
    /// Summary of the counterfactual analysis
    pub summary: String,
    /// The counterfactual outcome
    pub counterfactual_outcome: String,
    /// Comparison between actual and counterfactual
    pub comparison: CounterfactualComparison,
    /// Causal attribution score (0.0-1.0)
    pub causal_attribution: f64,
    /// Confidence in the analysis
    pub confidence: f64,
    /// ID of the counterfactual branch created
    pub counterfactual_branch_id: String,
    /// Key insights from the analysis
    pub insights: Vec<String>,
}

/// Comparison between actual and counterfactual outcomes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterfactualComparison {
    /// Description of the actual outcome
    pub actual_outcome: String,
    /// Description of the counterfactual outcome
    pub counterfactual_outcome: String,
    /// Outcome delta (positive = counterfactual is better)
    pub outcome_delta: f64,
    /// Key differences
    pub differences: Vec<String>,
    /// Factors that would change
    pub changed_factors: Vec<String>,
    /// Factors that would remain the same
    pub unchanged_factors: Vec<String>,
}

/// Counterfactual mode handler for "what if" reasoning.
#[derive(Clone)]
pub struct CounterfactualMode {
    /// Core infrastructure
    core: ModeCore,
    /// Reflection pipe for analysis
    reflection_pipe: String,
    /// Decision pipe for evaluation (reserved for future use)
    #[allow(dead_code)]
    decision_pipe: String,
}

impl CounterfactualMode {
    /// Create a new counterfactual mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        // Extract pipe names from config, with defaults
        let decision_pipe = config
            .pipes
            .decision
            .as_ref()
            .and_then(|c| c.pipe.clone())
            .unwrap_or_else(|| "decision-framework-v1".to_string());

        Self {
            core: ModeCore::new(storage, langbase),
            reflection_pipe: config.pipes.reflection.clone(),
            decision_pipe,
        }
    }

    /// Perform counterfactual analysis
    pub async fn analyze(&self, params: CounterfactualParams) -> AppResult<CounterfactualResult> {
        let start = Instant::now();

        // Validate input
        if params.question.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "question".to_string(),
                reason: "Counterfactual question cannot be empty".to_string(),
            }
            .into());
        }

        if params.intervention.trim().is_empty() {
            return Err(ToolError::Validation {
                field: "intervention".to_string(),
                reason: "Intervention description cannot be empty".to_string(),
            }
            .into());
        }

        // Get the original branch
        let original_branch = self
            .core
            .storage()
            .get_branch(&params.branch_id)
            .await?
            .ok_or_else(|| ToolError::Validation {
                field: "branch_id".to_string(),
                reason: format!("Branch not found: {}", params.branch_id),
            })?;

        // Get thoughts from the branch
        let thoughts = self
            .core
            .storage()
            .get_branch_thoughts(&params.branch_id)
            .await?;

        if thoughts.is_empty() {
            return Err(ToolError::Validation {
                field: "branch_id".to_string(),
                reason: "Branch has no thoughts to analyze".to_string(),
            }
            .into());
        }

        // Get the target thought if specified
        let target_thought = if let Some(ref target_id) = params.target_thought_id {
            thoughts.iter().find(|t| t.id == *target_id)
        } else {
            thoughts.last()
        };

        let target_content = target_thought
            .map(|t| t.content.clone())
            .unwrap_or_else(|| thoughts.last().map(|t| t.content.clone()).unwrap_or_default());

        // Build the actual reasoning chain
        let actual_chain: Vec<String> = thoughts.iter().map(|t| t.content.clone()).collect();
        let actual_chain_str = actual_chain.join("\n---\n");

        // Build counterfactual analysis prompt
        let intervention_desc = match params.intervention_type {
            CounterfactualInterventionType::Change => {
                format!("CHANGE: {}", params.intervention)
            }
            CounterfactualInterventionType::Remove => {
                format!("REMOVE: {}", params.intervention)
            }
            CounterfactualInterventionType::Replace => {
                format!("REPLACE WITH: {}", params.intervention)
            }
            CounterfactualInterventionType::Inject => {
                format!("INJECT: {}", params.intervention)
            }
        };

        let counterfactual_prompt = format!(
            "Perform counterfactual analysis using Pearl's Ladder of Causation.\n\n\
             QUESTION: {}\n\n\
             INTERVENTION TYPE: {}\n\n\
             ACTUAL REASONING CHAIN:\n{}\n\n\
             TARGET ELEMENT:\n{}\n\n\
             Analyze what would have happened differently if we had applied this intervention. \
             Consider:\n\
             1. Association: What correlations exist?\n\
             2. Intervention: What would change if we do(intervention)?\n\
             3. Counterfactual: What would have happened if we had done things differently?\n\n\
             Respond with JSON:\n\
             {{\n\
               \"summary\": \"brief summary\",\n\
               \"counterfactual_outcome\": \"what would have happened\",\n\
               \"actual_outcome\": \"what actually happened\",\n\
               \"outcome_delta\": 0.0 to 1.0 (positive = counterfactual better),\n\
               \"differences\": [\"key differences\"],\n\
               \"changed_factors\": [\"factors that would change\"],\n\
               \"unchanged_factors\": [\"factors that stay the same\"],\n\
               \"causal_attribution\": 0.0 to 1.0 (how much the intervention caused the change),\n\
               \"confidence\": 0.0 to 1.0,\n\
               \"insights\": [\"key insights\"]\n\
             }}",
            params.question, intervention_desc, actual_chain_str, target_content
        );

        // Call reflection pipe for deep analysis
        let messages = vec![
            Message::system(
                "You are an expert in causal reasoning and counterfactual analysis. \
                 Apply Pearl's Ladder of Causation framework rigorously."
            ),
            Message::user(counterfactual_prompt),
        ];

        let request = PipeRequest::new(&self.reflection_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let json_str = extract_json_from_completion(&response.completion)
            .map_err(|e| ToolError::Reasoning { message: e })?;
        let analysis: CounterfactualResponseData =
            serde_json::from_str(json_str).map_err(|e| ToolError::Reasoning {
                message: format!("Failed to parse counterfactual analysis: {}", e),
            })?;

        // Create counterfactual branch
        let cf_branch = Branch::new(&original_branch.session_id)
            .with_parent(&original_branch.id)
            .with_name(format!("Counterfactual: {}", &params.question.chars().take(30).collect::<String>()))
            .with_confidence(analysis.confidence);
        self.core.storage().create_branch(&cf_branch).await?;

        // Create counterfactual thought
        let cf_thought = Thought::new(
            &original_branch.session_id,
            &analysis.counterfactual_outcome,
            "counterfactual",
        )
        .with_branch(&cf_branch.id)
        .with_confidence(analysis.confidence);
        self.core.storage().create_thought(&cf_thought).await?;

        // Build comparison JSON for storage
        let comparison_json = serde_json::json!({
            "actual_outcome": analysis.actual_outcome,
            "counterfactual_outcome": analysis.counterfactual_outcome,
            "outcome_delta": analysis.outcome_delta,
            "differences": analysis.differences,
            "changed_factors": analysis.changed_factors,
            "unchanged_factors": analysis.unchanged_factors
        });

        // Create counterfactual analysis record
        let mut cf_analysis = CounterfactualAnalysis::new(
            &original_branch.session_id,
            &params.branch_id,
            &params.question,
            params.intervention_type.clone().into(),
            &params.intervention,
            &cf_branch.id,
        )
        .with_outcome_delta(analysis.outcome_delta)
        .with_causal_attribution(analysis.causal_attribution)
        .with_confidence(analysis.confidence)
        .with_comparison(comparison_json.clone());

        // Apply optional builders if values present
        if let Some(ref timeline_id) = params.timeline_id {
            cf_analysis = cf_analysis.with_timeline(timeline_id);
        }
        if let Some(ref target_id) = params.target_thought_id {
            cf_analysis = cf_analysis.with_target_thought(target_id);
        }

        self.core.storage().create_counterfactual(&cf_analysis).await?;

        // Log invocation
        let latency = start.elapsed().as_millis() as i64;
        let invocation = Invocation::new(
            "reasoning_counterfactual",
            serialize_for_log(&params, "counterfactual_params"),
        )
        .with_session(&original_branch.session_id)
        .with_pipe(&self.reflection_pipe)
        .success(serialize_for_log(&analysis, "counterfactual_analysis"), latency);
        self.core.storage().log_invocation(&invocation).await?;

        info!(
            analysis_id = %cf_analysis.id,
            branch_id = %params.branch_id,
            causal_attribution = analysis.causal_attribution,
            outcome_delta = analysis.outcome_delta,
            latency_ms = latency,
            "Counterfactual analysis complete"
        );

        let counterfactual_outcome = analysis.counterfactual_outcome.clone();
        Ok(CounterfactualResult {
            analysis_id: cf_analysis.id,
            question: params.question,
            summary: analysis.summary,
            counterfactual_outcome: counterfactual_outcome.clone(),
            comparison: CounterfactualComparison {
                actual_outcome: analysis.actual_outcome,
                counterfactual_outcome,
                outcome_delta: analysis.outcome_delta,
                differences: analysis.differences,
                changed_factors: analysis.changed_factors,
                unchanged_factors: analysis.unchanged_factors,
            },
            causal_attribution: analysis.causal_attribution,
            confidence: analysis.confidence,
            counterfactual_branch_id: cf_branch.id,
            insights: analysis.insights,
        })
    }
}

// Internal response type for parsing

#[derive(Debug, Serialize, Deserialize)]
struct CounterfactualResponseData {
    summary: String,
    counterfactual_outcome: String,
    actual_outcome: String,
    outcome_delta: f64,
    differences: Vec<String>,
    changed_factors: Vec<String>,
    unchanged_factors: Vec<String>,
    causal_attribution: f64,
    confidence: f64,
    insights: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============================================================================
    // CounterfactualParams Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_params_deserialize() {
        let json = json!({
            "question": "What if we had chosen approach B?",
            "branch_id": "branch-123",
            "intervention": "Use approach B instead of A"
        });
        let params: CounterfactualParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.question, "What if we had chosen approach B?");
        assert_eq!(params.branch_id, "branch-123");
        assert_eq!(params.intervention, "Use approach B instead of A");
        assert!(matches!(
            params.intervention_type,
            CounterfactualInterventionType::Change
        ));
    }

    #[test]
    fn test_counterfactual_params_with_all_fields() {
        let json = json!({
            "question": "Test question",
            "branch_id": "b-1",
            "intervention_type": "remove",
            "intervention": "Remove the assumption",
            "target_thought_id": "thought-456",
            "timeline_id": "tl-789"
        });
        let params: CounterfactualParams = serde_json::from_value(json).unwrap();
        assert!(matches!(
            params.intervention_type,
            CounterfactualInterventionType::Remove
        ));
        assert_eq!(params.target_thought_id, Some("thought-456".to_string()));
        assert_eq!(params.timeline_id, Some("tl-789".to_string()));
    }

    #[test]
    fn test_counterfactual_params_serialize() {
        let params = CounterfactualParams {
            question: "Test".to_string(),
            branch_id: "b".to_string(),
            intervention_type: CounterfactualInterventionType::Replace,
            intervention: "Replace X with Y".to_string(),
            target_thought_id: None,
            timeline_id: Some("tl".to_string()),
        };
        let json = serde_json::to_value(&params).unwrap();
        assert_eq!(json["question"], "Test");
        assert_eq!(json["intervention_type"], "replace");
        assert!(json.get("target_thought_id").is_none()); // skip_serializing_if
    }

    // ============================================================================
    // CounterfactualInterventionType Tests
    // ============================================================================

    #[test]
    fn test_intervention_type_default() {
        let default = CounterfactualInterventionType::default();
        assert!(matches!(default, CounterfactualInterventionType::Change));
    }

    #[test]
    fn test_intervention_type_all_variants() {
        let variants = vec![
            ("change", CounterfactualInterventionType::Change),
            ("remove", CounterfactualInterventionType::Remove),
            ("replace", CounterfactualInterventionType::Replace),
            ("inject", CounterfactualInterventionType::Inject),
        ];
        for (json_val, expected) in variants {
            let json = json!({
                "question": "q",
                "branch_id": "b",
                "intervention_type": json_val,
                "intervention": "i"
            });
            let params: CounterfactualParams = serde_json::from_value(json).unwrap();
            assert!(
                std::mem::discriminant(&params.intervention_type)
                    == std::mem::discriminant(&expected)
            );
        }
    }

    #[test]
    fn test_intervention_type_clone() {
        let original = CounterfactualInterventionType::Inject;
        let cloned = original.clone();
        assert!(matches!(cloned, CounterfactualInterventionType::Inject));
    }

    #[test]
    fn test_intervention_type_to_storage_type() {
        let change: InterventionType = CounterfactualInterventionType::Change.into();
        assert!(matches!(change, InterventionType::Change));

        let remove: InterventionType = CounterfactualInterventionType::Remove.into();
        assert!(matches!(remove, InterventionType::Remove));

        let replace: InterventionType = CounterfactualInterventionType::Replace.into();
        assert!(matches!(replace, InterventionType::Replace));

        let inject: InterventionType = CounterfactualInterventionType::Inject.into();
        assert!(matches!(inject, InterventionType::Inject));
    }

    // ============================================================================
    // CounterfactualResult Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_result_serialize() {
        let result = CounterfactualResult {
            analysis_id: "cf-123".to_string(),
            question: "What if?".to_string(),
            summary: "Analysis summary".to_string(),
            counterfactual_outcome: "Different outcome".to_string(),
            comparison: CounterfactualComparison {
                actual_outcome: "Actual".to_string(),
                counterfactual_outcome: "Different".to_string(),
                outcome_delta: 0.3,
                differences: vec!["diff1".to_string()],
                changed_factors: vec!["factor1".to_string()],
                unchanged_factors: vec!["unchanged1".to_string()],
            },
            causal_attribution: 0.75,
            confidence: 0.85,
            counterfactual_branch_id: "cf-branch".to_string(),
            insights: vec!["Insight 1".to_string()],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["analysis_id"], "cf-123");
        assert_eq!(json["causal_attribution"], 0.75);
        assert_eq!(json["confidence"], 0.85);
        assert!(!json["insights"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_counterfactual_result_deserialize() {
        let json = json!({
            "analysis_id": "a1",
            "question": "q",
            "summary": "s",
            "counterfactual_outcome": "co",
            "comparison": {
                "actual_outcome": "ao",
                "counterfactual_outcome": "co",
                "outcome_delta": 0.5,
                "differences": [],
                "changed_factors": [],
                "unchanged_factors": []
            },
            "causal_attribution": 0.6,
            "confidence": 0.7,
            "counterfactual_branch_id": "cb",
            "insights": ["i1"]
        });
        let result: CounterfactualResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.analysis_id, "a1");
        assert_eq!(result.causal_attribution, 0.6);
        assert_eq!(result.insights.len(), 1);
    }

    // ============================================================================
    // CounterfactualComparison Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_comparison_serialize() {
        let comparison = CounterfactualComparison {
            actual_outcome: "Reality".to_string(),
            counterfactual_outcome: "Alternative".to_string(),
            outcome_delta: -0.2,
            differences: vec!["Key difference".to_string()],
            changed_factors: vec!["Changed factor".to_string()],
            unchanged_factors: vec!["Unchanged factor".to_string()],
        };
        let json = serde_json::to_value(&comparison).unwrap();
        assert_eq!(json["actual_outcome"], "Reality");
        assert_eq!(json["outcome_delta"], -0.2);
        assert_eq!(json["differences"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_counterfactual_comparison_clone() {
        let comparison = CounterfactualComparison {
            actual_outcome: "a".to_string(),
            counterfactual_outcome: "c".to_string(),
            outcome_delta: 0.1,
            differences: vec!["d".to_string()],
            changed_factors: vec![],
            unchanged_factors: vec![],
        };
        let cloned = comparison.clone();
        assert_eq!(comparison.actual_outcome, cloned.actual_outcome);
        assert_eq!(comparison.outcome_delta, cloned.outcome_delta);
    }

    #[test]
    fn test_counterfactual_comparison_empty_arrays() {
        let comparison = CounterfactualComparison {
            actual_outcome: "a".to_string(),
            counterfactual_outcome: "c".to_string(),
            outcome_delta: 0.0,
            differences: vec![],
            changed_factors: vec![],
            unchanged_factors: vec![],
        };
        let json = serde_json::to_value(&comparison).unwrap();
        assert!(json["differences"].as_array().unwrap().is_empty());
        assert!(json["changed_factors"].as_array().unwrap().is_empty());
    }

    // ============================================================================
    // CounterfactualResponseData Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_response_data_deserialize() {
        let json = json!({
            "summary": "Analysis summary",
            "counterfactual_outcome": "Different result",
            "actual_outcome": "Original result",
            "outcome_delta": 0.4,
            "differences": ["diff1", "diff2"],
            "changed_factors": ["factor1"],
            "unchanged_factors": ["unchanged1", "unchanged2"],
            "causal_attribution": 0.8,
            "confidence": 0.9,
            "insights": ["insight1", "insight2"]
        });
        let response: CounterfactualResponseData = serde_json::from_value(json).unwrap();
        assert_eq!(response.summary, "Analysis summary");
        assert_eq!(response.outcome_delta, 0.4);
        assert_eq!(response.differences.len(), 2);
        assert_eq!(response.causal_attribution, 0.8);
        assert_eq!(response.insights.len(), 2);
    }

    #[test]
    fn test_counterfactual_response_data_serialize() {
        let response = CounterfactualResponseData {
            summary: "Summary".to_string(),
            counterfactual_outcome: "CF".to_string(),
            actual_outcome: "Actual".to_string(),
            outcome_delta: 0.5,
            differences: vec!["d".to_string()],
            changed_factors: vec!["c".to_string()],
            unchanged_factors: vec!["u".to_string()],
            causal_attribution: 0.7,
            confidence: 0.8,
            insights: vec!["i".to_string()],
        };
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["summary"], "Summary");
        assert_eq!(json["causal_attribution"], 0.7);
    }

    // ============================================================================
    // Round-trip Serialization Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_params_round_trip() {
        let original = CounterfactualParams {
            question: "What if X had happened?".to_string(),
            branch_id: "branch-abc".to_string(),
            intervention_type: CounterfactualInterventionType::Inject,
            intervention: "Inject new hypothesis".to_string(),
            target_thought_id: Some("thought-123".to_string()),
            timeline_id: Some("timeline-456".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CounterfactualParams = serde_json::from_str(&json).unwrap();
        assert_eq!(original.question, deserialized.question);
        assert_eq!(original.branch_id, deserialized.branch_id);
        assert_eq!(original.intervention, deserialized.intervention);
        assert_eq!(original.target_thought_id, deserialized.target_thought_id);
    }

    #[test]
    fn test_counterfactual_result_round_trip() {
        let original = CounterfactualResult {
            analysis_id: "cf-1".to_string(),
            question: "What if?".to_string(),
            summary: "Summary".to_string(),
            counterfactual_outcome: "CF outcome".to_string(),
            comparison: CounterfactualComparison {
                actual_outcome: "Actual".to_string(),
                counterfactual_outcome: "CF".to_string(),
                outcome_delta: 0.25,
                differences: vec!["d1".to_string()],
                changed_factors: vec!["c1".to_string()],
                unchanged_factors: vec!["u1".to_string()],
            },
            causal_attribution: 0.65,
            confidence: 0.75,
            counterfactual_branch_id: "cb-1".to_string(),
            insights: vec!["insight".to_string()],
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CounterfactualResult = serde_json::from_str(&json).unwrap();
        assert_eq!(original.analysis_id, deserialized.analysis_id);
        assert_eq!(original.causal_attribution, deserialized.causal_attribution);
        assert_eq!(
            original.comparison.outcome_delta,
            deserialized.comparison.outcome_delta
        );
    }

    // ============================================================================
    // Unicode and Edge Cases Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_params_unicode() {
        let json = json!({
            "question": "如果我们选择了方案B会怎样？🤔",
            "branch_id": "分支-123",
            "intervention": "使用方案B代替方案A"
        });
        let params: CounterfactualParams = serde_json::from_value(json).unwrap();
        assert!(params.question.contains("如果"));
        assert!(params.question.contains("🤔"));
        assert!(params.branch_id.contains("分支"));
    }

    #[test]
    fn test_counterfactual_comparison_negative_delta() {
        let comparison = CounterfactualComparison {
            actual_outcome: "Better".to_string(),
            counterfactual_outcome: "Worse".to_string(),
            outcome_delta: -0.5,
            differences: vec!["Negative impact".to_string()],
            changed_factors: vec![],
            unchanged_factors: vec![],
        };
        let json = serde_json::to_value(&comparison).unwrap();
        assert_eq!(json["outcome_delta"], -0.5);
    }

    #[test]
    fn test_counterfactual_result_many_insights() {
        let insights: Vec<String> = (0..100).map(|i| format!("Insight {}", i)).collect();
        let result = CounterfactualResult {
            analysis_id: "a".to_string(),
            question: "q".to_string(),
            summary: "s".to_string(),
            counterfactual_outcome: "co".to_string(),
            comparison: CounterfactualComparison {
                actual_outcome: "ao".to_string(),
                counterfactual_outcome: "co".to_string(),
                outcome_delta: 0.0,
                differences: vec![],
                changed_factors: vec![],
                unchanged_factors: vec![],
            },
            causal_attribution: 0.5,
            confidence: 0.5,
            counterfactual_branch_id: "cb".to_string(),
            insights,
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["insights"].as_array().unwrap().len(), 100);
    }

    #[test]
    fn test_counterfactual_boundary_values() {
        let result = CounterfactualResult {
            analysis_id: "a".to_string(),
            question: "q".to_string(),
            summary: "s".to_string(),
            counterfactual_outcome: "co".to_string(),
            comparison: CounterfactualComparison {
                actual_outcome: "ao".to_string(),
                counterfactual_outcome: "co".to_string(),
                outcome_delta: 1.0,
                differences: vec![],
                changed_factors: vec![],
                unchanged_factors: vec![],
            },
            causal_attribution: 0.0,
            confidence: 1.0,
            counterfactual_branch_id: "cb".to_string(),
            insights: vec![],
        };
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["causal_attribution"], 0.0);
        assert_eq!(json["confidence"], 1.0);
        assert_eq!(json["comparison"]["outcome_delta"], 1.0);
    }

    #[test]
    fn test_counterfactual_params_long_content() {
        let long_question = "What if ".to_string() + &"A".repeat(10000) + "?";
        let params = CounterfactualParams {
            question: long_question.clone(),
            branch_id: "b".to_string(),
            intervention_type: CounterfactualInterventionType::Change,
            intervention: "B".repeat(5000),
            target_thought_id: None,
            timeline_id: None,
        };
        let json = serde_json::to_value(&params).unwrap();
        assert!(json["question"].as_str().unwrap().len() > 10000);
    }

    // ============================================================================
    // Clone and Debug Tests
    // ============================================================================

    #[test]
    fn test_counterfactual_params_clone() {
        let params = CounterfactualParams {
            question: "Q".to_string(),
            branch_id: "B".to_string(),
            intervention_type: CounterfactualInterventionType::Replace,
            intervention: "I".to_string(),
            target_thought_id: Some("T".to_string()),
            timeline_id: Some("TL".to_string()),
        };
        let cloned = params.clone();
        assert_eq!(params.question, cloned.question);
        assert_eq!(params.branch_id, cloned.branch_id);
        assert_eq!(params.target_thought_id, cloned.target_thought_id);
    }

    #[test]
    fn test_counterfactual_result_clone() {
        let result = CounterfactualResult {
            analysis_id: "a".to_string(),
            question: "q".to_string(),
            summary: "s".to_string(),
            counterfactual_outcome: "co".to_string(),
            comparison: CounterfactualComparison {
                actual_outcome: "ao".to_string(),
                counterfactual_outcome: "co".to_string(),
                outcome_delta: 0.5,
                differences: vec!["d".to_string()],
                changed_factors: vec![],
                unchanged_factors: vec![],
            },
            causal_attribution: 0.5,
            confidence: 0.5,
            counterfactual_branch_id: "cb".to_string(),
            insights: vec!["i".to_string()],
        };
        let cloned = result.clone();
        assert_eq!(result.analysis_id, cloned.analysis_id);
        assert_eq!(result.causal_attribution, cloned.causal_attribution);
        assert_eq!(result.insights, cloned.insights);
    }
}
