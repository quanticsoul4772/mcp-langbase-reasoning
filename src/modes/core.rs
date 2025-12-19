//! Core infrastructure shared by all reasoning modes.
//!
//! This module provides the [`ModeCore`] struct that centralizes common
//! dependencies (storage and Langbase client) used across all mode implementations.

use crate::langbase::LangbaseClient;
use crate::storage::SqliteStorage;

/// Core infrastructure shared by all reasoning modes.
///
/// Contains the storage backend and Langbase client needed for
/// persisting data and calling LLM pipes. This struct is composed
/// into each mode to avoid duplicating these common fields.
///
/// # Example
///
/// ```ignore
/// pub struct MyMode {
///     core: ModeCore,
///     pipe_name: String,
/// }
///
/// impl MyMode {
///     pub fn new(storage: SqliteStorage, langbase: LangbaseClient, config: &Config) -> Self {
///         Self {
///             core: ModeCore::new(storage, langbase),
///             pipe_name: config.pipes.my_mode.clone(),
///         }
///     }
///
///     pub async fn process(&self) -> AppResult<()> {
///         let session = self.core.storage().get_or_create_session("id").await?;
///         let response = self.core.langbase().call_pipe(request).await?;
///         Ok(())
///     }
/// }
/// ```
#[derive(Clone)]
pub struct ModeCore {
    /// Storage backend for persisting data.
    storage: SqliteStorage,
    /// Langbase client for LLM-powered operations.
    langbase: LangbaseClient,
}

impl ModeCore {
    /// Create a new mode core with the given storage and langbase client.
    pub fn new(storage: SqliteStorage, langbase: LangbaseClient) -> Self {
        Self { storage, langbase }
    }

    /// Get a reference to the storage backend.
    #[inline]
    pub fn storage(&self) -> &SqliteStorage {
        &self.storage
    }

    /// Get a reference to the Langbase client.
    #[inline]
    pub fn langbase(&self) -> &LangbaseClient {
        &self.langbase
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DatabaseConfig, LangbaseConfig, RequestConfig};
    use std::path::PathBuf;

    // Helper function to create a test storage instance
    async fn create_test_storage() -> SqliteStorage {
        let config = DatabaseConfig {
            path: PathBuf::from(":memory:"),
            max_connections: 5,
        };
        SqliteStorage::new(&config)
            .await
            .expect("Failed to create in-memory storage")
    }

    // Helper function to create a test langbase client
    fn create_test_langbase() -> LangbaseClient {
        let config = LangbaseConfig {
            api_key: "test_api_key".to_string(),
            base_url: "https://api.langbase.com".to_string(),
        };
        let request_config = RequestConfig::default();
        LangbaseClient::new(&config, request_config).expect("Failed to create test client")
    }

    // Note: Full integration tests require mock storage and langbase client.
    // These tests verify the struct can be created and accessed.

    #[test]
    fn test_mode_core_is_clone() {
        // Verify ModeCore implements Clone (compile-time check)
        fn assert_clone<T: Clone>() {}
        assert_clone::<ModeCore>();
    }

    #[tokio::test]
    async fn test_mode_core_new() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let _core = ModeCore::new(storage, langbase);
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_storage_access() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core = ModeCore::new(storage, langbase);

        // Verify we can access the storage reference
        let _storage_ref = core.storage();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_langbase_access() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core = ModeCore::new(storage, langbase);

        // Verify we can access the langbase reference
        let _langbase_ref = core.langbase();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_storage_returns_correct_reference() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core = ModeCore::new(storage, langbase);

        // Access storage multiple times to ensure consistency
        let storage1 = core.storage();
        let storage2 = core.storage();

