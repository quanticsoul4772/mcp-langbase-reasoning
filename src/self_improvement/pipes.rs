//! Langbase pipe integration for the self-improvement system.
//!
//! This module provides a specialized interface to Langbase pipes for the
//! self-improvement loop. It uses existing pipes with tailored prompts:
//!
//! - **reflection-v1**: Diagnosis generation and learning synthesis
//! - **decision-framework-v1**: Action selection with multi-criteria analysis
//! - **detection-v1**: Decision validation (bias/fallacy detection)
//!
//! # Strategy: Existing Pipes First
//!
//! Rather than creating new specialized pipes, we use existing pipes with
//! carefully crafted prompts. This approach:
//! - Avoids pipe proliferation
//! - Reuses proven reasoning patterns
//! - Allows gradual specialization based on metrics

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::error::LangbaseError;
use crate::langbase::{LangbaseClient, Message, PipeRequest, PipeResponse};

use super::allowlist::ActionAllowlist;
use super::config::SelfImprovementPipeConfig;
use super::types::{HealthReport, MetricsSnapshot, NormalizedReward, SelfDiagnosis, SuggestedAction};

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during pipe operations.
#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    /// Pipe call failed due to network or API error
    #[error("Pipe '{pipe}' unavailable: {message}")]
    Unavailable {
        /// Name of the pipe
        pipe: String,
        /// Error message
        message: String,
        /// Whether a fallback was used
        fallback_used: bool,
    },

    /// Pipe call timed out
    #[error("Pipe '{pipe}' timed out after {timeout_ms}ms")]
    Timeout {
        /// Name of the pipe
        pipe: String,
        /// Timeout in milliseconds
        timeout_ms: u64,
    },

    /// Failed to parse pipe response
    #[error("Failed to parse response from '{pipe}': {error}")]
    ParseFailed {
        /// Name of the pipe
        pipe: String,
        /// Parse error details
        error: String,
    },

    /// Langbase client error
    #[error("Langbase error: {0}")]
    Langbase(#[from] LangbaseError),
}

impl PipeError {
    /// Check if this error indicates the pipe is unavailable.
    pub fn is_unavailable(&self) -> bool {
        matches!(self, PipeError::Unavailable { .. } | PipeError::Timeout { .. })
    }
}

// ============================================================================
// Response Types
// ============================================================================

/// Response from the diagnosis pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosisResponse {
    /// Root cause analysis
    pub suspected_cause: String,
    /// Severity assessment: info, warning, high, critical
    pub severity: String,
    /// Confidence in the diagnosis (0.0-1.0)
    pub confidence: f64,
    /// Supporting evidence
    pub evidence: Vec<String>,
    /// Recommended action type
    pub recommended_action_type: String,
    /// Target for the action (parameter name, feature name, etc.)
    pub action_target: Option<String>,
    /// Rationale for the recommendation
    pub rationale: String,
}

impl Default for DiagnosisResponse {
    fn default() -> Self {
        Self {
            suspected_cause: "Unable to determine cause".to_string(),
            severity: "info".to_string(),
            confidence: 0.0,
            evidence: vec![],
            recommended_action_type: "no_op".to_string(),
            action_target: None,
            rationale: "Diagnosis unavailable".to_string(),
        }
    }
}

/// Action scores from the decision framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionScores {
    /// Effectiveness score
    pub effectiveness: f64,
    /// Risk score (lower = safer)
    pub risk: f64,
    /// Reversibility score
    pub reversibility: f64,
    /// Historical success rate
    pub historical_success: f64,
}

/// Response from the action selection pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSelectionResponse {
    /// Selected action option
    pub selected_option: String,
    /// Scores for the selected action
    pub scores: ActionScores,
    /// Total composite score
    pub total_score: f64,
    /// Rationale for selection
    pub rationale: String,
    /// Other options that were considered
    pub alternatives_considered: Vec<String>,
}

impl Default for ActionSelectionResponse {
    fn default() -> Self {
        Self {
            selected_option: "no_op".to_string(),
            scores: ActionScores {
                effectiveness: 0.0,
                risk: 1.0,
                reversibility: 0.0,
                historical_success: 0.0,
            },
            total_score: 0.0,
            rationale: "Action selection unavailable".to_string(),
            alternatives_considered: vec![],
        }
    }
}

/// Bias/fallacy detection from validation pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasDetection {
    /// Type of bias detected
    pub bias_type: String,
    /// Severity (1-5)
    pub severity: i32,
    /// Explanation
    pub explanation: String,
}

