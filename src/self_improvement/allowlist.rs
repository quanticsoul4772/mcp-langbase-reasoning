//! Action allowlist for the self-improvement system.
//!
//! This module provides compile-time and runtime validation to ensure that
//! only safe, bounded actions can be executed by the self-improvement system.
//!
//! # Safety Guarantees
//!
//! - **Parameter Bounds**: All adjustable parameters have min/max/step limits
//! - **Feature Whitelist**: Only explicitly allowed features can be toggled
//! - **Resource Limits**: Scalable resources have bounded ranges
//! - **Step Constraints**: Maximum change per action is limited

use std::collections::{HashMap, HashSet};

use super::types::{ParamValue, ResourceType, SuggestedAction};

/// Error types for allowlist validation.
#[derive(Debug, thiserror::Error)]
pub enum AllowlistError {
    /// Parameter is not in the allowlist
    #[error("Parameter not in allowlist: {0}")]
    ParamNotAllowed(String),

    /// Feature cannot be toggled
    #[error("Feature not toggleable: {0}")]
    FeatureNotToggleable(String),

    /// Resource cannot be scaled
    #[error("Resource not scalable: {0}")]
    ResourceNotScalable(String),

    /// Value is outside allowed bounds
    #[error("Value {value} out of bounds [{min}, {max}]")]
    ValueOutOfBounds {
        /// The invalid value
        value: i64,
        /// Minimum allowed value
        min: i64,
        /// Maximum allowed value
        max: i64,
    },

    /// Float value is outside allowed bounds
    #[error("Float value {value} out of bounds [{min}, {max}]")]
    FloatValueOutOfBounds {
        /// The invalid value
        value: f64,
        /// Minimum allowed value
        min: f64,
        /// Maximum allowed value
        max: f64,
    },

    /// Change exceeds maximum step size
    #[error("Change {change} exceeds max step {max_step}")]
    StepTooLarge {
        /// The attempted change
        change: i64,
        /// Maximum allowed step
        max_step: i64,
    },

    /// Float change exceeds maximum step size
    #[error("Float change {change} exceeds max step {max_step}")]
    FloatStepTooLarge {
        /// The attempted change
        change: f64,
        /// Maximum allowed step
        max_step: f64,
    },

    /// Type mismatch in parameter value
    #[error("Type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        /// Expected type name
        expected: String,
        /// Actual type name
        actual: String,
    },
}

/// Bounds for a single adjustable parameter.
#[derive(Debug, Clone)]
pub struct ParamBounds {
    /// Current value of the parameter
    pub current_value: ParamValue,
    /// Minimum allowed value
    pub min: ParamValue,
    /// Maximum allowed value
    pub max: ParamValue,
    /// Maximum change per action
    pub step: ParamValue,
    /// Human-readable description
    pub description: String,
}

impl ParamBounds {
    /// Create new integer parameter bounds.
    pub fn integer(current: i64, min: i64, max: i64, step: i64, description: &str) -> Self {
        Self {
            current_value: ParamValue::Integer(current),
            min: ParamValue::Integer(min),
            max: ParamValue::Integer(max),
            step: ParamValue::Integer(step),
            description: description.to_string(),
        }
    }

    /// Create new float parameter bounds.
    pub fn float(current: f64, min: f64, max: f64, step: f64, description: &str) -> Self {
        Self {
            current_value: ParamValue::Float(current),
            min: ParamValue::Float(min),
            max: ParamValue::Float(max),
            step: ParamValue::Float(step),
            description: description.to_string(),
        }
    }

    /// Validate that a value is within bounds.
    pub fn validate_value(&self, value: &ParamValue) -> Result<(), AllowlistError> {
        match (&self.min, &self.max, value) {
            (ParamValue::Integer(min), ParamValue::Integer(max), ParamValue::Integer(v)) => {
                if *v < *min || *v > *max {
                    return Err(AllowlistError::ValueOutOfBounds {
                        value: *v,
                        min: *min,
                        max: *max,
                    });
                }
                Ok(())
            }
            (ParamValue::Float(min), ParamValue::Float(max), ParamValue::Float(v)) => {
                if *v < *min || *v > *max {
                    return Err(AllowlistError::FloatValueOutOfBounds {
                        value: *v,
                        min: *min,
                        max: *max,
                    });
                }
                Ok(())
            }
            _ => Err(AllowlistError::TypeMismatch {
                expected: self.type_name(),
                actual: value_type_name(value),
            }),
        }
    }

