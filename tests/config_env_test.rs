//! Config environment variable tests
//!
//! These tests verify that Config::from_env() correctly reads and applies
//! environment variable overrides. Note that Config::from_env() also loads
//! from .env file via dotenvy, so these tests focus on override behavior.
//!
//! Tests use #[serial] to prevent race conditions with shared env vars.

use mcp_langbase_reasoning::config::{Config, LogFormat};
use serial_test::serial;
use std::env;

#[test]
#[serial]
fn test_config_from_env_loads_successfully() {
    // Config::from_env() should succeed when LANGBASE_API_KEY is available
    // (either from env var or .env file)
    let result = Config::from_env();
    // If there's a .env file with the key, this succeeds
    // This test verifies the function works in the project environment
    assert!(
        result.is_ok(),
        "Config::from_env() should succeed with .env file present"
    );
}

#[test]
#[serial]
fn test_config_from_env_custom_base_url() {
    env::set_var("LANGBASE_BASE_URL", "https://custom.api.com");

    let config = Config::from_env().unwrap();
    assert_eq!(config.langbase.base_url, "https://custom.api.com");

    // Restore default
    env::set_var("LANGBASE_BASE_URL", "https://api.langbase.com");
}

#[test]
#[serial]
fn test_config_from_env_custom_database() {
    env::set_var("DATABASE_PATH", "/custom/path.db");
    env::set_var("DATABASE_MAX_CONNECTIONS", "10");

    let config = Config::from_env().unwrap();
    assert_eq!(config.database.path.to_str().unwrap(), "/custom/path.db");
    assert_eq!(config.database.max_connections, 10);

    // Restore defaults
    env::set_var("DATABASE_PATH", "./data/reasoning.db");
    env::set_var("DATABASE_MAX_CONNECTIONS", "5");
}

#[test]
#[serial]
fn test_config_from_env_json_log_format() {
    env::set_var("LOG_FORMAT", "json");

    let config = Config::from_env().unwrap();
    assert_eq!(config.logging.format, LogFormat::Json);

    // Restore default
    env::set_var("LOG_FORMAT", "pretty");
}

#[test]
#[serial]
fn test_config_from_env_custom_request() {
    env::set_var("REQUEST_TIMEOUT_MS", "60000");
    env::set_var("MAX_RETRIES", "5");
    env::set_var("RETRY_DELAY_MS", "2000");

    let config = Config::from_env().unwrap();
    assert_eq!(config.request.timeout_ms, 60000);
    assert_eq!(config.request.max_retries, 5);
    assert_eq!(config.request.retry_delay_ms, 2000);

    // Restore defaults
    env::set_var("REQUEST_TIMEOUT_MS", "30000");
    env::set_var("MAX_RETRIES", "3");
    env::set_var("RETRY_DELAY_MS", "1000");
}

#[test]
#[serial]
fn test_config_from_env_custom_pipes() {
    env::set_var("PIPE_LINEAR", "custom-linear-v2");
    env::set_var("PIPE_TREE", "custom-tree-v2");

    let config = Config::from_env().unwrap();
    assert_eq!(config.pipes.linear, "custom-linear-v2");
    assert_eq!(config.pipes.tree, "custom-tree-v2");

    // Restore defaults
    env::remove_var("PIPE_LINEAR");
    env::remove_var("PIPE_TREE");
}

#[test]
#[serial]
fn test_config_invalid_number_uses_default() {
    env::set_var("DATABASE_MAX_CONNECTIONS", "not-a-number");

    let config = Config::from_env().unwrap();
    // Should fall back to default
    assert_eq!(config.database.max_connections, 5);

    // Restore default
    env::set_var("DATABASE_MAX_CONNECTIONS", "5");
}