/// Fallacy detection from validation pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallacyDetection {
    /// Type of fallacy detected
    pub fallacy_type: String,
    /// Severity (1-5)
    pub severity: i32,
    /// Explanation
    pub explanation: String,
}

/// Response from the validation pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    /// Detected biases
    pub biases_detected: Vec<BiasDetection>,
    /// Detected fallacies
    pub fallacies_detected: Vec<FallacyDetection>,
    /// Overall quality score (0.0-1.0)
    pub overall_quality: f64,
    /// Whether to proceed with the action
    pub should_proceed: bool,
    /// Warnings to log
    pub warnings: Vec<String>,
}

impl Default for ValidationResponse {
    fn default() -> Self {
        Self {
            biases_detected: vec![],
            fallacies_detected: vec![],
            overall_quality: 0.5,
            should_proceed: false, // Default to safe behavior
            warnings: vec!["Validation unavailable - defaulting to safe behavior".to_string()],
        }
    }
}

/// Recommendations from the learning synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningRecommendations {
    /// Whether to adjust the allowlist
    pub adjust_allowlist: bool,
    /// Parameters to adjust
    pub param_adjustments: Vec<ParamAdjustment>,
    /// Whether to adjust cooldown
    pub adjust_cooldown: bool,
    /// New cooldown value if adjusting
    pub new_cooldown_secs: Option<u64>,
}

/// Suggested parameter adjustment from learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamAdjustment {
    /// Parameter key
    pub key: String,
    /// Direction: "increase" or "decrease"
    pub direction: String,
    /// Reason for the adjustment
    pub reason: String,
}

/// Response from the learning synthesis pipe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningResponse {
    /// Assessment of the outcome
    pub outcome_assessment: String,
    /// Accuracy of the root cause analysis (0.0-1.0)
    pub root_cause_accuracy: f64,
    /// Effectiveness of the action (0.0-1.0)
    pub action_effectiveness: f64,
    /// Key lessons learned
    pub lessons: Vec<String>,
    /// Recommendations for future actions
    pub recommendations: LearningRecommendations,
    /// Confidence in the analysis (0.0-1.0)
    pub confidence: f64,
}

impl Default for LearningResponse {
    fn default() -> Self {
        Self {
            outcome_assessment: "Learning synthesis unavailable".to_string(),
            root_cause_accuracy: 0.0,
            action_effectiveness: 0.0,
            lessons: vec![],
            recommendations: LearningRecommendations {
                adjust_allowlist: false,
                param_adjustments: vec![],
                adjust_cooldown: false,
                new_cooldown_secs: None,
            },
            confidence: 0.0,
        }
    }
}

// ============================================================================
// Pipe Metrics
// ============================================================================

/// Metrics for a single pipe call.
#[derive(Debug, Clone)]
pub struct PipeCallMetrics {
    /// Which pipe was called
    pub pipe_name: String,
    /// Latency in milliseconds
    pub latency_ms: u64,
    /// Whether parsing succeeded
    pub parse_success: bool,
    /// Whether the call succeeded overall
    pub call_success: bool,
}

// ============================================================================
// Self-Improvement Pipes
// ============================================================================

/// Self-improvement pipe operations using existing Langbase pipes.
///
/// This struct provides specialized methods for each phase of the
/// self-improvement loop, using existing pipes with tailored prompts.
#[derive(Clone)]
pub struct SelfImprovementPipes {
    langbase: Arc<LangbaseClient>,
    config: SelfImprovementPipeConfig,
}

impl SelfImprovementPipes {
    /// Create a new SelfImprovementPipes instance.
    pub fn new(langbase: Arc<LangbaseClient>, config: SelfImprovementPipeConfig) -> Self {
        Self { langbase, config }
    }

    /// Generate a diagnosis using reflection-v1 pipe.
    ///
    /// This analyzes the health report and trigger to determine root cause
    /// and recommend an action.
    pub async fn generate_diagnosis(
        &self,
        health_report: &HealthReport,
        trigger: &super::types::TriggerMetric,
    ) -> Result<(DiagnosisResponse, PipeCallMetrics), PipeError> {
        let prompt = self.build_diagnosis_prompt(health_report, trigger);
        let start = Instant::now();

        let response = self
            .call_pipe_with_timeout(&self.config.diagnosis_pipe, prompt)
            .await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        let diagnosis = self.parse_diagnosis_response(&response)?;

        let metrics = PipeCallMetrics {
            pipe_name: self.config.diagnosis_pipe.clone(),
            latency_ms,
            parse_success: true,
            call_success: true,
        };

        Ok((diagnosis, metrics))
    }

