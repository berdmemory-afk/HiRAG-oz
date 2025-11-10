//! Configuration management for the context management system

use serde::{Deserialize, Serialize};
use std::path::Path;
use secrecy::{Secret, ExposeSecret};

pub mod loader;
pub mod validation;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub embedding: EmbeddingConfig,
    pub vector_db: VectorDbConfig,
    pub hirag: HiRAGConfig,
    pub protocol: ProtocolConfig,
    pub logging: LoggingConfig,
    pub server: ServerConfig,
    #[serde(default)]
    pub token_budget: Option<TokenBudgetConfig>,
    #[serde(default)]
    pub vision: Option<VisionConfig>,
    #[serde(default)]
    pub facts: Option<FactsConfig>,
}

/// Configuration for the embedding service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Chutes API endpoint URL
    pub api_url: String,
    
    /// API authentication token (secured)
    #[serde(serialize_with = "serialize_secret", deserialize_with = "deserialize_secret")]
    pub api_token: Secret<String>,
    
    /// Maximum batch size for embedding requests
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    
    /// Request timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    
    /// Maximum retry attempts
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    
    /// Enable caching
    #[serde(default = "default_cache_enabled")]
    pub cache_enabled: bool,
    
    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
    
    /// Cache maximum size
    #[serde(default = "default_cache_size")]
    pub cache_size: usize,
    
    /// Enable TLS for embedding API connection
    #[serde(default)]
    pub tls_enabled: bool,
    
    /// Verify TLS certificates
    #[serde(default = "default_tls_verify")]
    pub tls_verify: bool,
}

/// Configuration for Qdrant vector database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorDbConfig {
    /// Qdrant server URL
    pub url: String,
    
    /// API key (optional, secured)
    #[serde(default, serialize_with = "serialize_optional_secret", deserialize_with = "deserialize_optional_secret")]
    pub api_key: Option<Secret<String>>,
    
    /// Collection name prefix
    #[serde(default = "default_collection_prefix")]
    pub collection_prefix: String,
    
    /// Vector dimension
    #[serde(default = "default_vector_size")]
    pub vector_size: usize,
    
    /// Distance metric
    #[serde(default)]
    pub distance: Distance,
    
    /// Connection timeout in seconds
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    
    /// Enable TLS for Qdrant connection
    #[serde(default)]
    pub tls_enabled: bool,
    
    /// Path to TLS certificate file (optional)
    pub tls_cert_path: Option<String>,
    
    /// Verify TLS certificates
    #[serde(default = "default_tls_verify")]
    pub tls_verify: bool,
}

/// Distance metrics supported
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Distance {
    #[default]
    Cosine,
    Euclidean,
    Dot,
}



/// HiRAG configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HiRAGConfig {
    /// L1 (Immediate) context size
    #[serde(default = "default_l1_size")]
    pub l1_size: usize,
    
    /// L2 (Short-term) context size
    #[serde(default = "default_l2_size")]
    pub l2_size: usize,
    
    /// Enable L3 (Long-term) context
    #[serde(default = "default_l3_enabled")]
    pub l3_enabled: bool,
    
    /// Maximum total context tokens
    #[serde(default = "default_max_context_tokens")]
    pub max_context_tokens: usize,
    
    /// Minimum relevance score threshold
    #[serde(default = "default_relevance_threshold")]
    pub relevance_threshold: f32,
    
    /// Token estimation method
    #[serde(default)]
    pub token_estimator: TokenEstimator,
    
    /// Retrieval strategy
    #[serde(default)]
    pub retrieval_strategy: RetrievalStrategy,
    
    /// Ranking weights
    #[serde(default)]
    pub ranking_weights: RankingWeights,
    
    /// Enable background garbage collection
    #[serde(default = "default_gc_enabled")]
    pub gc_enabled: bool,
    
    /// Garbage collection interval in seconds
    #[serde(default = "default_gc_interval")]
    pub gc_interval_secs: u64,
    
    /// L2 context TTL in seconds
    #[serde(default = "default_l2_ttl")]
    pub l2_ttl_secs: i64,
    
    /// L3 context TTL in seconds
    #[serde(default = "default_l3_ttl")]
    pub l3_ttl_secs: i64,
}