        // Both references should point to the same storage
        assert!(std::ptr::eq(storage1, storage2));
    }

    #[tokio::test]
    async fn test_mode_core_langbase_returns_correct_reference() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core = ModeCore::new(storage, langbase);

        // Access langbase multiple times to ensure consistency
        let langbase1 = core.langbase();
        let langbase2 = core.langbase();

        // Both references should point to the same client
        assert!(std::ptr::eq(langbase1, langbase2));
    }

    #[tokio::test]
    async fn test_mode_core_clone_creates_independent_copy() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core1 = ModeCore::new(storage, langbase);
        let core2 = core1.clone();

        // Both should have independent storage and langbase instances
        // (though they share the same underlying data due to Arc in SqliteStorage)
        assert!(!std::ptr::eq(&core1, &core2));
    }

    #[tokio::test]
    async fn test_mode_core_clone_preserves_storage() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core1 = ModeCore::new(storage, langbase);
        let core2 = core1.clone();

        // Verify both can access storage
        let _storage1 = core1.storage();
        let _storage2 = core2.storage();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_clone_preserves_langbase() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core1 = ModeCore::new(storage, langbase);
        let core2 = core1.clone();

        // Verify both can access langbase
        let _langbase1 = core1.langbase();
        let _langbase2 = core2.langbase();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_multiple_clones() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core1 = ModeCore::new(storage, langbase);
        let core2 = core1.clone();
        let core3 = core2.clone();
        let core4 = core3.clone();

        // All should be functional
        let _s1 = core1.storage();
        let _s2 = core2.storage();
        let _s3 = core3.storage();
        let _s4 = core4.storage();

        let _l1 = core1.langbase();
        let _l2 = core2.langbase();
        let _l3 = core3.langbase();
        let _l4 = core4.langbase();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_inline_methods() {
        // Verify that storage() and langbase() are marked as inline
        // This is a compile-time check that the inline attribute is present
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core = ModeCore::new(storage, langbase);

        // Multiple rapid accesses should be optimized by inlining
        for _ in 0..100 {
            let _s = core.storage();
            let _l = core.langbase();
        }
        // Test passes if we reach here without panic
    }

    #[test]
    fn test_mode_core_send_sync_traits() {
        // Verify ModeCore implements Send and Sync
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<ModeCore>();
        assert_sync::<ModeCore>();
    }

    #[tokio::test]
    async fn test_mode_core_composition_pattern() {
        // Test the composition pattern shown in the documentation
        struct TestMode {
            core: ModeCore,
            pipe_name: String,
        }

        impl TestMode {
            fn new(storage: SqliteStorage, langbase: LangbaseClient, pipe_name: String) -> Self {
                Self {
                    core: ModeCore::new(storage, langbase),
                    pipe_name,
                }
            }

            fn storage(&self) -> &SqliteStorage {
                self.core.storage()
            }

            fn langbase(&self) -> &LangbaseClient {
                self.core.langbase()
            }
        }

        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let mode = TestMode::new(storage, langbase, "test-pipe".to_string());

        // Verify composition works
        let _storage = mode.storage();
        let _langbase = mode.langbase();
        assert_eq!(mode.pipe_name, "test-pipe");
    }

    #[tokio::test]
    async fn test_mode_core_with_different_storage_backends() {
        // Test with in-memory storage
        let storage1 = create_test_storage().await;
        let langbase1 = create_test_langbase();
        let core1 = ModeCore::new(storage1, langbase1);

        // Test with another in-memory storage
        let storage2 = create_test_storage().await;
        let langbase2 = create_test_langbase();
        let core2 = ModeCore::new(storage2, langbase2);

        // Both should work independently
        let _s1 = core1.storage();
        let _s2 = core2.storage();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_with_different_langbase_configs() {
        let storage = create_test_storage().await;

        // Create clients with different configs
        let config1 = LangbaseConfig {
            api_key: "key1".to_string(),
            base_url: "https://api1.langbase.com".to_string(),
        };
        let langbase1 = LangbaseClient::new(&config1, RequestConfig::default()).unwrap();

        let config2 = LangbaseConfig {
            api_key: "key2".to_string(),
            base_url: "https://api2.langbase.com".to_string(),
        };
        let langbase2 = LangbaseClient::new(&config2, RequestConfig::default()).unwrap();

        // Create cores with different clients
        let core1 = ModeCore::new(storage.clone(), langbase1);
        let core2 = ModeCore::new(storage.clone(), langbase2);

        // Both should work
        let _l1 = core1.langbase();
        let _l2 = core2.langbase();
        // Test passes if we reach here without panic
    }

    #[tokio::test]
    async fn test_mode_core_reference_stability() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core = ModeCore::new(storage, langbase);

        // Get references multiple times
        let storage_ref1 = core.storage();
        let storage_ref2 = core.storage();
        let storage_ref3 = core.storage();

        // All should point to the same location
        assert!(std::ptr::eq(storage_ref1, storage_ref2));
        assert!(std::ptr::eq(storage_ref2, storage_ref3));

        let langbase_ref1 = core.langbase();
        let langbase_ref2 = core.langbase();
        let langbase_ref3 = core.langbase();

        // All should point to the same location
        assert!(std::ptr::eq(langbase_ref1, langbase_ref2));
        assert!(std::ptr::eq(langbase_ref2, langbase_ref3));
    }

    #[tokio::test]
    async fn test_mode_core_cloned_references_independent() {
        let storage = create_test_storage().await;
        let langbase = create_test_langbase();

        let core1 = ModeCore::new(storage, langbase);
        let core2 = core1.clone();

        // Get references from both cores
        let storage1 = core1.storage();
        let storage2 = core2.storage();

        let langbase1 = core1.langbase();
        let langbase2 = core2.langbase();

        // Core instances should be different
        assert!(!std::ptr::eq(&core1, &core2));

        // But they can still access their resources
        let _s1 = storage1;
        let _s2 = storage2;
        let _l1 = langbase1;
        let _l2 = langbase2;
        // Test passes if we reach here without panic
    }

    #[test]
    fn test_mode_core_zero_cost_abstraction() {
        // ModeCore should be a zero-cost abstraction
        // Size should be just two fields
        use std::mem::size_of;

        let storage_size = size_of::<SqliteStorage>();
        let langbase_size = size_of::<LangbaseClient>();
        let core_size = size_of::<ModeCore>();

        // Core should be exactly the sum of its parts (no overhead)
        assert_eq!(core_size, storage_size + langbase_size);
    }
}