    /// Select an action using decision-framework-v1 pipe.
    ///
    /// This evaluates the diagnosis against the allowlist and historical
    /// effectiveness to select the best action.
    pub async fn select_action(
        &self,
        diagnosis: &SelfDiagnosis,
        allowlist: &ActionAllowlist,
        history: &[ActionEffectiveness],
    ) -> Result<(ActionSelectionResponse, PipeCallMetrics), PipeError> {
        let prompt = self.build_action_selection_prompt(diagnosis, allowlist, history);
        let start = Instant::now();

        let response = self
            .call_pipe_with_timeout(&self.config.decision_pipe, prompt)
            .await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        let selection = self.parse_action_selection_response(&response)?;

        let metrics = PipeCallMetrics {
            pipe_name: self.config.decision_pipe.clone(),
            latency_ms,
            parse_success: true,
            call_success: true,
        };

        Ok((selection, metrics))
    }

    /// Validate a decision using detection-v1 pipe.
    ///
    /// This checks for cognitive biases and logical fallacies in the
    /// diagnosis and action selection process.
    pub async fn validate_decision(
        &self,
        diagnosis: &SelfDiagnosis,
        action: &SuggestedAction,
    ) -> Result<(ValidationResponse, PipeCallMetrics), PipeError> {
        if !self.config.enable_validation {
            // Validation disabled - return default approval
            return Ok((
                ValidationResponse {
                    biases_detected: vec![],
                    fallacies_detected: vec![],
                    overall_quality: 1.0,
                    should_proceed: true,
                    warnings: vec![],
                },
                PipeCallMetrics {
                    pipe_name: self.config.detection_pipe.clone(),
                    latency_ms: 0,
                    parse_success: true,
                    call_success: true,
                },
            ));
        }

        let prompt = self.build_validation_prompt(diagnosis, action);
        let start = Instant::now();

        let response = self
            .call_pipe_with_timeout(&self.config.detection_pipe, prompt)
            .await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        let validation = self.parse_validation_response(&response)?;

        let metrics = PipeCallMetrics {
            pipe_name: self.config.detection_pipe.clone(),
            latency_ms,
            parse_success: true,
            call_success: true,
        };

        Ok((validation, metrics))
    }

    /// Synthesize learning from an executed action using reflection-v1 pipe.
    ///
    /// This analyzes the before/after metrics to extract lessons and
    /// recommendations for future improvements.
    pub async fn synthesize_learning(
        &self,
        action: &SuggestedAction,
        diagnosis: &SelfDiagnosis,
        pre_metrics: &MetricsSnapshot,
        post_metrics: &MetricsSnapshot,
        reward: &NormalizedReward,
    ) -> Result<(LearningResponse, PipeCallMetrics), PipeError> {
        let prompt =
            self.build_learning_prompt(action, diagnosis, pre_metrics, post_metrics, reward);
        let start = Instant::now();

        let response = self
            .call_pipe_with_timeout(&self.config.learning_pipe, prompt)
            .await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        let learning = self.parse_learning_response(&response)?;

        let metrics = PipeCallMetrics {
            pipe_name: self.config.learning_pipe.clone(),
            latency_ms,
            parse_success: true,
            call_success: true,
        };

        Ok((learning, metrics))
    }

    // ========================================================================
    // Internal: Pipe Calls
    // ========================================================================

    async fn call_pipe_with_timeout(
        &self,
        pipe_name: &str,
        prompt: String,
    ) -> Result<PipeResponse, PipeError> {
        let timeout = Duration::from_millis(self.config.pipe_timeout_ms);

        let messages = vec![Message::user(prompt)];
        let request = PipeRequest::new(pipe_name, messages);

        debug!(pipe = %pipe_name, "Calling self-improvement pipe");

        match tokio::time::timeout(timeout, self.langbase.call_pipe(request)).await {
            Ok(Ok(response)) => {
                info!(pipe = %pipe_name, "Self-improvement pipe call succeeded");
                Ok(response)
            }
            Ok(Err(e)) => {
                error!(pipe = %pipe_name, error = %e, "Self-improvement pipe call failed");
                Err(PipeError::Unavailable {
                    pipe: pipe_name.to_string(),
                    message: e.to_string(),
                    fallback_used: false,
                })
            }
            Err(_) => {
                warn!(pipe = %pipe_name, timeout_ms = self.config.pipe_timeout_ms, "Self-improvement pipe call timed out");
                Err(PipeError::Timeout {
                    pipe: pipe_name.to_string(),
                    timeout_ms: self.config.pipe_timeout_ms,
                })
            }
        }
    }

