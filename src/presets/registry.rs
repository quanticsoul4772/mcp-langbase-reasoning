//! Preset registry for managing workflow presets.

use std::collections::HashMap;
use std::sync::RwLock;

use tracing::error;

use super::builtins;
use super::types::{PresetSummary, WorkflowPreset};

/// Registry for workflow presets.
///
/// Thread-safe storage for preset definitions with built-in presets
/// automatically registered on creation.
pub struct PresetRegistry {
    presets: RwLock<HashMap<String, WorkflowPreset>>,
}

impl PresetRegistry {
    /// Create a new registry with built-in presets.
    pub fn new() -> Self {
        let registry = Self {
            presets: RwLock::new(HashMap::new()),
        };
        registry.register_builtins();
        registry
    }

    /// Register a preset.
    ///
    /// # Errors
    /// Returns error if a preset with the same ID already exists.
    pub fn register(&self, preset: WorkflowPreset) -> Result<(), String> {
        if preset.id.is_empty() {
            return Err("Preset ID is required".to_string());
        }
        if preset.name.is_empty() {
            return Err("Preset name is required".to_string());
        }
        if preset.steps.is_empty() {
            return Err("Preset must have at least one step".to_string());
        }

        let mut presets = self.presets.write().unwrap();
        if presets.contains_key(&preset.id) {
            return Err(format!("Preset '{}' already exists", preset.id));
        }

        presets.insert(preset.id.clone(), preset);
        Ok(())
    }

    /// Get a preset by ID.
    pub fn get(&self, id: &str) -> Option<WorkflowPreset> {
        self.presets.read().unwrap().get(id).cloned()
    }

    /// List all presets, optionally filtered by category.
    pub fn list(&self, category: Option<&str>) -> Vec<PresetSummary> {
        self.presets
            .read()
            .unwrap()
            .values()
            .filter(|p| category.is_none() || Some(p.category.as_str()) == category)
            .map(|p| p.to_summary())
            .collect()
    }

    /// Get all unique categories.
    pub fn categories(&self) -> Vec<String> {
        let presets = self.presets.read().unwrap();
        let mut cats: Vec<_> = presets.values().map(|p| p.category.clone()).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    /// Get the number of registered presets.
    pub fn count(&self) -> usize {
        self.presets.read().unwrap().len()
    }

    fn register_builtins(&self) {
        let presets = [
            ("code-review", builtins::code_review_preset()),
            ("debug-analysis", builtins::debug_analysis_preset()),
            (
                "architecture-decision",
                builtins::architecture_decision_preset(),
            ),
            ("strategic-decision", builtins::strategic_decision_preset()),
            (
                "evidence-based-conclusion",
                builtins::evidence_based_conclusion_preset(),
            ),
        ];

        for (name, preset) in presets {
            if let Err(e) = self.register(preset) {
                error!(
                    preset = name,
                    error = %e,
                    "Failed to register builtin preset - this indicates a programming error"
                );
            }
        }
    }
}

impl Default for PresetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::PresetStep;

    fn test_preset(id: &str) -> WorkflowPreset {
        WorkflowPreset {
            id: id.to_string(),
            name: format!("Test {}", id),
            description: "A test preset".to_string(),
            category: "testing".to_string(),
            steps: vec![PresetStep::new("step1", "reasoning_linear")],
            input_schema: HashMap::new(),
            output_format: "json".to_string(),
            estimated_time: "1 minute".to_string(),
            tags: vec![],
        }
    }

    #[test]
    fn test_registry_new_has_builtins() {
        let registry = PresetRegistry::new();
        assert!(registry.count() >= 5);
        assert!(registry.get("code-review").is_some());
        assert!(registry.get("debug-analysis").is_some());
        assert!(registry.get("architecture-decision").is_some());
        assert!(registry.get("strategic-decision").is_some());
        assert!(registry.get("evidence-based-conclusion").is_some());
    }

    #[test]
    fn test_registry_register_and_get() {
        let registry = PresetRegistry::new();
        let preset = test_preset("custom");

        registry.register(preset).unwrap();
        let retrieved = registry.get("custom").unwrap();
        assert_eq!(retrieved.id, "custom");
    }

    #[test]
    fn test_registry_duplicate_fails() {
        let registry = PresetRegistry::new();
        let preset1 = test_preset("dup");
        let preset2 = test_preset("dup");

        registry.register(preset1).unwrap();
        let result = registry.register(preset2);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[test]
    fn test_registry_validation() {
        let registry = PresetRegistry::new();

        // Empty ID
        let mut preset = test_preset("test");
        preset.id = String::new();
        assert!(registry.register(preset).is_err());

        // Empty name
        let mut preset = test_preset("test2");
        preset.name = String::new();
        assert!(registry.register(preset).is_err());

        // No steps
        let mut preset = test_preset("test3");
        preset.steps = vec![];
        assert!(registry.register(preset).is_err());
    }

    #[test]
    fn test_registry_list_all() {
        let registry = PresetRegistry::new();
        let presets = registry.list(None);
        assert!(presets.len() >= 5);
    }

    #[test]
    fn test_registry_list_by_category() {
        let registry = PresetRegistry::new();
        let code_presets = registry.list(Some("code"));
        assert!(code_presets.len() >= 2);
        assert!(code_presets.iter().all(|p| p.category == "code"));

        let arch_presets = registry.list(Some("architecture"));
        assert!(arch_presets.len() >= 1);
        assert!(arch_presets.iter().all(|p| p.category == "architecture"));
    }

    #[test]
    fn test_registry_categories() {
        let registry = PresetRegistry::new();
        let categories = registry.categories();
        assert!(categories.contains(&"code".to_string()));
        assert!(categories.contains(&"architecture".to_string()));
        assert!(categories.contains(&"decision".to_string()));
        assert!(categories.contains(&"research".to_string()));
    }

    #[test]
    fn test_registry_list_by_decision_category() {
        let registry = PresetRegistry::new();
        let decision_presets = registry.list(Some("decision"));
        assert!(decision_presets.len() >= 1);
        assert!(decision_presets.iter().all(|p| p.category == "decision"));
    }

    #[test]
    fn test_registry_list_by_research_category() {
        let registry = PresetRegistry::new();
        let research_presets = registry.list(Some("research"));
        assert!(research_presets.len() >= 1);
        assert!(research_presets.iter().all(|p| p.category == "research"));
    }

    #[test]
    fn test_registry_default() {
        let registry = PresetRegistry::default();
        assert!(registry.count() >= 5);
    }

    #[test]
    fn test_registry_get_nonexistent() {
        let registry = PresetRegistry::new();
        assert!(registry.get("nonexistent-preset-id").is_none());
    }

    #[test]
    fn test_registry_list_nonexistent_category() {
        let registry = PresetRegistry::new();
        let presets = registry.list(Some("nonexistent-category"));
        assert!(presets.is_empty());
    }
}
