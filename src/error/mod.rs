//! Error types for the context management system

use thiserror::Error;

/// Result type alias for context manager operations
pub type Result<T> = std::result::Result<T, ContextError>;

/// Main error type for the context management system
#[derive(Error, Debug)]
pub enum ContextError {
    #[error("Embedding error: {0}")]
    Embedding(#[from] EmbeddingError),
    
    #[error("Vector database error: {0}")]
    VectorDb(#[from] VectorDbError),
    
    #[error("HiRAG error: {0}")]
    HiRAG(#[from] HiRAGError),
    
    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),
    
    #[error("Validation error: {0}")]
    Validation(#[from] crate::middleware::ValidationError),
    
    #[error("Rate limit error: {0}")]
    RateLimit(#[from] crate::middleware::RateLimitError),
    
    #[error("Authentication error: {0}")]
    Auth(#[from] crate::middleware::AuthError),
    
    #[error("Configuration error: {0}")]
    Configuration(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
    
    #[error("Token budget error: {0}")]
    TokenBudget(String),
}

/// Errors related to embedding generation
#[derive(Error, Debug)]
pub enum EmbeddingError {
    #[error("API request failed: {0}")]
    ApiError(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Authentication failed")]
    AuthenticationFailed,
    
    #[error("Timeout after {0} seconds")]
    Timeout(u64),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
}

/// Errors related to vector database operations
#[derive(Error, Debug)]
pub enum VectorDbError {
    #[error("Connection error: {0}")]
    ConnectionError(String),
    
    #[error("Collection not found: {0}")]
    CollectionNotFound(String),
    
    #[error("Collection already exists: {0}")]
    CollectionExists(String),
    
    #[error("Search error: {0}")]
    SearchError(String),
    
    #[error("Insert error: {0}")]
    InsertError(String),
    
    #[error("Delete error: {0}")]
    DeleteError(String),
    
    #[error("Invalid vector dimension: expected {expected}, got {actual}")]
    InvalidDimension { expected: usize, actual: usize },
    
    #[error("Payload too large: {size} bytes exceeds maximum of {max_size} bytes")]
    PayloadTooLarge { size: usize, max_size: usize },

    #[error("Invalid ID format: {0}")]
    InvalidIdFormat(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Qdrant client error: {0}")]
    QdrantError(String),
}

/// Errors related to HiRAG operations
#[derive(Error, Debug)]
pub enum HiRAGError {
    #[error("Context not found: {0}")]
    ContextNotFound(String),
    
    #[error("Invalid context level: {0}")]
    InvalidLevel(String),
    
    #[error("Token limit exceeded: {limit}")]
    TokenLimitExceeded { limit: usize },
    
    #[error("Retrieval error: {0}")]
    RetrievalError(String),
    
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Ranking error: {0}")]
    RankingError(String),
}

/// Errors related to protocol operations
#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Invalid message format: {0}")]
    InvalidFormat(String),
    
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(String),
    
    #[error("Message validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Encoding error: {0}")]
    EncodingError(String),
    
    #[error("Decoding error: {0}")]
    DecodingError(String),
    
    #[error("Handler not found for message type: {0}")]
    HandlerNotFound(String),
    
    #[error("Message too large: {size} bytes (max: {max_size} bytes)")]
    MessageTooLarge { size: usize, max_size: usize },
}

impl From<config::ConfigError> for ContextError {
    fn from(err: config::ConfigError) -> Self {
        ContextError::Configuration(err.to_string())
    }
}
impl From<crate::context::token_budget::BudgetError> for ContextError {
    fn from(err: crate::context::token_budget::BudgetError) -> Self {
        ContextError::TokenBudget(err.to_string())
    }
}