    // ========================================================================
    // Internal: Prompt Building
    // ========================================================================

    fn build_diagnosis_prompt(
        &self,
        health_report: &HealthReport,
        trigger: &super::types::TriggerMetric,
    ) -> String {
        let trigger_json =
            serde_json::to_string_pretty(trigger).unwrap_or_else(|_| format!("{:?}", trigger));
        let baselines_json = serde_json::to_string_pretty(&health_report.baselines)
            .unwrap_or_else(|_| "{}".to_string());
        let metrics_json = serde_json::to_string_pretty(&health_report.current_metrics)
            .unwrap_or_else(|_| "{}".to_string());

        format!(
            r#"## Self-Improvement System Diagnosis Request

### Context
You are analyzing system health data to diagnose issues and recommend actions.
This is an autonomous self-improvement system for an MCP reasoning server.

### Trigger Event
```json
{trigger_json}
```

### Current Metrics
```json
{metrics_json}
```

### Baseline Values
```json
{baselines_json}
```

### Task
Analyze this data and provide a diagnosis. Respond with a JSON object:

```json
{{
  "suspected_cause": "Root cause analysis - what is likely causing this trigger",
  "severity": "info|warning|high|critical",
  "confidence": 0.0-1.0,
  "evidence": ["evidence point 1", "evidence point 2"],
  "recommended_action_type": "adjust_param|toggle_feature|scale_resource|restart_service|clear_cache|no_op",
  "action_target": "parameter or feature name if applicable",
  "rationale": "Why this action would help"
}}
```

Focus on:
1. Identifying the most likely root cause
2. Recommending safe, reversible actions
3. Providing clear rationale"#
        )
    }

    fn build_action_selection_prompt(
        &self,
        diagnosis: &SelfDiagnosis,
        allowlist: &ActionAllowlist,
        history: &[ActionEffectiveness],
    ) -> String {
        let diagnosis_json =
            serde_json::to_string_pretty(diagnosis).unwrap_or_else(|_| "{}".to_string());
        let allowlist_summary = format!("{}", allowlist.summary());
        let history_json =
            serde_json::to_string_pretty(history).unwrap_or_else(|_| "[]".to_string());

        format!(
            r#"## Self-Improvement Action Selection

### Diagnosis
```json
{diagnosis_json}
```

### Available Actions (Allowlist)
{allowlist_summary}

### Historical Effectiveness
```json
{history_json}
```

### Task
Select the best action from the allowlist. Respond with a JSON object:

```json
{{
  "selected_option": "action type and target",
  "scores": {{
    "effectiveness": 0.0-1.0,
    "risk": 0.0-1.0,
    "reversibility": 0.0-1.0,
    "historical_success": 0.0-1.0
  }},
  "total_score": 0.0-1.0,
  "rationale": "Why this action is the best choice",
  "alternatives_considered": ["other options that were evaluated"]
}}
```

Important:
1. Only select actions within the allowlist bounds
2. Prefer reversible actions
3. Consider historical success rates
4. Balance effectiveness against risk"#
        )
    }

    fn build_validation_prompt(
        &self,
        diagnosis: &SelfDiagnosis,
        action: &SuggestedAction,
    ) -> String {
        let diagnosis_json =
            serde_json::to_string_pretty(diagnosis).unwrap_or_else(|_| "{}".to_string());
        let action_json =
            serde_json::to_string_pretty(action).unwrap_or_else(|_| "{}".to_string());

        format!(
            r#"## Self-Improvement Decision Validation

### Diagnosis
```json
{diagnosis_json}
```

### Proposed Action
```json
{action_json}
```

### Task
Validate this diagnosis and action for cognitive biases and logical fallacies.
Respond with a JSON object:

```json
{{
  "biases_detected": [
    {{"bias_type": "name", "severity": 1-5, "explanation": "why this is a concern"}}
  ],
  "fallacies_detected": [
    {{"fallacy_type": "name", "severity": 1-5, "explanation": "why this is a concern"}}
  ],
  "overall_quality": 0.0-1.0,
  "should_proceed": true/false,
  "warnings": ["any important caveats"]
}}
```

Check for:
1. Confirmation bias (only seeing supporting evidence)
2. Anchoring bias (over-relying on first data point)
3. Hasty generalization (insufficient samples)
4. False cause fallacy (correlation != causation)
5. Bandwagon fallacy (because it worked before)"#
        )
    }

