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
    /// Consolidated pipe name for all detection operations (prompts passed dynamically).
    detection_pipe: String,
}

impl DetectionMode {
    /// Create a new detection mode handler
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
        let detection_pipe = config
            .pipes
            .detection
            .as_ref()
            .and_then(|d| d.pipe.clone())
            .unwrap_or_else(|| "detection-v1".to_string());

        info!(
            pipe = %detection_pipe,
            from_env = config.pipes.detection.as_ref().and_then(|d| d.pipe.as_ref()).is_some(),
            "DetectionMode initialized with pipe"
        );

        Self {
            core: ModeCore::new(storage, langbase),
            detection_pipe,
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
        let request = PipeRequest::new(&self.detection_pipe, messages);
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
        let request = PipeRequest::new(&self.detection_pipe, messages);
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

    #[test]
    fn test_default_true_function() {
        assert!(default_true());
    }

    #[test]
    fn test_detect_biases_params_all_fields() {
        let json = r#"{
            "content": "Test content",
            "thought_id": "thought-123",
            "session_id": "session-456",
            "check_types": ["confirmation_bias"]
        }"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("Test content".to_string()));
        assert_eq!(params.thought_id, Some("thought-123".to_string()));
        assert_eq!(params.session_id, Some("session-456".to_string()));
        assert_eq!(
            params.check_types,
            Some(vec!["confirmation_bias".to_string()])
        );
    }

    #[test]
    fn test_detect_biases_params_empty_check_types() {
        let json = r#"{"content": "Test", "check_types": []}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.check_types, Some(vec![]));
    }

    #[test]
    fn test_detect_biases_params_only_thought_id() {
        let json = r#"{"thought_id": "thought-789"}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert_eq!(params.thought_id, Some("thought-789".to_string()));
        assert!(params.session_id.is_none());
        assert!(params.check_types.is_none());
    }

