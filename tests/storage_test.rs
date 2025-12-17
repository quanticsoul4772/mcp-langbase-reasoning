//! Integration tests for SQLite storage layer
//!
//! Tests database operations using an in-memory SQLite database.

use chrono::Utc;
use serde_json::json;

use mcp_langbase_reasoning::storage::{Invocation, Session, SqliteStorage, Storage, Thought};

/// Create an in-memory storage instance for testing
async fn create_test_storage() -> SqliteStorage {
    SqliteStorage::new_in_memory()
        .await
        .expect("Failed to create in-memory storage")
}

#[cfg(test)]
mod session_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_session() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        let result = storage.create_session(&session).await;

        assert!(result.is_ok(), "Should create session successfully");
    }

    #[tokio::test]
    async fn test_get_session() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let retrieved = storage.get_session(&session.id).await.unwrap();

        assert!(retrieved.is_some(), "Session should exist");
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, session.id);
        assert_eq!(retrieved.mode, "linear");
    }

    #[tokio::test]
    async fn test_get_nonexistent_session() {
        let storage = create_test_storage().await;

        let result = storage.get_session("nonexistent-id").await.unwrap();

        assert!(result.is_none(), "Should return None for nonexistent session");
    }

    #[tokio::test]
    async fn test_update_session() {
        let storage = create_test_storage().await;

        let mut session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        session.mode = "tree".to_string();
        session.updated_at = Utc::now();

        let result = storage.update_session(&session).await;
        assert!(result.is_ok());

        let retrieved = storage.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(retrieved.mode, "tree");
    }

    #[tokio::test]
    async fn test_delete_session() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        storage.delete_session(&session.id).await.unwrap();

        let result = storage.get_session(&session.id).await.unwrap();
        assert!(result.is_none(), "Session should be deleted");
    }

    #[tokio::test]
    async fn test_session_with_metadata() {
        let storage = create_test_storage().await;

        let mut session = Session::new("linear");
        session.metadata = Some(json!({
            "user": "test",
            "context": "integration-test"
        }));

        storage.create_session(&session).await.unwrap();

        let retrieved = storage.get_session(&session.id).await.unwrap().unwrap();
        assert!(retrieved.metadata.is_some());

        let metadata = retrieved.metadata.unwrap();
        assert_eq!(metadata["user"], "test");
    }
}

#[cfg(test)]
mod thought_tests {
    use super::*;

    #[tokio::test]
    async fn test_create_thought() {
        let storage = create_test_storage().await;

        // Create session first
        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        // Create thought
        let thought = Thought::new(&session.id, "Test reasoning content", "linear");
        let result = storage.create_thought(&thought).await;

        assert!(result.is_ok(), "Should create thought successfully");
    }

    #[tokio::test]
    async fn test_get_thought() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let thought = Thought::new(&session.id, "Test content", "linear").with_confidence(0.9);
        storage.create_thought(&thought).await.unwrap();

