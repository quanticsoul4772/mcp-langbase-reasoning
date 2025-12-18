//! Detection mode - bias and fallacy detection in reasoning.
//!
//! This module provides cognitive bias and logical fallacy detection:
//! - Bias detection (confirmation bias, anchoring, etc.)
//! - Fallacy detection (formal and informal)
//! - Storage persistence for detected issues
//! - Integration with thought analysis

use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::info;

use super::ModeCore;
use crate::config::Config;
use crate::error::{AppResult, ToolError};
use crate::langbase::{
    BiasDetectionResponse, FallacyDetectionResponse, LangbaseClient, Message, PipeRequest,
};
use crate::prompts::{BIAS_DETECTION_PROMPT, FALLACY_DETECTION_PROMPT};
use crate::storage::{Detection, DetectionType, SqliteStorage, Storage};

// ============================================================================
// Bias Detection
// ============================================================================

/// Parameters for bias detection
#[derive(Debug, Clone, Deserialize)]
pub struct DetectBiasesParams {
    /// Content to analyze for biases
    pub content: Option<String>,
    /// ID of an existing thought to analyze
    pub thought_id: Option<String>,
    /// Session ID for context and persistence
    pub session_id: Option<String>,
    /// Specific bias types to check (optional)
    pub check_types: Option<Vec<String>>,
}

/// Result of bias detection
#[derive(Debug, Clone, Serialize)]
pub struct DetectBiasesResult {
    /// Detected biases
    pub detections: Vec<Detection>,
    /// Number of detections
    pub detection_count: usize,
    /// Length of analyzed content
    pub analyzed_content_length: usize,
    /// Overall assessment
    pub overall_assessment: Option<String>,
    /// Reasoning quality score (0.0-1.0)
    pub reasoning_quality: Option<f64>,
}

// ============================================================================
// Fallacy Detection
// ============================================================================

/// Parameters for fallacy detection
#[derive(Debug, Clone, Deserialize)]
pub struct DetectFallaciesParams {
    /// Content to analyze for fallacies
    pub content: Option<String>,
    /// ID of an existing thought to analyze
    pub thought_id: Option<String>,
    /// Session ID for context and persistence
    pub session_id: Option<String>,
    /// Check for formal logical fallacies (default: true)
    #[serde(default = "default_true")]
    pub check_formal: bool,
    /// Check for informal logical fallacies (default: true)
    #[serde(default = "default_true")]
    pub check_informal: bool,
}

fn default_true() -> bool {
    true
}

/// Result of fallacy detection
#[derive(Debug, Clone, Serialize)]
pub struct DetectFallaciesResult {
    /// Detected fallacies
    pub detections: Vec<Detection>,
    /// Number of detections
    pub detection_count: usize,
    /// Length of analyzed content
    pub analyzed_content_length: usize,
    /// Overall assessment
    pub overall_assessment: Option<String>,
    /// Argument validity score (0.0-1.0)
    pub argument_validity: Option<f64>,
}

// ============================================================================
// Detection Mode
// ============================================================================

/// Detection mode handler for bias and fallacy detection.
#[derive(Clone)]
pub struct DetectionMode {
    /// Core infrastructure (storage and langbase client).
    core: ModeCore,
    /// Pipe name for bias detection.
    bias_pipe: String,
    /// Pipe name for fallacy detection.
    fallacy_pipe: String,
}

impl DetectionMode {
    /// Create a new detection mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        let bias_pipe = config
            .pipes
            .detection
            .as_ref()
            .and_then(|d| d.bias_pipe.clone())
            .unwrap_or_else(|| "detect-biases-v1".to_string());

        let fallacy_pipe = config
            .pipes
            .detection
            .as_ref()
            .and_then(|d| d.fallacy_pipe.clone())
            .unwrap_or_else(|| "detect-fallacies-v1".to_string());