    /// Validate that a change is within the step limit.
    pub fn validate_step(
        &self,
        old_value: &ParamValue,
        new_value: &ParamValue,
    ) -> Result<(), AllowlistError> {
        match (&self.step, old_value, new_value) {
            (ParamValue::Integer(max_step), ParamValue::Integer(old), ParamValue::Integer(new)) => {
                let change = (*new - *old).abs();
                if change > *max_step {
                    return Err(AllowlistError::StepTooLarge {
                        change,
                        max_step: *max_step,
                    });
                }
                Ok(())
            }
            (ParamValue::Float(max_step), ParamValue::Float(old), ParamValue::Float(new)) => {
                let change = (*new - *old).abs();
                if change > *max_step {
                    return Err(AllowlistError::FloatStepTooLarge {
                        change,
                        max_step: *max_step,
                    });
                }
                Ok(())
            }
            _ => Err(AllowlistError::TypeMismatch {
                expected: self.type_name(),
                actual: value_type_name(new_value),
            }),
        }
    }

    /// Get the type name for error messages.
    fn type_name(&self) -> String {
        match &self.min {
            ParamValue::Integer(_) => "integer".to_string(),
            ParamValue::Float(_) => "float".to_string(),
            ParamValue::String(_) => "string".to_string(),
            ParamValue::DurationMs(_) => "duration_ms".to_string(),
            ParamValue::Boolean(_) => "boolean".to_string(),
        }
    }

    /// Update the current value.
    pub fn update_current(&mut self, new_value: ParamValue) {
        self.current_value = new_value;
    }
}

/// Get the type name for a ParamValue.
fn value_type_name(value: &ParamValue) -> String {
    match value {
        ParamValue::Integer(_) => "integer".to_string(),
        ParamValue::Float(_) => "float".to_string(),
        ParamValue::String(_) => "string".to_string(),
        ParamValue::DurationMs(_) => "duration_ms".to_string(),
        ParamValue::Boolean(_) => "boolean".to_string(),
    }
}

/// Bounds for scalable resources.
#[derive(Debug, Clone)]
pub struct ResourceBounds {
    /// Minimum allowed value
    pub min: u32,
    /// Maximum allowed value
    pub max: u32,
    /// Maximum change per action
    pub step: u32,
}

impl ResourceBounds {
    /// Create new resource bounds.
    pub fn new(min: u32, max: u32, step: u32) -> Self {
        Self { min, max, step }
    }

    /// Validate a value against bounds.
    pub fn validate_value(&self, value: u32) -> Result<(), AllowlistError> {
        if value < self.min || value > self.max {
            return Err(AllowlistError::ValueOutOfBounds {
                value: value as i64,
                min: self.min as i64,
                max: self.max as i64,
            });
        }
        Ok(())
    }

    /// Validate a change against step limit.
    pub fn validate_step(&self, old_value: u32, new_value: u32) -> Result<(), AllowlistError> {
        let change = (new_value as i32 - old_value as i32).unsigned_abs();
        if change > self.step {
            return Err(AllowlistError::StepTooLarge {
                change: change as i64,
                max_step: self.step as i64,
            });
        }
        Ok(())
    }
}

/// Registry of allowed actions with safe bounds.
///
/// The allowlist serves as a safety boundary between the AI-driven diagnosis
/// and the actual system configuration. Only actions that pass allowlist
/// validation can be executed.
#[derive(Debug, Clone)]
pub struct ActionAllowlist {
    /// Parameters that can be adjusted
    pub adjustable_params: HashMap<String, ParamBounds>,
    /// Features that can be toggled
    pub toggleable_features: HashSet<String>,
    /// Resources that can be scaled
    pub scalable_resources: HashMap<ResourceType, ResourceBounds>,
}

impl Default for ActionAllowlist {
    fn default() -> Self {
        Self::default_allowlist()
    }
}

impl ActionAllowlist {
    /// Create a new empty allowlist.
    pub fn new() -> Self {
        Self {
            adjustable_params: HashMap::new(),
            toggleable_features: HashSet::new(),
            scalable_resources: HashMap::new(),
        }
    }

