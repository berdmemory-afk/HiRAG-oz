# Embedding Service and Caching Mechanisms

## Overview

Rust-HiRAG implements a sophisticated embedding service with multi-layered caching to provide high-performance vector generation for multilingual text processing. The system integrates with the Chutes API for E5-Large multilingual embeddings while maintaining intelligent caching strategies to minimize latency and cost.

## Embedding Service Architecture

### Chutes API Integration

The system uses the Chutes API for generating high-quality multilingual embeddings using the IntFloat E5-Large model.

#### Model Specifications

**IntFloat Multilingual E5-Large**:
- **Dimensions**: 1024
- **Languages**: 100+ supported languages
- **Performance**: State-of-the-art semantic understanding
- **Use Case**: Multilingual semantic similarity and retrieval

#### API Configuration

```rust
pub struct EmbeddingConfig {
    pub api_url: String,
    pub api_token: Secret<String>,
    pub batch_size: usize,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub cache_enabled: bool,
    pub cache_ttl_secs: u64,
    pub cache_size: usize,
    pub tls_enabled: bool,
    pub tls_verify: bool,
}
```

**Default Configuration**:
```toml
[embedding]
api_url = "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings"
api_token = "your_api_token_here"
batch_size = 32
timeout_secs = 30
max_retries = 3
cache_enabled = true
cache_ttl_secs = 3600
cache_size = 1000
tls_enabled = true
tls_verify = true
```

### Embedding Client Implementation

#### Core Embedding Provider Trait

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate embedding for a single text
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>>;
    
    /// Generate embeddings for multiple texts
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    
    /// Get the dimension of embeddings
    fn embedding_dimension(&self) -> usize;
}
```

#### Enhanced Embedding Client V2

```rust
pub struct EmbeddingClientV2 {
    config: EmbeddingConfig,
    http_client: reqwest::Client,
    cache: Option<Arc<EmbeddingCache>>,
    metrics: Option<Arc<MetricsCollector>>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl EmbeddingClientV2 {
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent("Rust-HiRAG/0.1.0")
            .build()?;
            
        let cache = if config.cache_enabled {
            Some(Arc::new(EmbeddingCache::new(
                config.cache_size,
                Duration::from_secs(config.cache_ttl_secs),
            )))
        } else {
            None
        };
        
        Ok(Self {
            config,
            http_client,
            cache,
            metrics: None,
            circuit_breaker: Arc::new(CircuitBreaker::default()),
        })
    }
}
```

### API Request/Response Models

#### Request Models

```rust
#[derive(Debug, Serialize)]
pub struct EmbeddingRequest {
    pub input: EmbeddingInput,
    pub model: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Debug, Serialize)]
pub struct EmbeddingParameters {
    pub truncate: Option<bool>,
    pub normalize: Option<bool>,
}
```

#### Response Models

```rust
#[derive(Debug, Deserialize)]
pub struct EmbeddingResponse {
    pub object: String,
    pub data: Vec<EmbeddingData>,
    pub model: String,
    pub usage: Usage,
}

#[derive(Debug, Deserialize)]
pub struct EmbeddingData {
    pub object: String,
    pub embedding: Vec<f32>,
    pub index: usize,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub total_tokens: usize,
}
```

## Caching Architecture

### Multi-Layer Caching Strategy

The system implements a sophisticated multi-layer caching approach to optimize performance and reduce API costs.

#### L1 Cache: In-Memory Cache

```rust
pub struct EmbeddingCache {
    cache: Arc<MokaCache<String, Vec<f32>>>,
    ttl: Duration,
    max_size: usize,
    hit_count: Arc<AtomicU64>,
    miss_count: Arc<AtomicU64>,
}

impl EmbeddingCache {
    pub fn new(max_size: usize, ttl: Duration) -> Self {
        let cache = moka::future::CacheBuilder::new(max_size)
            .time_to_live(ttl)
            .build();
            
        Self {
            cache: Arc::new(cache),
            ttl,
            max_size,
            hit_count: Arc::new(AtomicU64::new(0)),
            miss_count: Arc::new(AtomicU64::new(0)),
        }
    }
    