    fn build_learning_prompt(
        &self,
        action: &SuggestedAction,
        diagnosis: &SelfDiagnosis,
        pre_metrics: &MetricsSnapshot,
        post_metrics: &MetricsSnapshot,
        reward: &NormalizedReward,
    ) -> String {
        let action_json =
            serde_json::to_string_pretty(action).unwrap_or_else(|_| "{}".to_string());
        let diagnosis_json =
            serde_json::to_string_pretty(diagnosis).unwrap_or_else(|_| "{}".to_string());
        let pre_json =
            serde_json::to_string_pretty(pre_metrics).unwrap_or_else(|_| "{}".to_string());
        let post_json =
            serde_json::to_string_pretty(post_metrics).unwrap_or_else(|_| "{}".to_string());
        let reward_json =
            serde_json::to_string_pretty(reward).unwrap_or_else(|_| "{}".to_string());

        format!(
            r#"## Self-Improvement Learning Synthesis

### Original Diagnosis
```json
{diagnosis_json}
```

### Executed Action
```json
{action_json}
```

### Metrics Before
```json
{pre_json}
```

### Metrics After
```json
{post_json}
```

### Calculated Reward
```json
{reward_json}
```

### Task
Synthesize learning from this action execution. Respond with a JSON object:

```json
{{
  "outcome_assessment": "summary of what happened",
  "root_cause_accuracy": 0.0-1.0,
  "action_effectiveness": 0.0-1.0,
  "lessons": ["lesson 1", "lesson 2"],
  "recommendations": {{
    "adjust_allowlist": true/false,
    "param_adjustments": [
      {{"key": "param name", "direction": "increase|decrease", "reason": "why"}}
    ],
    "adjust_cooldown": true/false,
    "new_cooldown_secs": null or number
  }},
  "confidence": 0.0-1.0
}}
```

Focus on:
1. Was the root cause diagnosis accurate?
2. Did the action have the intended effect?
3. What can we learn for future actions?
4. Should we adjust any parameters or thresholds?"#
        )
    }

    // ========================================================================
    // Internal: Response Parsing
    // ========================================================================

    fn parse_diagnosis_response(
        &self,
        response: &PipeResponse,
    ) -> Result<DiagnosisResponse, PipeError> {
        let json_str = extract_json(&response.completion);

        serde_json::from_str::<DiagnosisResponse>(&json_str).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %response.completion.chars().take(200).collect::<String>(),
                "Failed to parse diagnosis response"
            );
            PipeError::ParseFailed {
                pipe: self.config.diagnosis_pipe.clone(),
                error: e.to_string(),
            }
        })
    }

    fn parse_action_selection_response(
        &self,
        response: &PipeResponse,
    ) -> Result<ActionSelectionResponse, PipeError> {
        let json_str = extract_json(&response.completion);

        serde_json::from_str::<ActionSelectionResponse>(&json_str).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %response.completion.chars().take(200).collect::<String>(),
                "Failed to parse action selection response"
            );
            PipeError::ParseFailed {
                pipe: self.config.decision_pipe.clone(),
                error: e.to_string(),
            }
        })
    }

    fn parse_validation_response(
        &self,
        response: &PipeResponse,
    ) -> Result<ValidationResponse, PipeError> {
        let json_str = extract_json(&response.completion);

        serde_json::from_str::<ValidationResponse>(&json_str).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %response.completion.chars().take(200).collect::<String>(),
                "Failed to parse validation response"
            );
            PipeError::ParseFailed {
                pipe: self.config.detection_pipe.clone(),
                error: e.to_string(),
            }
        })
    }

    fn parse_learning_response(
        &self,
        response: &PipeResponse,
    ) -> Result<LearningResponse, PipeError> {
        let json_str = extract_json(&response.completion);

        serde_json::from_str::<LearningResponse>(&json_str).map_err(|e| {
            warn!(
                error = %e,
                completion_preview = %response.completion.chars().take(200).collect::<String>(),
                "Failed to parse learning response"
            );
            PipeError::ParseFailed {
                pipe: self.config.learning_pipe.clone(),
                error: e.to_string(),
            }
        })
    }

    /// Get the configuration.
    pub fn config(&self) -> &SelfImprovementPipeConfig {
        &self.config
    }
}

// ============================================================================
// Helper Types
// ============================================================================