        let retrieved = storage.get_thought(&thought.id).await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, thought.id);
        assert_eq!(retrieved.content, "Test content");
        assert!((retrieved.confidence - 0.9).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_get_session_thoughts_ordered() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        // Create multiple thoughts
        let thought1 = Thought::new(&session.id, "First thought", "linear");
        storage.create_thought(&thought1).await.unwrap();

        // Small delay to ensure different timestamps
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let thought2 = Thought::new(&session.id, "Second thought", "linear");
        storage.create_thought(&thought2).await.unwrap();

        let thoughts = storage.get_session_thoughts(&session.id).await.unwrap();

        assert_eq!(thoughts.len(), 2);
        assert_eq!(thoughts[0].content, "First thought");
        assert_eq!(thoughts[1].content, "Second thought");
    }

    #[tokio::test]
    async fn test_get_latest_thought() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let thought1 = Thought::new(&session.id, "Old thought", "linear");
        storage.create_thought(&thought1).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let thought2 = Thought::new(&session.id, "Latest thought", "linear");
        storage.create_thought(&thought2).await.unwrap();

        let latest = storage.get_latest_thought(&session.id).await.unwrap();

        assert!(latest.is_some());
        assert_eq!(latest.unwrap().content, "Latest thought");
    }

    #[tokio::test]
    async fn test_thought_with_parent() {
        let storage = create_test_storage().await;

        let session = Session::new("tree");
        storage.create_session(&session).await.unwrap();

        let parent = Thought::new(&session.id, "Parent thought", "tree");
        storage.create_thought(&parent).await.unwrap();

        let child = Thought::new(&session.id, "Child thought", "tree").with_parent(&parent.id);
        storage.create_thought(&child).await.unwrap();

        let retrieved = storage.get_thought(&child.id).await.unwrap().unwrap();
        assert_eq!(retrieved.parent_id, Some(parent.id));
    }

    #[tokio::test]
    async fn test_thought_confidence_clamping() {
        let thought = Thought::new("session-1", "Test", "linear")
            .with_confidence(1.5); // Over 1.0

        assert!((thought.confidence - 1.0).abs() < 0.001, "Confidence should be clamped to 1.0");

        let thought2 = Thought::new("session-1", "Test", "linear")
            .with_confidence(-0.5); // Under 0.0

        assert!((thought2.confidence - 0.0).abs() < 0.001, "Confidence should be clamped to 0.0");
    }

    #[tokio::test]
    async fn test_thought_with_metadata() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let thought = Thought::new(&session.id, "Analyzed data", "linear").with_metadata(json!({
            "sources": ["doc1", "doc2"],
            "analysis_type": "comparative"
        }));

        storage.create_thought(&thought).await.unwrap();

        let retrieved = storage.get_thought(&thought.id).await.unwrap().unwrap();
        let metadata = retrieved.metadata.unwrap();
        assert_eq!(metadata["analysis_type"], "comparative");
    }
}

#[cfg(test)]
mod invocation_tests {
    use super::*;

    #[tokio::test]
    async fn test_log_successful_invocation() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let invocation = Invocation::new("reasoning.linear", json!({"content": "test"}))
            .with_session(&session.id)
            .with_pipe("linear-reasoning-v1")
            .success(json!({"thought": "result"}), 150);

        let result = storage.log_invocation(&invocation).await;

        assert!(result.is_ok());
        assert!(invocation.success);
        assert_eq!(invocation.latency_ms, Some(150));
    }

    #[tokio::test]
    async fn test_log_failed_invocation() {
        let storage = create_test_storage().await;

        let invocation = Invocation::new("reasoning.linear", json!({"content": "test"}))
            .failure("API timeout", 5000);

        let result = storage.log_invocation(&invocation).await;

        assert!(result.is_ok());
        assert!(!invocation.success);
        assert_eq!(invocation.error, Some("API timeout".to_string()));
    }

    #[tokio::test]
    async fn test_invocation_without_session() {
        let storage = create_test_storage().await;

        let invocation = Invocation::new("reasoning.linear", json!({}));

        let result = storage.log_invocation(&invocation).await;

        assert!(result.is_ok());
        assert!(invocation.session_id.is_none());
    }
}

#[cfg(test)]
mod cascade_delete_tests {
    use super::*;

    #[tokio::test]
    async fn test_delete_session_cascades_thoughts() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let thought = Thought::new(&session.id, "Will be deleted", "linear");
        storage.create_thought(&thought).await.unwrap();

        // Delete session
        storage.delete_session(&session.id).await.unwrap();

        // Thought should also be deleted (CASCADE)
        let thoughts = storage.get_session_thoughts(&session.id).await.unwrap();
        assert!(thoughts.is_empty(), "Thoughts should be cascade deleted");
    }
}

#[cfg(test)]
mod concurrent_access_tests {
    use super::*;

    #[tokio::test]
    async fn test_concurrent_thought_creation() {
        let storage = create_test_storage().await;

        let session = Session::new("linear");
        storage.create_session(&session).await.unwrap();

        let session_id = session.id.clone();

        // Create multiple thoughts concurrently
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let storage = storage.clone();
                let session_id = session_id.clone();
                tokio::spawn(async move {
                    let thought = Thought::new(&session_id, format!("Thought {}", i), "linear");
                    storage.create_thought(&thought).await
                })
            })
            .collect();

        for handle in handles {
            assert!(handle.await.unwrap().is_ok());
        }

        let thoughts = storage.get_session_thoughts(&session_id).await.unwrap();
        assert_eq!(thoughts.len(), 5);
    }
}