    pub async fn get(&self, key: &str) -> Option<Vec<f32>> {
        match self.cache.get(key).await {
            Some(embedding) => {
                self.hit_count.fetch_add(1, Ordering::Relaxed);
                Some(embedding)
            }
            None => {
                self.miss_count.fetch_add(1, Ordering::Relaxed);
                None
            }
        }
    }
    
    pub async fn insert(&self, key: String, embedding: Vec<f32>) {
        self.cache.insert(key, embedding).await;
    }
    
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}
```

#### Cache Key Generation

```rust
impl EmbeddingClientV2 {
    fn generate_cache_key(&self, text: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        format!("embedding:{:x}", hasher.finish())
    }
    
    fn generate_batch_cache_key(&self, texts: &[String]) -> String {
        let combined = texts.join("|");
        self.generate_cache_key(&combined)
    }
}
```

### Cache Invalidation Strategies

#### TTL-Based Expiration

```rust
impl EmbeddingCache {
    pub async fn invalidate_expired(&self) {
        // Moka cache handles TTL expiration automatically
        // This method can be used for manual cleanup if needed
        let metrics = self.cache.metrics();
        debug!(
            "Cache metrics - Size: {}, Hits: {}, Misses: {}, Hit Rate: {:.2}%",
            metrics.current_size(),
            metrics.hits(),
            metrics.misses(),
            self.hit_rate() * 100.0
        );
    }
}
```

#### Manual Invalidation

```rust
impl EmbeddingClientV2 {
    pub async fn invalidate_cache(&self, text: &str) -> Result<()> {
        if let Some(cache) = &self.cache {
            let key = self.generate_cache_key(text);
            cache.cache.invalidate(&key).await;
        }
        Ok(())
    }
    
    pub async fn clear_cache(&self) -> Result<()> {
        if let Some(cache) = &self.cache {
            cache.cache.invalidate_all().await;
        }
        Ok(())
    }
}
```

## Performance Optimizations

### Batch Processing

#### Intelligent Batching

```rust
impl EmbeddingClientV2 {
    pub async fn embed_batch_optimized(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let batch_size = self.config.batch_size;
        let mut results = Vec::with_capacity(texts.len());
        
        // Check cache first for all texts
        let mut uncached_texts = Vec::new();
        let mut uncached_indices = Vec::new();
        
        for (index, text) in texts.iter().enumerate() {
            if let Some(cache) = &self.cache {
                if let Some(embedding) = cache.get(text).await {
                    results.push(embedding);
                } else {
                    uncached_texts.push(text.clone());
                    uncached_indices.push(index);
                    results.push(Vec::new()); // Placeholder
                }
            } else {
                uncached_texts.push(text.clone());
                uncached_indices.push(index);
                results.push(Vec::new()); // Placeholder
            }
        }
        
        // Process uncached texts in batches
        if !uncached_texts.is_empty() {
            let batch_results = self.process_uncached_batch(&uncached_texts).await?;
            
            // Update cache and results
            for ((text, index), embedding) in uncached_texts
                .into_iter()
                .zip(uncached_indices)
                .zip(batch_results)
            {
                if let Some(cache) = &self.cache {
                    cache.insert(text, embedding.clone()).await;
                }
                results[index] = embedding;
            }
        }
        
        Ok(results)
    }
    
    async fn process_uncached_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let batch_size = self.config.batch_size;
        let mut all_results = Vec::new();
        
        for chunk in texts.chunks(batch_size) {
            let chunk_results = self.embed_batch_api(chunk).await?;
            all_results.extend(chunk_results);
        }
        