/// Token estimation methods
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TokenEstimator {
    CharacterBased { chars_per_token: f32 },
    WordBased { words_per_token: f32 },
}

impl Default for TokenEstimator {
    fn default() -> Self {
        TokenEstimator::CharacterBased { chars_per_token: 4.0 }
    }
}

/// Context retrieval strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalStrategy {
    /// L1 allocation (percentage of max tokens)
    #[serde(default = "default_l1_allocation")]
    pub l1_allocation: f32,
    
    /// L2 allocation (percentage of max tokens)
    #[serde(default = "default_l2_allocation")]
    pub l2_allocation: f32,
    
    /// L3 allocation (percentage of max tokens)
    #[serde(default = "default_l3_allocation")]
    pub l3_allocation: f32,
    
    /// Minimum contexts per level
    #[serde(default = "default_min_contexts")]
    pub min_contexts_per_level: usize,
}

impl Default for RetrievalStrategy {
    fn default() -> Self {
        Self {
            l1_allocation: 0.3,
            l2_allocation: 0.4,
            l3_allocation: 0.3,
            min_contexts_per_level: 1,
        }
    }
}

/// Ranking weights for context scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankingWeights {
    /// Weight for vector similarity
    #[serde(default = "default_similarity_weight")]
    pub similarity_weight: f32,
    
    /// Weight for recency
    #[serde(default = "default_recency_weight")]
    pub recency_weight: f32,
    
    /// Weight for context level
    #[serde(default = "default_level_weight")]
    pub level_weight: f32,
    
    /// Weight for access frequency
    #[serde(default = "default_frequency_weight")]
    pub frequency_weight: f32,
}

impl Default for RankingWeights {
    fn default() -> Self {
        Self {
            similarity_weight: 0.5,
            recency_weight: 0.2,
            level_weight: 0.2,
            frequency_weight: 0.1,
        }
    }
}

/// Protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Protocol version
    #[serde(default = "default_protocol_version")]
    pub version: String,
    
    /// Serialization format
    #[serde(default)]
    pub codec: CodecType,
    
    /// Maximum message size in MB
    #[serde(default = "default_max_message_size")]
    pub max_message_size_mb: usize,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server port
    #[serde(default = "default_server_port")]
    pub port: u16,
    
    /// Server host
    #[serde(default = "default_server_host")]
    pub host: String,
    
    /// Maximum request body size in MB (0 = unlimited)
    #[serde(default = "default_max_body_size")]
    pub max_body_size_mb: usize,
}

/// Codec types for message serialization
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodecType {
    #[default]
    Json,
    MessagePack,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,
    
    /// Log format
    #[serde(default = "default_log_format")]
    pub format: String,
}

// Default value functions
fn default_batch_size() -> usize { 32 }
fn default_timeout() -> u64 { 30 }

fn default_tls_verify() -> bool { true }
fn default_max_retries() -> u32 { 3 }
fn default_cache_enabled() -> bool { true }
fn default_cache_ttl() -> u64 { 3600 }
fn default_cache_size() -> usize { 1000 }
fn default_collection_prefix() -> String { "contexts".to_string() }
fn default_vector_size() -> usize { 1024 }
fn default_l1_size() -> usize { 10 }
fn default_l2_size() -> usize { 100 }
fn default_l3_enabled() -> bool { true }
fn default_max_context_tokens() -> usize { 4000 }
fn default_relevance_threshold() -> f32 { 0.7 }
fn default_l1_allocation() -> f32 { 0.3 }
fn default_l2_allocation() -> f32 { 0.4 }
fn default_l3_allocation() -> f32 { 0.3 }
fn default_min_contexts() -> usize { 1 }
fn default_similarity_weight() -> f32 { 0.5 }
fn default_recency_weight() -> f32 { 0.2 }
fn default_level_weight() -> f32 { 0.2 }
fn default_frequency_weight() -> f32 { 0.1 }
fn default_protocol_version() -> String { "1.0.0".to_string() }
fn default_max_message_size() -> usize { 10 }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }
fn default_server_port() -> u16 { 8080 }
fn default_server_host() -> String { "0.0.0.0".to_string() }