    /// Create default allowlist based on existing Config structure.
    ///
    /// This provides safe defaults for the self-improvement system to work with.
    pub fn default_allowlist() -> Self {
        let mut params = HashMap::new();

        // REQUEST_TIMEOUT_MS: 5000-60000ms, step 5000ms
        params.insert(
            "REQUEST_TIMEOUT_MS".to_string(),
            ParamBounds::integer(
                30000,
                5000,
                60000,
                5000,
                "HTTP request timeout for Langbase API calls",
            ),
        );

        // MAX_RETRIES: 1-10, step 1
        params.insert(
            "MAX_RETRIES".to_string(),
            ParamBounds::integer(
                3,
                1,
                10,
                1,
                "Maximum retry attempts for failed API calls",
            ),
        );

        // RETRY_DELAY_MS: 500-5000ms, step 500ms
        params.insert(
            "RETRY_DELAY_MS".to_string(),
            ParamBounds::integer(1000, 500, 5000, 500, "Delay between retry attempts"),
        );

        // DATABASE_MAX_CONNECTIONS: 1-50, step 5
        params.insert(
            "DATABASE_MAX_CONNECTIONS".to_string(),
            ParamBounds::integer(5, 1, 50, 5, "Maximum SQLite connection pool size"),
        );

        // Quality thresholds (floats)
        params.insert(
            "REFLECTION_QUALITY_THRESHOLD".to_string(),
            ParamBounds::float(
                0.8,
                0.5,
                0.95,
                0.05,
                "Quality threshold for reflection mode iterations",
            ),
        );

        params.insert(
            "GOT_PRUNE_THRESHOLD".to_string(),
            ParamBounds::float(
                0.3,
                0.1,
                0.7,
                0.1,
                "Score threshold for Graph-of-Thoughts node pruning",
            ),
        );

        // EMA smoothing factor for baselines
        params.insert(
            "SI_EMA_ALPHA".to_string(),
            ParamBounds::float(
                0.1,
                0.05,
                0.3,
                0.05,
                "EMA smoothing factor for baseline calculations",
            ),
        );

        // Warning/critical multipliers
        params.insert(
            "SI_WARNING_MULTIPLIER".to_string(),
            ParamBounds::float(1.5, 1.2, 2.0, 0.1, "Warning threshold multiplier"),
        );

        params.insert(
            "SI_CRITICAL_MULTIPLIER".to_string(),
            ParamBounds::float(2.0, 1.5, 3.0, 0.2, "Critical threshold multiplier"),
        );

        // Toggleable features
        let mut features = HashSet::new();
        features.insert("ENABLE_AUTO_REFLECTION".to_string());
        features.insert("ENABLE_DETECTION_POST_PROCESS".to_string());
        features.insert("ENABLE_GOT_AGGRESSIVE_PRUNING".to_string());
        features.insert("ENABLE_VERBOSE_LOGGING".to_string());
        features.insert("ENABLE_FALLBACK_TRACKING".to_string());
        features.insert("ENABLE_QUALITY_ASSESSMENT".to_string());

        // Scalable resources
        let mut resources = HashMap::new();
        resources.insert(
            ResourceType::MaxConcurrentRequests,
            ResourceBounds::new(1, 20, 2),
        );
        resources.insert(
            ResourceType::ConnectionPoolSize,
            ResourceBounds::new(1, 50, 5),
        );
        resources.insert(ResourceType::CacheSize, ResourceBounds::new(100, 10000, 100));
        resources.insert(ResourceType::TimeoutMs, ResourceBounds::new(5000, 60000, 5000));
        resources.insert(ResourceType::MaxRetries, ResourceBounds::new(1, 10, 1));
        resources.insert(
            ResourceType::RetryDelayMs,
            ResourceBounds::new(500, 5000, 500),
        );

        Self {
            adjustable_params: params,
            toggleable_features: features,
            scalable_resources: resources,
        }
    }

