//! HiRAG (Hierarchical Retrieval-Augmented Generation) implementation

pub mod manager;
pub mod manager_v2;
pub mod manager_enhanced;
pub mod retriever;
pub mod ranker;
pub mod models;
pub mod token_estimator;
pub mod background;

pub use manager::HiRAGManager;
pub use manager_v2::HiRAGManagerV2;
pub use manager_enhanced::EnhancedHiRAGManager;
pub use models::{Context, ContextRequest, ContextResponse, Priority};
pub use ranker::ContextRanker;
pub use token_estimator::TokenEstimator;

use async_trait::async_trait;
use crate::error::Result;
use crate::vector_db::ContextLevel;
use std::collections::HashMap;
use uuid::Uuid;

/// Trait for context management operations
#[async_trait]
pub trait ContextManager: Send + Sync {
    /// Store new context
    async fn store_context(
        &self,
        text: &str,
        level: ContextLevel,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid>;
    
    /// Retrieve relevant contexts
    async fn retrieve_context(&self, request: ContextRequest) -> Result<ContextResponse>;
    
    /// Update context metadata
    async fn update_context(
        &self,
        id: Uuid,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<()>;
    
    /// Delete context
    async fn delete_context(&self, id: Uuid) -> Result<()>;
    
    /// Clear contexts by level
    async fn clear_level(&self, level: ContextLevel) -> Result<()>;
}