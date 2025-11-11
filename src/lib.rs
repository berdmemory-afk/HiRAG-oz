//! Context Manager - AI Agent Context Management with Embeddings, Vector DB, and HiRAG
//!
//! This library provides a comprehensive context management system for AI agents,
//! featuring hierarchical retrieval-augmented generation (HiRAG), vector embeddings,
//! and structured communication protocols.
//!
//! ## Features
//!
//! - **Circuit Breaker Protection**: Automatic failure detection and recovery
//! - **Rate Limiting**: Protect against abuse and overload
//! - **Authentication**: Secure API access with token-based auth
//! - **Input Validation**: Comprehensive input sanitization and validation
//! - **Observability**: Built-in metrics and health checks
//! - **Concurrency Safety**: Lock-free operations where possible
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use context_manager::prelude::*;
//! use context_manager::v2::{EmbeddingClientV2, HiRAGManagerV2};
//! use std::sync::Arc;
//! use std::collections::HashMap;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     // Load configuration
//!     let config = Config::from_file("config.toml")?;
//!     
//!     // Initialize components
//!     let embedding_client = EmbeddingClientV2::new(config.embedding)?;
//!     // Note: VectorDbClientV2 is not currently available
//!     // let vector_db = VectorDbClientV2::new(config.vector_db).await?;
//!     // let manager = HiRAGManagerV2::new(
//!     //     config.hirag,
//!     //     Arc::new(embedding_client),
//!     //     Arc::new(vector_db),
//!     // ).await?;
//!     
//!     // Store and retrieve context
//!     let id = manager.store_context(
//!         "User prefers dark mode",
//!         ContextLevel::LongTerm,
//!         HashMap::new(),
//!     ).await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod api;
pub mod autodev;
pub mod config;
pub mod context;
pub mod embedding;
pub mod error;
pub mod facts;
pub mod hirag;
pub mod middleware;
pub mod metrics;
pub mod observability;
pub mod protocol;
pub mod vector_db;
pub mod shutdown;
pub mod server;

pub use config::Config;
pub use error::{ContextError, Result};

/// Re-export commonly used types
pub mod prelude {
    pub use crate::api::vision::VisionState;
    pub use crate::config::Config;
    pub use crate::context::{TokenBudgetManager, AdaptiveContextManager, BudgetAllocation};
    pub use crate::embedding::{EmbeddingClient, EmbeddingProvider};
    pub use crate::error::{ContextError, Result};
    pub use crate::facts::{FactStore, Fact, FactQuery, FactsState};
    pub use crate::hirag::{ContextManager, HiRAGManager, ContextRequest, ContextResponse};
    pub use crate::middleware::{RateLimiter, RateLimitConfig, AuthMiddleware, AuthConfig, InputValidator};
    pub use crate::observability::{MetricsCollector, HealthChecker};
    pub use crate::protocol::{Message, MessageHandler, Codec};
    pub use crate::vector_db::{VectorDbClient, VectorStore, ContextLevel};
}

/// Re-export enhanced V2 implementations
pub mod v2 {
    pub use crate::embedding::client_v2::EmbeddingClientV2;
    pub use crate::hirag::manager_v2::HiRAGManagerV2;
}