    /// Validate an action against the allowlist.
    ///
    /// Returns `Ok(())` if the action is allowed, or an error describing
    /// why the action was rejected.
    pub fn validate(&self, action: &SuggestedAction) -> Result<(), AllowlistError> {
        match action {
            SuggestedAction::AdjustParam {
                key,
                old_value,
                new_value,
                ..
            } => {
                let bounds = self
                    .adjustable_params
                    .get(key)
                    .ok_or_else(|| AllowlistError::ParamNotAllowed(key.clone()))?;

                bounds.validate_value(new_value)?;
                bounds.validate_step(old_value, new_value)?;
                Ok(())
            }

            SuggestedAction::ToggleFeature { feature_name, .. } => {
                if !self.toggleable_features.contains(feature_name) {
                    return Err(AllowlistError::FeatureNotToggleable(feature_name.clone()));
                }
                Ok(())
            }

            SuggestedAction::ScaleResource {
                resource,
                old_value,
                new_value,
            } => {
                let bounds = self
                    .scalable_resources
                    .get(resource)
                    .ok_or_else(|| AllowlistError::ResourceNotScalable(format!("{:?}", resource)))?;

                bounds.validate_value(*new_value)?;
                bounds.validate_step(*old_value, *new_value)?;
                Ok(())
            }

            // These actions are always allowed (but may have other restrictions)
            SuggestedAction::RestartService { .. } => Ok(()),
            SuggestedAction::ClearCache { .. } => Ok(()),
            SuggestedAction::NoOp { .. } => Ok(()),
        }
    }

    /// Add or update a parameter in the allowlist.
    pub fn add_param(&mut self, key: String, bounds: ParamBounds) {
        self.adjustable_params.insert(key, bounds);
    }

    /// Add a feature to the toggleable list.
    pub fn add_toggleable_feature(&mut self, feature: String) {
        self.toggleable_features.insert(feature);
    }

    /// Add a scalable resource.
    pub fn add_resource(&mut self, resource: ResourceType, bounds: ResourceBounds) {
        self.scalable_resources.insert(resource, bounds);
    }

    /// Get bounds for a parameter.
    pub fn get_param_bounds(&self, key: &str) -> Option<&ParamBounds> {
        self.adjustable_params.get(key)
    }

    /// Get bounds for a resource.
    pub fn get_resource_bounds(&self, resource: &ResourceType) -> Option<&ResourceBounds> {
        self.scalable_resources.get(resource)
    }

    /// Check if a feature is toggleable.
    pub fn is_feature_toggleable(&self, feature: &str) -> bool {
        self.toggleable_features.contains(feature)
    }

    /// Update the current value for a parameter.
    ///
    /// This should be called after an action is executed to keep the
    /// allowlist in sync with actual configuration.
    pub fn update_param_current(&mut self, key: &str, new_value: ParamValue) {
        if let Some(bounds) = self.adjustable_params.get_mut(key) {
            bounds.update_current(new_value);
        }
    }

    /// Get a summary of the allowlist for display.
    pub fn summary(&self) -> AllowlistSummary {
        AllowlistSummary {
            param_count: self.adjustable_params.len(),
            feature_count: self.toggleable_features.len(),
            resource_count: self.scalable_resources.len(),
            param_keys: self.adjustable_params.keys().cloned().collect(),
            features: self.toggleable_features.iter().cloned().collect(),
        }
    }
}

/// Summary of allowlist contents for display.
#[derive(Debug, Clone)]
pub struct AllowlistSummary {
    /// Number of adjustable parameters
    pub param_count: usize,
    /// Number of toggleable features
    pub feature_count: usize,
    /// Number of scalable resources
    pub resource_count: usize,
    /// Keys of all adjustable parameters
    pub param_keys: Vec<String>,
    /// All toggleable features
    pub features: Vec<String>,
}