    #[test]
    fn test_detect_fallacies_params_all_fields() {
        let json = r#"{
            "content": "Test content",
            "thought_id": "thought-123",
            "session_id": "session-456",
            "check_formal": true,
            "check_informal": false
        }"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("Test content".to_string()));
        assert_eq!(params.thought_id, Some("thought-123".to_string()));
        assert_eq!(params.session_id, Some("session-456".to_string()));
        assert!(params.check_formal);
        assert!(!params.check_informal);
    }

    #[test]
    fn test_detect_fallacies_params_only_thought_id() {
        let json = r#"{"thought_id": "thought-789"}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert_eq!(params.thought_id, Some("thought-789".to_string()));
        assert!(params.session_id.is_none());
        assert!(params.check_formal); // default
        assert!(params.check_informal); // default
    }

    #[test]
    fn test_detect_biases_result_round_trip() {
        let original = DetectBiasesResult {
            detections: vec![],
            detection_count: 3,
            analyzed_content_length: 250,
            overall_assessment: Some("Multiple biases detected".to_string()),
            reasoning_quality: Some(0.65),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized["detection_count"], 3);
        assert_eq!(deserialized["analyzed_content_length"], 250);
        assert_eq!(deserialized["reasoning_quality"], 0.65);
    }

    #[test]
    fn test_detect_fallacies_result_round_trip() {
        let original = DetectFallaciesResult {
            detections: vec![],
            detection_count: 1,
            analyzed_content_length: 150,
            overall_assessment: Some("One fallacy detected".to_string()),
            argument_validity: Some(0.9),
        };
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized["detection_count"], 1);
        assert_eq!(deserialized["analyzed_content_length"], 150);
        assert_eq!(deserialized["argument_validity"], 0.9);
    }

    #[test]
    fn test_detect_biases_result_with_none_values() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 50,
            overall_assessment: None,
            reasoning_quality: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("detection_count"));
        assert!(json.contains("\"overall_assessment\":null"));
        assert!(json.contains("\"reasoning_quality\":null"));
    }

    #[test]
    fn test_detect_fallacies_result_with_none_values() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 75,
            overall_assessment: None,
            argument_validity: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("detection_count"));
        assert!(json.contains("\"overall_assessment\":null"));
        assert!(json.contains("\"argument_validity\":null"));
    }

    // ========================================================================
    // Edge Cases & Boundary Values
    // ========================================================================

    #[test]
    fn test_detect_biases_params_empty_content() {
        let json = r#"{"content": ""}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("".to_string()));
    }

    #[test]
    fn test_detect_biases_params_unicode_content() {
        let json = r#"{"content": "测试 🎉 тест"}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("测试 🎉 тест".to_string()));
    }

    #[test]
    fn test_detect_biases_params_long_content() {
        let long_text = "a".repeat(10000);
        let json = serde_json::json!({
            "content": long_text
        });
        let params: DetectBiasesParams = serde_json::from_value(json).unwrap();
        assert_eq!(params.content.unwrap().len(), 10000);
    }

    #[test]
    fn test_detect_biases_params_special_characters() {
        let json = r#"{"content": "Line1\nLine2\t\"quoted\""}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        let content = params.content.unwrap();
        assert!(content.contains("Line1"));
        assert!(content.contains("Line2"));
    }

    #[test]
    fn test_detect_fallacies_params_empty_content() {
        let json = r#"{"content": ""}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("".to_string()));
    }

    #[test]
    fn test_detect_fallacies_params_unicode_content() {
        let json = r#"{"content": "Test 日本語"}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("Test 日本語".to_string()));
    }

    #[test]
    fn test_detect_biases_result_boundary_scores() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 0,
            overall_assessment: Some("".to_string()),
            reasoning_quality: Some(0.0),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("0.0"));

        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 999,
            analyzed_content_length: 999999,
            overall_assessment: Some("Max".to_string()),
            reasoning_quality: Some(1.0),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("1.0"));
        assert!(json.contains("999999"));
    }

    #[test]
    fn test_detect_fallacies_result_boundary_scores() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 0,
            overall_assessment: Some("".to_string()),
            argument_validity: Some(0.0),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("0.0"));

        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 500,
            analyzed_content_length: 100000,
            overall_assessment: Some("Max".to_string()),
            argument_validity: Some(1.0),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("1.0"));
        assert!(json.contains("100000"));
    }

    // ========================================================================
    // Clone & Debug Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_params_clone() {
        let params = DetectBiasesParams {
            content: Some("Test".to_string()),
            thought_id: Some("t1".to_string()),
            session_id: Some("s1".to_string()),
            check_types: Some(vec!["bias1".to_string()]),
        };
        let cloned = params.clone();
        assert_eq!(params.content, cloned.content);
        assert_eq!(params.thought_id, cloned.thought_id);
        assert_eq!(params.session_id, cloned.session_id);
        assert_eq!(params.check_types, cloned.check_types);
    }

    #[test]
    fn test_detect_fallacies_params_clone() {
        let params = DetectFallaciesParams {
            content: Some("Test".to_string()),
            thought_id: Some("t1".to_string()),
            session_id: Some("s1".to_string()),
            check_formal: true,
            check_informal: false,
        };
        let cloned = params.clone();
        assert_eq!(params.content, cloned.content);
        assert_eq!(params.check_formal, cloned.check_formal);
        assert_eq!(params.check_informal, cloned.check_informal);
    }

    #[test]
    fn test_detect_biases_result_clone() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 5,
            analyzed_content_length: 100,
            overall_assessment: Some("Test".to_string()),
            reasoning_quality: Some(0.75),
        };
        let cloned = result.clone();
        assert_eq!(result.detection_count, cloned.detection_count);
        assert_eq!(
            result.analyzed_content_length,
            cloned.analyzed_content_length
        );
        assert_eq!(result.reasoning_quality, cloned.reasoning_quality);
    }

    #[test]
    fn test_detect_fallacies_result_clone() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 3,
            analyzed_content_length: 200,
            overall_assessment: Some("Test".to_string()),
            argument_validity: Some(0.8),
        };
        let cloned = result.clone();
        assert_eq!(result.detection_count, cloned.detection_count);
        assert_eq!(result.argument_validity, cloned.argument_validity);
    }

    #[test]
    fn test_detect_biases_params_debug() {
        let params = DetectBiasesParams {
            content: Some("Test".to_string()),
            thought_id: None,
            session_id: None,
            check_types: None,
        };
        let debug_str = format!("{:?}", params);
        assert!(debug_str.contains("DetectBiasesParams"));
        assert!(debug_str.contains("Test"));
    }

    #[test]
    fn test_detect_fallacies_params_debug() {
        let params = DetectFallaciesParams {
            content: Some("Test".to_string()),
            thought_id: None,
            session_id: None,
            check_formal: true,
            check_informal: true,
        };
        let debug_str = format!("{:?}", params);
        assert!(debug_str.contains("DetectFallaciesParams"));
        assert!(debug_str.contains("Test"));
    }

    #[test]
    fn test_detect_biases_result_debug() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 1,
            analyzed_content_length: 50,
            overall_assessment: Some("Test".to_string()),
            reasoning_quality: Some(0.9),
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("DetectBiasesResult"));
    }

    #[test]
    fn test_detect_fallacies_result_debug() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 2,
            analyzed_content_length: 100,
            overall_assessment: Some("Test".to_string()),
            argument_validity: Some(0.85),
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("DetectFallaciesResult"));
    }

    // ========================================================================
    // JSON Parsing - Invalid/Malformed Input
    // ========================================================================

    #[test]
    fn test_detect_biases_params_invalid_json() {
        let json = r#"{"content": "Test", invalid}"#;
        let result: Result<DetectBiasesParams, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_fallacies_params_invalid_json() {
        let json = r#"{"content": malformed"#;
        let result: Result<DetectFallaciesParams, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_biases_params_missing_all_fields() {
        let json = r#"{}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert!(params.thought_id.is_none());
        assert!(params.session_id.is_none());
        assert!(params.check_types.is_none());
    }

    #[test]
    fn test_detect_fallacies_params_missing_all_optional_fields() {
        let json = r#"{}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert!(params.thought_id.is_none());
        assert!(params.session_id.is_none());
        assert!(params.check_formal); // defaults to true
        assert!(params.check_informal); // defaults to true
    }

    #[test]
    fn test_detect_biases_params_null_values() {
        let json =
            r#"{"content": null, "thought_id": null, "session_id": null, "check_types": null}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert!(params.thought_id.is_none());
        assert!(params.session_id.is_none());
        assert!(params.check_types.is_none());
    }

    #[test]
    fn test_detect_fallacies_params_null_values() {
        let json = r#"{"content": null, "thought_id": null, "session_id": null}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert!(params.thought_id.is_none());
        assert!(params.session_id.is_none());
    }

    // ========================================================================
    // Multiple check_types Variations
    // ========================================================================

    #[test]
    fn test_detect_biases_params_single_check_type() {
        let json = r#"{"content": "Test", "check_types": ["anchoring"]}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.check_types, Some(vec!["anchoring".to_string()]));
    }

    #[test]
    fn test_detect_biases_params_many_check_types() {
        let json =
            r#"{"content": "Test", "check_types": ["bias1", "bias2", "bias3", "bias4", "bias5"]}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.check_types.as_ref().unwrap().len(), 5);
    }

    #[test]
    fn test_detect_biases_params_check_types_with_spaces() {
        let json = r#"{"content": "Test", "check_types": ["confirmation bias", "anchoring bias"]}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(
            params.check_types,
            Some(vec![
                "confirmation bias".to_string(),
                "anchoring bias".to_string()
            ])
        );
    }

    // ========================================================================
    // Serialization Consistency
    // ========================================================================

    #[test]
    fn test_detect_biases_result_serialization_field_names() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 1,
            analyzed_content_length: 100,
            overall_assessment: Some("Test".to_string()),
            reasoning_quality: Some(0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("detections"));
        assert!(json.contains("detection_count"));
        assert!(json.contains("analyzed_content_length"));
        assert!(json.contains("overall_assessment"));
        assert!(json.contains("reasoning_quality"));
    }

    #[test]
    fn test_detect_fallacies_result_serialization_field_names() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 1,
            analyzed_content_length: 100,
            overall_assessment: Some("Test".to_string()),
            argument_validity: Some(0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("detections"));
        assert!(json.contains("detection_count"));
        assert!(json.contains("analyzed_content_length"));
        assert!(json.contains("overall_assessment"));
        assert!(json.contains("argument_validity"));
    }

    // ========================================================================
    // Boolean Combinations for Fallacy Detection
    // ========================================================================

    #[test]
    fn test_detect_fallacies_params_both_false() {
        let json = r#"{"content": "Test", "check_formal": false, "check_informal": false}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(!params.check_formal);
        assert!(!params.check_informal);
    }

    #[test]
    fn test_detect_fallacies_params_only_formal_true() {
        let json = r#"{"content": "Test", "check_formal": true, "check_informal": false}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.check_formal);
        assert!(!params.check_informal);
    }

    #[test]
    fn test_detect_fallacies_params_only_informal_true() {
        let json = r#"{"content": "Test", "check_formal": false, "check_informal": true}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(!params.check_formal);
        assert!(params.check_informal);
    }

    #[test]
    fn test_detect_fallacies_params_both_true() {
        let json = r#"{"content": "Test", "check_formal": true, "check_informal": true}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.check_formal);
        assert!(params.check_informal);
    }

    // ========================================================================
    // Large Data Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_result_large_detection_count() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 1000,
            analyzed_content_length: 50000,
            overall_assessment: Some("Many detections".to_string()),
            reasoning_quality: Some(0.3),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("1000"));
        assert!(json.contains("50000"));
    }

    #[test]
    fn test_detect_fallacies_result_large_detection_count() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 2000,
            analyzed_content_length: 100000,
            overall_assessment: Some("Critical issues".to_string()),
            argument_validity: Some(0.1),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("2000"));
        assert!(json.contains("100000"));
    }

    // ========================================================================
    // Float Precision Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_result_float_precision() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 1,
            analyzed_content_length: 100,
            overall_assessment: Some("Test".to_string()),
            reasoning_quality: Some(0.123456789),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let quality = parsed["reasoning_quality"].as_f64().unwrap();
        assert!((quality - 0.123456789).abs() < 1e-9);
    }

    #[test]
    fn test_detect_fallacies_result_float_precision() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 1,
            analyzed_content_length: 100,
            overall_assessment: Some("Test".to_string()),
            argument_validity: Some(0.987654321),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let validity = parsed["argument_validity"].as_f64().unwrap();
        assert!((validity - 0.987654321).abs() < 1e-9);
    }

    // ========================================================================
    // Default Function Test (already exists but adding more comprehensive check)
    // ========================================================================

    #[test]
    fn test_default_true_is_consistent() {
        assert!(default_true());
        assert_eq!(default_true(), default_true());
    }

    // ========================================================================
    // Empty String Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_params_empty_string_fields() {
        let json = r#"{"content": "", "thought_id": "", "session_id": ""}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("".to_string()));
        assert_eq!(params.thought_id, Some("".to_string()));
        assert_eq!(params.session_id, Some("".to_string()));
    }

    #[test]
    fn test_detect_fallacies_params_empty_string_fields() {
        let json = r#"{"content": "", "thought_id": "", "session_id": ""}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("".to_string()));
        assert_eq!(params.thought_id, Some("".to_string()));
        assert_eq!(params.session_id, Some("".to_string()));
    }

    #[test]
    fn test_detect_biases_result_empty_assessment() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 0,
            overall_assessment: Some("".to_string()),
            reasoning_quality: Some(0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""overall_assessment":"""#));
    }

    #[test]
    fn test_detect_fallacies_result_empty_assessment() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 0,
            overall_assessment: Some("".to_string()),
            argument_validity: Some(0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains(r#""overall_assessment":"""#));
    }

    // ========================================================================
    // Mixed Valid/Invalid Fields
    // ========================================================================

    #[test]
    fn test_detect_biases_params_mixed_some_none() {
        let json =
            r#"{"content": "Test", "thought_id": null, "session_id": "s1", "check_types": null}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("Test".to_string()));
        assert!(params.thought_id.is_none());
        assert_eq!(params.session_id, Some("s1".to_string()));
        assert!(params.check_types.is_none());
    }

    #[test]
    fn test_detect_fallacies_params_mixed_some_none() {
        let json = r#"{"content": null, "thought_id": "t1", "session_id": null}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert!(params.content.is_none());
        assert_eq!(params.thought_id, Some("t1".to_string()));
        assert!(params.session_id.is_none());
    }

    // ========================================================================
    // Negative and Out-of-Range Score Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_result_negative_score() {
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 0,
            overall_assessment: None,
            reasoning_quality: Some(-0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("-0.5"));
    }

    #[test]
    fn test_detect_fallacies_result_over_one_score() {
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 0,
            overall_assessment: None,
            argument_validity: Some(1.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("1.5"));
    }

    // ========================================================================
    // Very Long String Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_result_very_long_assessment() {
        let long_text = "a".repeat(50000);
        let result = DetectBiasesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 50000,
            overall_assessment: Some(long_text),
            reasoning_quality: Some(0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.len() > 50000);
    }

    #[test]
    fn test_detect_fallacies_result_very_long_assessment() {
        let long_text = "b".repeat(30000);
        let result = DetectFallaciesResult {
            detections: vec![],
            detection_count: 0,
            analyzed_content_length: 30000,
            overall_assessment: Some(long_text),
            argument_validity: Some(0.5),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.len() > 30000);
    }

    // ========================================================================
    // Whitespace Tests
    // ========================================================================

    #[test]
    fn test_detect_biases_params_whitespace_content() {
        let json = r#"{"content": "   \n\t   "}"#;
        let params: DetectBiasesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("   \n\t   ".to_string()));
    }

    #[test]
    fn test_detect_fallacies_params_whitespace_content() {
        let json = r#"{"content": "\t\n\r  "}"#;
        let params: DetectFallaciesParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.content, Some("\t\n\r  ".to_string()));
    }

    // ========================================================================
    // Type Mismatch Tests (should fail)
    // ========================================================================

    #[test]
    fn test_detect_biases_params_wrong_type_check_types() {
        let json = r#"{"content": "Test", "check_types": "not_an_array"}"#;
        let result: Result<DetectBiasesParams, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_fallacies_params_wrong_type_check_formal() {
        let json = r#"{"content": "Test", "check_formal": "true"}"#;
        let result: Result<DetectFallaciesParams, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_detect_biases_result_wrong_type_detection_count() {
        let json = r#"{"detections": [], "detection_count": "5", "analyzed_content_length": 100}"#;
        let result: Result<serde_json::Value, _> = serde_json::from_str(json);
        assert!(result.is_ok()); // JSON parsing succeeds but won't match struct type
    }
}