// GC configuration defaults
fn default_gc_enabled() -> bool { false }
fn default_gc_interval() -> u64 { 300 } // 5 minutes
fn default_l2_ttl() -> i64 { 3600 } // 1 hour
fn default_l3_ttl() -> i64 { 86400 } // 24 hours

// Server configuration defaults
fn default_max_body_size() -> usize { 10 } // 10 MB default

impl Config {
    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<Path>>(path: P) -> crate::error::Result<Self> {
        let config = loader::load_config(path)?;
        validation::validate_config(&config)?;
        Ok(config)
    }
    
    /// Load configuration with environment variable overrides
    pub fn from_file_with_env<P: AsRef<Path>>(path: P) -> crate::error::Result<Self> {
        let config = loader::load_config_with_env(path)?;
        validation::validate_config(&config)?;
        Ok(config)
    }
    
    /// Validate this configuration
    pub fn validate(&self) -> crate::error::Result<()> {
        validation::validate_config(self)
    }
    
    /// Create default configuration
    pub fn default_config() -> Self {
        Self {
            embedding: EmbeddingConfig {
                api_url: "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings".to_string(),
                api_token: Secret::new(std::env::var("CHUTES_API_TOKEN").unwrap_or_default()),
                batch_size: default_batch_size(),
                timeout_secs: default_timeout(),
                max_retries: default_max_retries(),
                cache_enabled: default_cache_enabled(),
                cache_ttl_secs: default_cache_ttl(),
                cache_size: default_cache_size(),
                tls_enabled: false,
                tls_verify: true,
            },
            vector_db: VectorDbConfig {
                url: "http://localhost:6334".to_string(),
                api_key: None,
                collection_prefix: default_collection_prefix(),
                vector_size: default_vector_size(),
                distance: Distance::default(),
                timeout_secs: default_timeout(),
                tls_enabled: false,
                tls_cert_path: None,
                tls_verify: true,
            },
            hirag: HiRAGConfig {
                l1_size: default_l1_size(),
                l2_size: default_l2_size(),
                l3_enabled: default_l3_enabled(),
                max_context_tokens: default_max_context_tokens(),
                relevance_threshold: default_relevance_threshold(),
                token_estimator: TokenEstimator::default(),
                retrieval_strategy: RetrievalStrategy::default(),
                ranking_weights: RankingWeights::default(),
                gc_enabled: default_gc_enabled(),
                gc_interval_secs: default_gc_interval(),
                l2_ttl_secs: default_l2_ttl(),
                l3_ttl_secs: default_l3_ttl(),
            },
            protocol: ProtocolConfig {
                version: default_protocol_version(),
                codec: CodecType::default(),
                max_message_size_mb: default_max_message_size(),
            },
            logging: LoggingConfig {
                level: default_log_level(),
                format: default_log_format(),
            },
            server: ServerConfig {
                port: default_server_port(),
                host: default_server_host(),
                max_body_size_mb: default_max_body_size(),
            },
        }
    }
}
/// Custom serializer for Secret<String>
fn serialize_secret<S>(secret: &Secret<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(secret.expose_secret())
}

/// Custom deserializer for Secret<String>
fn deserialize_secret<'de, D>(deserializer: D) -> Result<Secret<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Secret::new(s))
}