impl std::fmt::Display for AllowlistSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Action Allowlist:")?;
        writeln!(f, "  Adjustable parameters: {}", self.param_count)?;
        writeln!(f, "  Toggleable features: {}", self.feature_count)?;
        writeln!(f, "  Scalable resources: {}", self.resource_count)?;

        if !self.param_keys.is_empty() {
            writeln!(f, "  Parameters:")?;
            for key in &self.param_keys {
                writeln!(f, "    - {}", key)?;
            }
        }

        if !self.features.is_empty() {
            writeln!(f, "  Features:")?;
            for feature in &self.features {
                writeln!(f, "    - {}", feature)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_improvement::types::ConfigScope;

    fn test_allowlist() -> ActionAllowlist {
        ActionAllowlist::default_allowlist()
    }

    #[test]
    fn test_param_bounds_validation() {
        let bounds = ParamBounds::integer(30000, 5000, 60000, 5000, "Test timeout");

        // Valid value
        assert!(bounds.validate_value(&ParamValue::Integer(40000)).is_ok());

        // Below minimum
        assert!(bounds.validate_value(&ParamValue::Integer(1000)).is_err());

        // Above maximum
        assert!(bounds.validate_value(&ParamValue::Integer(100000)).is_err());

        // Type mismatch
        assert!(bounds.validate_value(&ParamValue::Float(40000.0)).is_err());
    }

    #[test]
    fn test_param_step_validation() {
        let bounds = ParamBounds::integer(30000, 5000, 60000, 5000, "Test timeout");

        // Valid step
        assert!(bounds
            .validate_step(&ParamValue::Integer(30000), &ParamValue::Integer(35000))
            .is_ok());

        // Step too large
        assert!(bounds
            .validate_step(&ParamValue::Integer(30000), &ParamValue::Integer(45000))
            .is_err());
    }

    #[test]
    fn test_float_param_bounds() {
        let bounds = ParamBounds::float(0.8, 0.5, 0.95, 0.05, "Quality threshold");

        // Valid value
        assert!(bounds.validate_value(&ParamValue::Float(0.85)).is_ok());

        // Below minimum
        assert!(bounds.validate_value(&ParamValue::Float(0.3)).is_err());

        // Above maximum
        assert!(bounds.validate_value(&ParamValue::Float(0.99)).is_err());

        // Valid step
        assert!(bounds
            .validate_step(&ParamValue::Float(0.8), &ParamValue::Float(0.85))
            .is_ok());

        // Step too large
        assert!(bounds
            .validate_step(&ParamValue::Float(0.5), &ParamValue::Float(0.9))
            .is_err());
    }

    #[test]
    fn test_resource_bounds_validation() {
        let bounds = ResourceBounds::new(1, 20, 2);

        // Valid value and step
        assert!(bounds.validate_value(10).is_ok());
        assert!(bounds.validate_step(10, 12).is_ok());

        // Invalid value
        assert!(bounds.validate_value(25).is_err());

        // Step too large
        assert!(bounds.validate_step(10, 15).is_err());
    }

    #[test]
    fn test_validate_adjust_param_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::AdjustParam {
            key: "REQUEST_TIMEOUT_MS".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(35000),
            scope: ConfigScope::Runtime,
        };

        assert!(allowlist.validate(&action).is_ok());
    }

    #[test]
    fn test_validate_adjust_param_not_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::AdjustParam {
            key: "UNKNOWN_PARAM".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(35000),
            scope: ConfigScope::Runtime,
        };

        assert!(matches!(
            allowlist.validate(&action),
            Err(AllowlistError::ParamNotAllowed(_))
        ));
    }

    #[test]
    fn test_validate_adjust_param_out_of_bounds() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::AdjustParam {
            key: "REQUEST_TIMEOUT_MS".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(100000), // Exceeds max of 60000
            scope: ConfigScope::Runtime,
        };

        assert!(matches!(
            allowlist.validate(&action),
            Err(AllowlistError::ValueOutOfBounds { .. })
        ));
    }

    #[test]
    fn test_validate_adjust_param_step_too_large() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::AdjustParam {
            key: "REQUEST_TIMEOUT_MS".to_string(),
            old_value: ParamValue::Integer(30000),
            new_value: ParamValue::Integer(50000), // Change of 20000, step limit is 5000
            scope: ConfigScope::Runtime,
        };

        assert!(matches!(
            allowlist.validate(&action),
            Err(AllowlistError::StepTooLarge { .. })
        ));
    }

    #[test]
    fn test_validate_toggle_feature_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::ToggleFeature {
            feature_name: "ENABLE_AUTO_REFLECTION".to_string(),
            desired_state: true,
            reason: "Test".to_string(),
        };

        assert!(allowlist.validate(&action).is_ok());
    }

    #[test]
    fn test_validate_toggle_feature_not_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::ToggleFeature {
            feature_name: "UNKNOWN_FEATURE".to_string(),
            desired_state: true,
            reason: "Test".to_string(),
        };

        assert!(matches!(
            allowlist.validate(&action),
            Err(AllowlistError::FeatureNotToggleable(_))
        ));
    }

    #[test]
    fn test_validate_scale_resource_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::ScaleResource {
            resource: ResourceType::MaxConcurrentRequests,
            old_value: 5,
            new_value: 7, // Change of 2, within step limit
        };

        assert!(allowlist.validate(&action).is_ok());
    }

    #[test]
    fn test_validate_scale_resource_step_too_large() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::ScaleResource {
            resource: ResourceType::MaxConcurrentRequests,
            old_value: 5,
            new_value: 15, // Change of 10, step limit is 2
        };

        assert!(matches!(
            allowlist.validate(&action),
            Err(AllowlistError::StepTooLarge { .. })
        ));
    }

    #[test]
    fn test_validate_no_op_always_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::NoOp {
            reason: "Test".to_string(),
            revisit_after: std::time::Duration::from_secs(300),
        };

        assert!(allowlist.validate(&action).is_ok());
    }

    #[test]
    fn test_validate_restart_service_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::RestartService {
            component: crate::self_improvement::types::ServiceComponent::LangbaseClient,
            graceful: true,
        };

        assert!(allowlist.validate(&action).is_ok());
    }

    #[test]
    fn test_validate_clear_cache_allowed() {
        let allowlist = test_allowlist();

        let action = SuggestedAction::ClearCache {
            cache_name: "sessions".to_string(),
        };

        assert!(allowlist.validate(&action).is_ok());
    }

    #[test]
    fn test_update_param_current() {
        let mut allowlist = test_allowlist();

        let initial = allowlist
            .get_param_bounds("REQUEST_TIMEOUT_MS")
            .unwrap()
            .current_value
            .clone();

        allowlist.update_param_current("REQUEST_TIMEOUT_MS", ParamValue::Integer(40000));

        let updated = allowlist
            .get_param_bounds("REQUEST_TIMEOUT_MS")
            .unwrap()
            .current_value
            .clone();

        assert_ne!(initial, updated);
        assert_eq!(updated, ParamValue::Integer(40000));
    }

    #[test]
    fn test_allowlist_summary() {
        let allowlist = test_allowlist();
        let summary = allowlist.summary();

        assert!(summary.param_count > 0);
        assert!(summary.feature_count > 0);
        assert!(summary.resource_count > 0);
        assert!(!summary.param_keys.is_empty());
        assert!(!summary.features.is_empty());
    }

    #[test]
    fn test_default_allowlist_params() {
        let allowlist = ActionAllowlist::default_allowlist();

        // Check key parameters exist
        assert!(allowlist.get_param_bounds("REQUEST_TIMEOUT_MS").is_some());
        assert!(allowlist.get_param_bounds("MAX_RETRIES").is_some());
        assert!(allowlist.get_param_bounds("RETRY_DELAY_MS").is_some());
        assert!(allowlist
            .get_param_bounds("DATABASE_MAX_CONNECTIONS")
            .is_some());
        assert!(allowlist
            .get_param_bounds("REFLECTION_QUALITY_THRESHOLD")
            .is_some());
        assert!(allowlist.get_param_bounds("GOT_PRUNE_THRESHOLD").is_some());
    }

    #[test]
    fn test_default_allowlist_features() {
        let allowlist = ActionAllowlist::default_allowlist();

        assert!(allowlist.is_feature_toggleable("ENABLE_AUTO_REFLECTION"));
        assert!(allowlist.is_feature_toggleable("ENABLE_VERBOSE_LOGGING"));
        assert!(!allowlist.is_feature_toggleable("NONEXISTENT_FEATURE"));
    }

    #[test]
    fn test_default_allowlist_resources() {
        let allowlist = ActionAllowlist::default_allowlist();

        assert!(allowlist
            .get_resource_bounds(&ResourceType::MaxConcurrentRequests)
            .is_some());
        assert!(allowlist
            .get_resource_bounds(&ResourceType::ConnectionPoolSize)
            .is_some());
        assert!(allowlist
            .get_resource_bounds(&ResourceType::CacheSize)
            .is_some());
    }

    #[test]
    fn test_add_custom_param() {
        let mut allowlist = ActionAllowlist::new();

        allowlist.add_param(
            "CUSTOM_PARAM".to_string(),
            ParamBounds::integer(100, 10, 1000, 50, "Custom parameter"),
        );

        assert!(allowlist.get_param_bounds("CUSTOM_PARAM").is_some());
    }
}