/// Historical effectiveness data for an action type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEffectiveness {
    /// Action type
    pub action_type: String,
    /// Action signature (for grouping similar actions)
    pub action_signature: String,
    /// Total attempts
    pub total_attempts: u32,
    /// Successful attempts
    pub successful_attempts: u32,
    /// Average reward
    pub avg_reward: f64,
    /// Effectiveness score
    pub effectiveness_score: f64,
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Extract JSON from a completion that may contain markdown code blocks.
fn extract_json(completion: &str) -> String {
    // Try to find JSON in markdown code block
    if let Some(start) = completion.find("```json") {
        if let Some(end) = completion[start + 7..].find("```") {
            return completion[start + 7..start + 7 + end].trim().to_string();
        }
    }

    // Try to find JSON in generic code block
    if let Some(start) = completion.find("```") {
        let after_start = &completion[start + 3..];
        // Skip language identifier if present
        let json_start = after_start.find('\n').map(|n| n + 1).unwrap_or(0);
        if let Some(end) = after_start[json_start..].find("```") {
            return after_start[json_start..json_start + end].trim().to_string();
        }
    }

    // Try to find JSON object directly
    if let Some(start) = completion.find('{') {
        if let Some(end) = completion.rfind('}') {
            if end > start {
                return completion[start..=end].to_string();
            }
        }
    }

    // Return original if no JSON found
    completion.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_from_markdown() {
        let completion = r#"Here is my analysis:

```json
{
  "suspected_cause": "High error rate",
  "severity": "warning"
}
```

That's my diagnosis."#;

        let json = extract_json(completion);
        assert!(json.contains("suspected_cause"));
        assert!(json.contains("High error rate"));
    }

    #[test]
    fn test_extract_json_from_code_block() {
        let completion = r#"```
{
  "key": "value"
}
```"#;

        let json = extract_json(completion);
        assert!(json.contains("key"));
    }

    #[test]
    fn test_extract_json_direct() {
        let completion = r#"{"key": "value"}"#;

        let json = extract_json(completion);
        assert_eq!(json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_with_text() {
        let completion = r#"Here is some text before {"key": "value"} and after"#;

        let json = extract_json(completion);
        assert_eq!(json, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_diagnosis_response_default() {
        let response = DiagnosisResponse::default();
        assert_eq!(response.recommended_action_type, "no_op");
        assert_eq!(response.confidence, 0.0);
    }

    #[test]
    fn test_validation_response_default() {
        let response = ValidationResponse::default();
        assert!(!response.should_proceed);
        assert_eq!(response.warnings.len(), 1);
    }

    #[test]
    fn test_learning_response_default() {
        let response = LearningResponse::default();
        assert!(!response.recommendations.adjust_allowlist);
        assert_eq!(response.confidence, 0.0);
    }

    #[test]
    fn test_pipe_error_is_unavailable() {
        let unavailable = PipeError::Unavailable {
            pipe: "test".to_string(),
            message: "failed".to_string(),
            fallback_used: false,
        };
        assert!(unavailable.is_unavailable());

        let timeout = PipeError::Timeout {
            pipe: "test".to_string(),
            timeout_ms: 30000,
        };
        assert!(timeout.is_unavailable());

        let parse_failed = PipeError::ParseFailed {
            pipe: "test".to_string(),
            error: "invalid json".to_string(),
        };
        assert!(!parse_failed.is_unavailable());
    }

    #[test]
    fn test_diagnosis_response_serialization() {
        let response = DiagnosisResponse {
            suspected_cause: "High latency".to_string(),
            severity: "warning".to_string(),
            confidence: 0.85,
            evidence: vec!["P95 increased by 50%".to_string()],
            recommended_action_type: "adjust_param".to_string(),
            action_target: Some("REQUEST_TIMEOUT_MS".to_string()),
            rationale: "Increase timeout to handle slow responses".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: DiagnosisResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.suspected_cause, response.suspected_cause);
        assert_eq!(parsed.confidence, response.confidence);
    }

    #[test]
    fn test_action_effectiveness_serialization() {
        let effectiveness = ActionEffectiveness {
            action_type: "adjust_param".to_string(),
            action_signature: "REQUEST_TIMEOUT_MS:increase".to_string(),
            total_attempts: 5,
            successful_attempts: 4,
            avg_reward: 0.3,
            effectiveness_score: 0.8,
        };

        let json = serde_json::to_string(&effectiveness).unwrap();
        let parsed: ActionEffectiveness = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.action_type, effectiveness.action_type);
        assert_eq!(parsed.effectiveness_score, effectiveness.effectiveness_score);
    }
}