/// Custom serializer for Option<Secret<String>>
fn serialize_optional_secret<S>(secret: &Option<Secret<String>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match secret {
        Some(s) => serializer.serialize_some(s.expose_secret()),
        None => serializer.serialize_none(),
    }
}

/// Custom deserializer for Option<Secret<String>>
fn deserialize_optional_secret<'de, D>(deserializer: D) -> Result<Option<Secret<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(deserializer)?;
    Ok(opt.map(Secret::new))
}

/// Token budget configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudgetConfig {
    /// System/Instructions tokens
    #[serde(default = "default_system_tokens")]
    pub system_tokens: usize,
    
    /// Running brief tokens
    #[serde(default = "default_running_brief")]
    pub running_brief: usize,
    
    /// Recent turns tokens
    #[serde(default = "default_recent_turns")]
    pub recent_turns: usize,
    
    /// Retrieved context tokens
    #[serde(default = "default_retrieved_context")]
    pub retrieved_context: usize,
    
    /// Completion tokens
    #[serde(default = "default_completion")]
    pub completion: usize,
    
    /// Maximum total tokens per turn
    #[serde(default = "default_max_total")]
    pub max_total: usize,
}

impl Default for TokenBudgetConfig {
    fn default() -> Self {
        Self {
            system_tokens: default_system_tokens(),
            running_brief: default_running_brief(),
            recent_turns: default_recent_turns(),
            retrieved_context: default_retrieved_context(),
            completion: default_completion(),
            max_total: default_max_total(),
        }
    }
}

/// Vision API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    /// DeepSeek service URL
    #[serde(default = "default_vision_service_url")]
    pub service_url: String,
    
    /// Request timeout in milliseconds
    #[serde(default = "default_vision_timeout_ms")]
    pub timeout_ms: u64,
    
    /// Maximum regions per decode request
    #[serde(default = "default_max_regions_per_request")]
    pub max_regions_per_request: usize,
    
    /// Default fidelity level
    #[serde(default = "default_fidelity")]
    pub default_fidelity: String,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            service_url: default_vision_service_url(),
            timeout_ms: default_vision_timeout_ms(),
            max_regions_per_request: default_max_regions_per_request(),
            default_fidelity: default_fidelity(),
        }
    }
}

/// Facts store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactsConfig {
    /// Qdrant collection name for facts
    #[serde(default = "default_facts_collection_name")]
    pub collection_name: String,
    
    /// Enable hash-based deduplication
    #[serde(default = "default_dedup_enabled")]
    pub dedup_enabled: bool,
    
    /// Minimum confidence threshold for fact insertion
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f32,
    
    /// Maximum facts returned per query
    #[serde(default = "default_max_facts_per_query")]
    pub max_facts_per_query: usize,
}

impl Default for FactsConfig {
    fn default() -> Self {
        Self {
            collection_name: default_facts_collection_name(),
            dedup_enabled: default_dedup_enabled(),
            confidence_threshold: default_confidence_threshold(),
            max_facts_per_query: default_max_facts_per_query(),
        }
    }
}

// Default functions for TokenBudgetConfig
fn default_system_tokens() -> usize { 700 }
fn default_running_brief() -> usize { 1200 }
fn default_recent_turns() -> usize { 450 }
fn default_retrieved_context() -> usize { 3750 }
fn default_completion() -> usize { 1000 }
fn default_max_total() -> usize { 8000 }

// Default functions for VisionConfig
fn default_vision_service_url() -> String { "http://localhost:8080".to_string() }
fn default_vision_timeout_ms() -> u64 { 5000 }
fn default_max_regions_per_request() -> usize { 16 }
fn default_fidelity() -> String { "10x".to_string() }

// Default functions for FactsConfig
fn default_facts_collection_name() -> String { "facts".to_string() }
fn default_dedup_enabled() -> bool { true }
fn default_confidence_threshold() -> f32 { 0.8 }
fn default_max_facts_per_query() -> usize { 100 }