        Ok(all_results)
    }
}
```

### Connection Pooling

```rust
impl EmbeddingClientV2 {
    fn create_http_client(&self) -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(self.config.timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(30))
            .user_agent("Rust-HiRAG/0.1.0")
            .build()
            .map_err(|e| EmbeddingError::HttpClientError(e.to_string()).into())
    }
}
```

### Retry Logic with Exponential Backoff

```rust
impl EmbeddingClientV2 {
    async fn embed_with_retry(&self, text: &str) -> Result<Vec<f32>> {
        let max_retries = self.config.max_retries;
        let mut last_error = None;
        
        for attempt in 1..=max_retries {
            match self.embed_single_api(text).await {
                Ok(embedding) => return Ok(embedding),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries {
                        let delay = Duration::from_millis(100 * 2_u64.pow(attempt - 1));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| EmbeddingError::MaxRetriesExceeded.into()))
    }
}
```

## API Implementation

### HTTP Client Integration

#### Single Text Embedding

```rust
impl EmbeddingClientV2 {
    async fn embed_single_api(&self, text: &str) -> Result<Vec<f32>> {
        // Check circuit breaker
        if !self.circuit_breaker.allow_request().await {
            return Err(EmbeddingError::CircuitBreakerOpen.into());
        }
        
        let request = EmbeddingRequest {
            input: EmbeddingInput::Single(text.to_string()),
            model: "intfloat/multilingual-e5-large".to_string(),
        };
        
        let response = self.http_client
            .post(&self.config.api_url)
            .bearer_auth(self.config.api_token.expose_secret())
            .json(&request)
            .send()
            .await?;
            
        if response.status().is_success() {
            let embedding_response: EmbeddingResponse = response.json().await?;
            self.circuit_breaker.record_success().await;
            
            if let Some(data) = embedding_response.data.first() {
                Ok(data.embedding.clone())
            } else {
                Err(EmbeddingError::InvalidResponse("No embedding data".to_string()).into())
            }
        } else {
            self.circuit_breaker.record_failure().await;
            let error_text = response.text().await.unwrap_or_default();
            Err(EmbeddingError::ApiError(error_text).into())
        }
    }
}
```

#### Batch Text Embedding

```rust
impl EmbeddingClientV2 {
    async fn embed_batch_api(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        // Check circuit breaker
        if !self.circuit_breaker.allow_request().await {
            return Err(EmbeddingError::CircuitBreakerOpen.into());
        }
        
        let request = EmbeddingRequest {
            input: EmbeddingInput::Batch(texts.to_vec()),
            model: "intfloat/multilingual-e5-large".to_string(),
        };
        
        let response = self.http_client
            .post(&self.config.api_url)
            .bearer_auth(self.config.api_token.expose_secret())
            .json(&request)
            .send()
            .await?;
            
        if response.status().is_success() {
            let embedding_response: EmbeddingResponse = response.json().await?;
            self.circuit_breaker.record_success().await;
            
            let mut embeddings = Vec::with_capacity(embedding_response.data.len());
            for data in embedding_response.data {
                embeddings.push(data.embedding);
            }
            
            Ok(embeddings)
        } else {
            self.circuit_breaker.record_failure().await;
            let error_text = response.text().await.unwrap_or_default();
            Err(EmbeddingError::ApiError(error_text).into())
        }
    }
}
```

## Monitoring and Metrics

### Performance Metrics

```rust
pub struct EmbeddingMetrics {
    pub total_requests: AtomicU64,
    pub cache_hits: AtomicU64,
    pub cache_misses: AtomicU64,
    pub api_calls: AtomicU64,
    pub api_errors: AtomicU64,
    pub avg_response_time_ms: AtomicU64,
    pub total_tokens_processed: AtomicU64,
}

impl EmbeddingMetrics {
    pub fn record_request(&self, duration: Duration) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        
        let duration_ms = duration.as_millis() as u64;
        let current_avg = self.avg_response_time_ms.load(Ordering::Relaxed);
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        
        // Calculate rolling average
        let new_avg = ((current_avg * (total_requests - 1)) + duration_ms) / total_requests;
        self.avg_response_time_ms.store(new_avg, Ordering::Relaxed);
    }
    
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        let total = hits + misses;
        
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}
```

### Health Monitoring

```rust
impl EmbeddingClientV2 {
    pub async fn health_check(&self) -> ComponentHealth {
        // Test API connectivity with a simple request
        let test_text = "health check";
        
        match self.embed_single_api(test_text).await {
            Ok(embedding) => {
                if embedding.len() == 1024 { // Expected dimension
                    ComponentHealth {
                        name: "Embedding Service".to_string(),
                        status: HealthStatus::Healthy,
                        message: Some("API responding normally".to_string()),
                    }
                } else {
                    ComponentHealth {
                        name: "Embedding Service".to_string(),
                        status: HealthStatus::Degraded,
                        message: Some(format!("Unexpected embedding dimension: {}", embedding.len())),
                    }
                }
            }
            Err(e) => {
                ComponentHealth {
                    name: "Embedding Service".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some(format!("API error: {}", e)),
                }
            }
        }
    }
    
