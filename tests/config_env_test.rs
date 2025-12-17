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

#[test]
#[serial]
fn test_config_from_env_got_config_partial() {
    // Set only some GoT env vars - should create GotPipeConfig with those values
    env::set_var("PIPE_GOT_GENERATE", "custom-got-generate");
    env::set_var("GOT_MAX_NODES", "50");

    let config = Config::from_env().unwrap();

    // Should create GotPipeConfig because at least one value is set
    let got = config.pipes.got.expect("GotPipeConfig should be Some");
    assert_eq!(got.generate_pipe, Some("custom-got-generate".to_string()));
    assert_eq!(got.max_nodes, Some(50));
    // Other values should be None since not set
    assert!(got.score_pipe.is_none());
    assert!(got.aggregate_pipe.is_none());
    assert!(got.refine_pipe.is_none());
    assert!(got.max_depth.is_none());
    assert!(got.default_k.is_none());
    assert!(got.prune_threshold.is_none());

    // Restore defaults
    env::remove_var("PIPE_GOT_GENERATE");
    env::remove_var("GOT_MAX_NODES");
}

#[test]
#[serial]
fn test_config_from_env_got_config_full() {
    // Set all GoT env vars
    env::set_var("PIPE_GOT_GENERATE", "got-gen-v2");
    env::set_var("PIPE_GOT_SCORE", "got-score-v2");
    env::set_var("PIPE_GOT_AGGREGATE", "got-agg-v2");
    env::set_var("PIPE_GOT_REFINE", "got-refine-v2");
    env::set_var("GOT_MAX_NODES", "200");
    env::set_var("GOT_MAX_DEPTH", "20");
    env::set_var("GOT_DEFAULT_K", "5");
    env::set_var("GOT_PRUNE_THRESHOLD", "0.5");

    let config = Config::from_env().unwrap();

    let got = config.pipes.got.expect("GotPipeConfig should be Some");
    assert_eq!(got.generate_pipe, Some("got-gen-v2".to_string()));
    assert_eq!(got.score_pipe, Some("got-score-v2".to_string()));
    assert_eq!(got.aggregate_pipe, Some("got-agg-v2".to_string()));
    assert_eq!(got.refine_pipe, Some("got-refine-v2".to_string()));
    assert_eq!(got.max_nodes, Some(200));
    assert_eq!(got.max_depth, Some(20));
    assert_eq!(got.default_k, Some(5));
    assert_eq!(got.prune_threshold, Some(0.5));

    // Cleanup
    env::remove_var("PIPE_GOT_GENERATE");
    env::remove_var("PIPE_GOT_SCORE");
    env::remove_var("PIPE_GOT_AGGREGATE");
    env::remove_var("PIPE_GOT_REFINE");
    env::remove_var("GOT_MAX_NODES");
    env::remove_var("GOT_MAX_DEPTH");
    env::remove_var("GOT_DEFAULT_K");
    env::remove_var("GOT_PRUNE_THRESHOLD");
}

#[test]
#[serial]
fn test_config_from_env_optional_pipes() {
    // Test PIPE_AUTO and PIPE_BACKTRACKING
    env::set_var("PIPE_AUTO", "custom-auto-v1");
    env::set_var("PIPE_BACKTRACKING", "backtrack-v1");

    let config = Config::from_env().unwrap();

    assert_eq!(config.pipes.auto, Some("custom-auto-v1".to_string()));
    assert_eq!(config.pipes.backtracking, Some("backtrack-v1".to_string()));
    // auto_router uses PIPE_AUTO as well
    assert_eq!(config.pipes.auto_router, "custom-auto-v1");

    // Cleanup
    env::remove_var("PIPE_AUTO");
    env::remove_var("PIPE_BACKTRACKING");
}

#[test]
#[serial]
fn test_config_from_env_log_level() {
    env::set_var("LOG_LEVEL", "debug");

    let config = Config::from_env().unwrap();
    assert_eq!(config.logging.level, "debug");

    // Restore default
    env::set_var("LOG_LEVEL", "info");
}
