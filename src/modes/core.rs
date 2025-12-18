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

    // Note: Full integration tests require mock storage and langbase client.
    // These tests verify the struct can be created and accessed.

    #[test]
    fn test_mode_core_is_clone() {
        // Verify ModeCore implements Clone (compile-time check)
        fn assert_clone<T: Clone>() {}
        assert_clone::<ModeCore>();
    }
}