    pub async fn get_cache_stats(&self) -> CacheStats {
        if let Some(cache) = &self.cache {
            CacheStats {
                hit_rate: cache.hit_rate(),
                size: cache.cache.metrics().current_size(),
                max_size: cache.max_size,
                ttl_seconds: cache.ttl.as_secs(),
            }
        } else {
            CacheStats::disabled()
        }
    }
}
```

## Advanced Features

### Text Preprocessing

```rust
impl EmbeddingClientV2 {
    fn preprocess_text(&self, text: &str) -> String {
        // Normalize text for better embedding quality
        text.trim()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || *c == '.' || *c == ',')
            .collect::<String>()
            .chars()
            .collect::<String>()
            .to_lowercase()
    }
    
    fn validate_text(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Err(EmbeddingError::InvalidInput("Text cannot be empty".to_string()).into());
        }
        
        if text.len() > 8192 { // Maximum token limit
            return Err(EmbeddingError::InvalidInput("Text too long".to_string()).into());
        }
        
        Ok(())
    }
}
```

### Async Batch Queue

```rust
pub struct AsyncEmbeddingQueue {
    sender: mpsc::UnboundedSender<EmbeddingJob>,
    metrics: Arc<EmbeddingMetrics>,
}

struct EmbeddingJob {
    texts: Vec<String>,
    response_tx: oneshot::Sender<Result<Vec<Vec<f32>>>>,
}

impl AsyncEmbeddingQueue {
    pub async fn new(client: Arc<EmbeddingClientV2>) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        let metrics = Arc::new(EmbeddingMetrics::new());
        
        // Start worker task
        let client_clone = client.clone();
        let metrics_clone = metrics.clone();
        tokio::spawn(async move {
            Self::worker_task(receiver, client_clone, metrics_clone).await;
        });
        
        Self { sender, metrics }
    }
    
    async fn worker_task(
        mut receiver: mpsc::UnboundedReceiver<EmbeddingJob>,
        client: Arc<EmbeddingClientV2>,
        metrics: Arc<EmbeddingMetrics>,
    ) {
        while let Some(job) = receiver.recv().await {
            let start_time = Instant::now();
            
            let result = client.embed_batch_optimized(&job.texts).await;
            
            metrics.record_request(start_time.elapsed());
            
            let _ = job.response_tx.send(result);
        }
    }
}
```

### Distributed Caching (Future Enhancement)

```rust
#[cfg(feature = "redis-cache")]
pub struct RedisEmbeddingCache {
    client: redis::Client,
    ttl: Duration,
    prefix: String,
}

#[cfg(feature = "redis-cache")]
impl RedisEmbeddingCache {
    pub async fn get(&self, key: &str) -> Option<Vec<f32>> {
        let mut conn = self.client.get_async_connection().await.ok()?;
        let cache_key = format!("{}:{}", self.prefix, key);
        
        let result: Option<Vec<u8>> = conn.get(&cache_key).await.ok()?;
        
        result.and_then(|data| {
            bincode::deserialize::<Vec<f32>>(&data).ok()
        })
    }
    
    pub async fn insert(&self, key: &str, embedding: &[f32]) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let cache_key = format!("{}:{}", self.prefix, key);
        
        let serialized = bincode::serialize(embedding)?;
        
        conn.set_ex(&cache_key, serialized, self.ttl.as_secs()).await?;
        
        Ok(())
    }
}
```

This comprehensive embedding service with multi-layered caching provides high-performance, cost-effective vector generation for the HiRAG system, ensuring low latency and high throughput for multilingual text processing.