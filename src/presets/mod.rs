//! Workflow preset system for composing reasoning modes into higher-level workflows.
//!
//! This module provides:
//! - `WorkflowPreset`: Definition of multi-step reasoning workflows
//! - `PresetRegistry`: Registration and lookup of presets
//! - `execute_preset`: Workflow execution engine
//! - Built-in presets for common tasks

mod builtins;
mod executor;
mod registry;
mod types;

pub use builtins::*;
pub use executor::execute_preset;
pub use registry::PresetRegistry;
pub use types::*;