        Self {
            core: ModeCore::new(storage, langbase),
            bias_pipe,
            fallacy_pipe,
        }
    }

    /// Detect biases in content or a thought
    pub async fn detect_biases(&self, params: DetectBiasesParams) -> AppResult<DetectBiasesResult> {
        let start = Instant::now();

        // Validate and resolve content
        let (analysis_content, thought_id) = self
            .resolve_content(
                params.content.as_deref(),
                params.thought_id.as_deref(),
                "detect_biases",
            )
            .await?;

        // Build messages for Langbase
        let mut messages = vec![Message::system(BIAS_DETECTION_PROMPT)];

        // Add specific bias types to check if provided
        if let Some(check_types) = &params.check_types {
            if !check_types.is_empty() {
                messages.push(Message::user(format!(
                    "Focus specifically on detecting these bias types: {}\n\nContent to analyze:\n{}",
                    check_types.join(", "),
                    analysis_content
                )));
            } else {
                messages.push(Message::user(format!(
                    "Analyze the following content for cognitive biases:\n\n{}",
                    analysis_content
                )));
            }
        } else {
            messages.push(Message::user(format!(
                "Analyze the following content for cognitive biases:\n\n{}",
                analysis_content
            )));
        }

        // Call Langbase pipe
        let request = PipeRequest::new(&self.bias_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let bias_response = BiasDetectionResponse::from_completion(&response.completion);

        // Convert to Detection structs and persist
        let mut detections = Vec::new();
        for detected in &bias_response.detections {
            let mut detection = Detection::new(
                DetectionType::Bias,
                &detected.bias_type,
                detected.severity,
                detected.confidence,
                &detected.explanation,
            );

            if let Some(session_id) = &params.session_id {
                detection = detection.with_session(session_id);
            }
            if let Some(tid) = &thought_id {
                detection = detection.with_thought(tid);
            }
            if let Some(remediation) = &detected.remediation {
                detection = detection.with_remediation(remediation);
            }
            if let Some(excerpt) = &detected.excerpt {
                detection = detection.with_metadata(serde_json::json!({ "excerpt": excerpt }));
            }

            // Persist to storage
            self.core.storage().create_detection(&detection).await?;
            detections.push(detection);
        }

        let latency = start.elapsed().as_millis();
        info!(
            detection_count = detections.len(),
            latency_ms = latency,
            "Bias detection completed"
        );

        Ok(DetectBiasesResult {
            detections,
            detection_count: bias_response.detections.len(),
            analyzed_content_length: analysis_content.len(),
            overall_assessment: Some(bias_response.overall_assessment),
            reasoning_quality: Some(bias_response.reasoning_quality),
        })
    }

    /// Detect fallacies in content or a thought
    pub async fn detect_fallacies(
        &self,
        params: DetectFallaciesParams,
    ) -> AppResult<DetectFallaciesResult> {
        let start = Instant::now();

        // Validate check flags
        if !params.check_formal && !params.check_informal {
            return Err(ToolError::Validation {
                field: "check_formal/check_informal".to_string(),
                reason: "At least one of check_formal or check_informal must be true".to_string(),
            }
            .into());
        }

        // Validate and resolve content
        let (analysis_content, thought_id) = self
            .resolve_content(
                params.content.as_deref(),
                params.thought_id.as_deref(),
                "detect_fallacies",
            )
            .await?;

        info!(
            check_formal = %params.check_formal,
            check_informal = %params.check_informal,
            "Detecting fallacies"
        );

        // Build messages for Langbase
        let mut messages = vec![Message::system(FALLACY_DETECTION_PROMPT)];

        // Build instruction based on what types to check
        let check_instruction = match (params.check_formal, params.check_informal) {
            (true, true) => "Check for both formal and informal logical fallacies.",
            (true, false) => "Focus only on formal logical fallacies (structural errors).",
            (false, true) => "Focus only on informal logical fallacies (content/context errors).",
            (false, false) => unreachable!(), // Already validated above
        };

        messages.push(Message::user(format!(
            "{}\n\nContent to analyze:\n{}",
            check_instruction, analysis_content
        )));

        // Call Langbase pipe
        let request = PipeRequest::new(&self.fallacy_pipe, messages);
        let response = self.core.langbase().call_pipe(request).await?;

        // Parse response
        let fallacy_response = FallacyDetectionResponse::from_completion(&response.completion);

        // Convert to Detection structs and persist
        let mut detections = Vec::new();
        for detected in &fallacy_response.detections {
            // Filter based on check_formal/check_informal params
            let is_formal = detected.category.to_lowercase() == "formal";
            if (is_formal && !params.check_formal) || (!is_formal && !params.check_informal) {
                continue;
            }

            let mut detection = Detection::new(
                DetectionType::Fallacy,
                &detected.fallacy_type,
                detected.severity,
                detected.confidence,
                &detected.explanation,
            );

            if let Some(session_id) = &params.session_id {
                detection = detection.with_session(session_id);
            }
            if let Some(tid) = &thought_id {
                detection = detection.with_thought(tid);
            }
            if let Some(remediation) = &detected.remediation {
                detection = detection.with_remediation(remediation);
            }

            // Store category and excerpt in metadata
            let mut meta = serde_json::Map::new();
            meta.insert("category".to_string(), serde_json::json!(detected.category));
            if let Some(excerpt) = &detected.excerpt {
                meta.insert("excerpt".to_string(), serde_json::json!(excerpt));
            }
            detection = detection.with_metadata(serde_json::Value::Object(meta));

            // Persist to storage
            self.core.storage().create_detection(&detection).await?;
            detections.push(detection);
        }

        let latency = start.elapsed().as_millis();
        info!(
            detection_count = detections.len(),
            latency_ms = latency,
            "Fallacy detection completed"
        );

        Ok(DetectFallaciesResult {
            detections,
            detection_count: fallacy_response.detections.len(),
            analyzed_content_length: analysis_content.len(),
            overall_assessment: Some(fallacy_response.overall_assessment),
            argument_validity: Some(fallacy_response.argument_validity),
        })
    }

    /// Resolve content from either direct content or thought ID
    async fn resolve_content(
        &self,
        content: Option<&str>,
        thought_id: Option<&str>,
        operation: &str,
    ) -> AppResult<(String, Option<String>)> {
        match (content, thought_id) {
            (Some(content), _) => Ok((content.to_string(), thought_id.map(|s| s.to_string()))),
            (None, Some(thought_id)) => {
                let thought = self
                    .core
                    .storage()
                    .get_thought(thought_id)
                    .await?
                    .ok_or_else(|| {
                        ToolError::Session(format!("Thought not found: {}", thought_id))
                    })?;
                Ok((thought.content, Some(thought_id.to_string())))
            }
            (None, None) => Err(ToolError::Validation {
                field: "content/thought_id".to_string(),
                reason: format!(
                    "Either 'content' or 'thought_id' must be provided for {}",
                    operation
                ),
            }
            .into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_biases_params_deserialize() {
        let json = r#"{"content": "Test content"}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("Test content".to_string()));
        assert!(params.thought_id.is_none());
        assert!(params.session_id.is_none());
        assert!(params.check_types.is_none());
    }

    #[test]
    fn test_detect_biases_params_with_check_types() {
        let json = r#"{"content": "Test", "check_types": ["confirmation_bias", "anchoring"]}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(
            params.check_types,
            Some(vec![
                "confirmation_bias".to_string(),
                "anchoring".to_string()
            ])
        );
    }

    #[test]
    fn test_detect_fallacies_params_defaults() {
        let json = r#"{"content": "Test content"}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.check_formal);
        assert!(params.check_informal);
    }

    #[test]
    fn test_detect_fallacies_params_custom_checks() {
        let json = r#"{"content": "Test", "check_formal": false, "check_informal": true}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(!params.check_formal);
        assert!(params.check_informal);
    }

    #[test]
    fn test_detect_biases_result_serialize() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 100,
            overall_assessment: Some("Good reasoning".to_string()),
            reasoning_quality: Some(0.85),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("detection_count"));
        assert!(json.contains("0.85"));
    }

    #[test]
    fn test_detect_fallacies_result_serialize() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 2,
            analyzed_content_length: 200,
            overall_assessment: Some("Some issues found".to_string()),
            argument_validity: Some(0.7),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("detection_count"));
        assert!(json.contains("0.7"));
    }
}
