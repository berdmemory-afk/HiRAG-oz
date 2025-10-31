# Vector Database Integration and Circuit Breaker Patterns

## Overview

Rust-HiRAG implements a sophisticated vector database integration with Qdrant, enhanced with enterprise-grade circuit breaker patterns to ensure resilience and high availability. This combination provides reliable vector storage and retrieval capabilities with automatic failure detection and recovery mechanisms.

## Vector Database Architecture

### Qdrant Integration

The system uses Qdrant as the primary vector database, providing high-performance vector similarity search and storage capabilities.

#### Core Components

**VectorDbClient**: Main interface for vector operations
```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn create_collection(&self, name: &str) -> Result<()>;
    async fn delete_collection(&self, name: &str) -> Result<()>;
    async fn insert_points(&self, collection: &str, points: Vec<VectorPoint>) -> Result<()>;
    async fn search(&self, collection: &str, params: SearchParams) -> Result<Vec<SearchResult>>;
    async fn delete_points(&self, collection: &str, ids: Vec<Uuid>) -> Result<()>;
    async fn get_point(&self, collection: &str, id: Uuid) -> Result<Option<VectorPoint>>;
}
```

**Data Models**:
```rust
pub struct VectorPoint {
    pub id: Uuid,
    pub vector: Vec<f32>,
    pub payload: Payload,
}

pub struct Payload {
    pub text: String,
    pub level: ContextLevel,
    pub timestamp: i64,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

pub struct SearchParams {
    pub query_vector: Vec<f32>,
    pub limit: usize,
    pub filter: Option<Filter>,
    pub score_threshold: Option<f32>,
}
```

### Collection Management Strategy

#### Hierarchical Collections

The system creates separate collections for each context level, optimizing for different access patterns:

```rust
impl HiRAGManagerV2 {
    fn collection_name(&self, level: ContextLevel) -> String {
        format!("{}_{}", self.config.collection_prefix, level.as_str().to_lowercase())
    }
    
    async fn initialize(&self) -> Result<()> {
        // Create collections for each level
        for level in &[ContextLevel::Immediate, ContextLevel::ShortTerm, ContextLevel::LongTerm] {
            let collection_name = self.collection_name(*level);
            
            // Try to create collection (will fail if exists, which is fine)
            let _ = self.vector_db.create_collection(&collection_name).await;
        }
        
        Ok(())
    }
}
```

**Collection Naming Convention**:
- `contexts_immediate`: L1 contexts (in-memory + persistent backup)
- `contexts_shortterm`: L2 contexts (session-persistent)
- `contexts_longterm`: L3 contexts (long-term storage)

#### Collection Configuration

```rust
pub struct CollectionConfig {
    pub vector_size: usize,           // 1024 for E5-Large
    pub distance: Distance,           // Cosine, Euclidean, or Dot
    pub indexing: IndexConfig,        // HNSW indexing parameters
    pub replication: ReplicationConfig, // Replication settings
}

pub enum Distance {
    Cosine,      // Default for semantic similarity
    Euclidean,   // Geometric distance
    Dot,         // Dot product similarity
}
```

### Search and Retrieval

#### Vector Similarity Search

```rust
async fn search_similar_contexts(
    &self,
    collection: &str,
    query_embedding: &[f32],
    limit: usize,
    filters: Option<Filter>,
) -> Result<Vec<SearchResult>> {
    let search_params = SearchParams {
        query_vector: query_embedding.to_vec(),
        limit,
        filter: filters,
        score_threshold: Some(self.config.relevance_threshold),
    };
    
    self.vector_db.search(collection, search_params).await
}
```

#### Advanced Filtering

```rust
pub struct Filter {
    pub level: Option<ContextLevel>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub time_range: Option<TimeRange>,
    pub metadata_conditions: Vec<MetadataCondition>,
}

pub enum MetadataCondition {
    Equals { key: String, value: serde_json::Value },
    GreaterThan { key: String, value: serde_json::Value },
    Contains { key: String, value: String },
    In { key: String, values: Vec<serde_json::Value> },
}
```

## Circuit Breaker Implementation

### Circuit Breaker Pattern

The circuit breaker pattern protects the system from cascading failures by automatically detecting and isolating failing components.

#### Circuit States

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Circuit is closed, requests flow normally
    Closed,
    /// Circuit is open, requests are rejected
    Open,
    /// Circuit is half-open, testing if service recovered
    HalfOpen,
}
```

#### Circuit Breaker Configuration

```rust
pub struct CircuitBreakerConfig {
    /// Failure threshold to open circuit
    pub failure_threshold: usize,
    /// Success threshold to close circuit from half-open
    pub success_threshold: usize,
    /// Timeout before attempting to close circuit
    pub timeout: Duration,
    /// Window size for counting failures
    pub window_size: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(60),
            window_size: Duration::from_secs(60),
        }
    }
}
```

### Circuit Breaker Implementation

#### Core Circuit Breaker

```rust
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: Arc<RwLock<CircuitState>>,
    failure_count: Arc<AtomicUsize>,
    success_count: Arc<AtomicUsize>,
    last_failure_time: Arc<RwLock<Option<Instant>>>,
    total_calls: Arc<AtomicU64>,
    total_failures: Arc<AtomicU64>,
}
```

#### State Management

**Request Allowance Logic**:
```rust
impl CircuitBreaker {
    pub async fn allow_request(&self) -> bool {
        self.total_calls.fetch_add(1, Ordering::Relaxed);
        
        let state = *self.state.read().await;
        
        match state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // Check if timeout has elapsed
                if let Some(last_failure) = *self.last_failure_time.read().await {
                    if last_failure.elapsed() >= self.config.timeout {
                        // Transition to half-open
                        *self.state.write().await = CircuitState::HalfOpen;
                        self.success_count.store(0, Ordering::Relaxed);
                        debug!("Circuit breaker transitioning to half-open state");
                        return true;
                    }
                }
                false
            }
            CircuitState::HalfOpen => true,
        }
    }
}
```

**Success Recording**:
```rust
pub async fn record_success(&self) {
    let state = *self.state.read().await;
    
    match state {
        CircuitState::Closed => {
            // Reset failure count on success
            self.failure_count.store(0, Ordering::Relaxed);
        }
        CircuitState::HalfOpen => {
            let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
            
            if successes >= self.config.success_threshold {
                // Transition to closed
                *self.state.write().await = CircuitState::Closed;
                self.failure_count.store(0, Ordering::Relaxed);
                self.success_count.store(0, Ordering::Relaxed);
                debug!("Circuit breaker closed after successful recovery");
            }
        }
        CircuitState::Open => {}
    }
}
```

**Failure Recording**:
```rust
pub async fn record_failure(&self) {
    self.total_failures.fetch_add(1, Ordering::Relaxed);
    
    let state = *self.state.read().await;
    
    match state {
        CircuitState::Closed => {
            let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
            
            if failures >= self.config.failure_threshold {
                // Transition to open
                *self.state.write().await = CircuitState::Open;
                *self.last_failure_time.write().await = Some(Instant::now());
                warn!("Circuit breaker opened after {} failures", failures);
            }
        }
        CircuitState::HalfOpen => {
            // Transition back to open
            *self.state.write().await = CircuitState::Open;
            *self.last_failure_time.write().await = Some(Instant::now());
            self.success_count.store(0, Ordering::Relaxed);
            warn!("Circuit breaker reopened after failure in half-open state");
        }
        CircuitState::Open => {}
    }
}
```

### Integration with Vector Database

#### Protected Vector Operations

```rust
pub struct ProtectedVectorDbClient {
    inner: Arc<dyn VectorStore>,
    circuit_breaker: Arc<CircuitBreaker>,
    metrics: Option<Arc<MetricsCollector>>,
}

impl ProtectedVectorDbClient {
    pub async fn search_with_protection(
        &self,
        collection: &str,
        params: SearchParams,
    ) -> Result<Vec<SearchResult>> {
        // Check circuit breaker
        if !self.circuit_breaker.allow_request().await {
            return Err(VectorDbError::CircuitBreakerOpen.into());
        }
        
        // Execute operation
        match self.inner.search(collection, params).await {
            Ok(results) => {
                self.circuit_breaker.record_success().await;
                if let Some(metrics) = &self.metrics {
                    metrics.record_cache_hit(); // Successful operation
                }
                Ok(results)
            }
            Err(e) => {
                self.circuit_breaker.record_failure().await;
                if let Some(metrics) = &self.metrics {
                    metrics.record_error();
                }
                Err(e)
            }
        }
    }
}
```

#### Retry Logic with Circuit Breaker

```rust
pub async fn search_with_retry(
    &self,
    collection: &str,
    params: SearchParams,
) -> Result<Vec<SearchResult>> {
    let max_retries = 3;
    let mut last_error = None;
    
    for attempt in 1..=max_retries {
        // Check circuit breaker before each attempt
        if !self.circuit_breaker.allow_request().await {
            return Err(VectorDbError::CircuitBreakerOpen.into());
        }
        
        match self.search_with_protection(collection, params.clone()).await {
            Ok(results) => return Ok(results),
            Err(e) => {
                last_error = Some(e);
                if attempt < max_retries {
                    tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| VectorDbError::MaxRetriesExceeded.into()))
}
```

## Performance Optimizations

### Connection Pooling

```rust
pub struct VectorDbPool {
    clients: Arc<RwLock<Vec<QdrantClient>>>,
    current_index: Arc<AtomicUsize>,
    max_connections: usize,
}

impl VectorDbPool {
    pub async fn get_client(&self) -> QdrantClient {
        let index = self.current_index.fetch_add(1, Ordering::Relaxed) % self.max_connections;
        let clients = self.clients.read().await;
        clients[index].clone()
    }
}
```

### Batch Operations

```rust
pub async fn batch_insert_optimized(
    &self,
    collection: &str,
    points: Vec<VectorPoint>,
) -> Result<()> {
    let batch_size = 100; // Optimal for Qdrant
    let mut tasks = Vec::new();
    
    for chunk in points.chunks(batch_size) {
        let client = self.pool.get_client().await;
        let collection = collection.to_string();
        let chunk = chunk.to_vec();
        
        tasks.push(tokio::spawn(async move {
            client.insert_points(&collection, chunk).await
        }));
    }
    
    // Wait for all batches with error handling
    for task in tasks {
        task.await??;
    }
    
    Ok(())
}
```

### Index Optimization

```rust
pub struct IndexConfig {
    pub hnsw_config: HnswConfig,
    pub payload_index: PayloadIndexConfig,
}

pub struct HnswConfig {
    pub m: usize,              // Number of connections
    pub ef_construct: usize,   // Index build time
    pub ef_search: usize,      // Search time
    pub max_indexing_threads: usize,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            hnsw_config: HnswConfig {
                m: 16,
                ef_construct: 100,
                ef_search: 64,
                max_indexing_threads: 4,
            },
            payload_index: PayloadIndexConfig::default(),
        }
    }
}
```

## Monitoring and Observability

### Circuit Breaker Metrics

```rust
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub total_calls: u64,
    pub total_failures: u64,
    pub current_failures: usize,
    pub success_rate: f64,
    pub failure_rate: f64,
}

impl CircuitBreaker {
    pub async fn stats(&self) -> CircuitBreakerStats {
        let total_calls = self.total_calls.load(Ordering::Relaxed);
        let total_failures = self.total_failures.load(Ordering::Relaxed);
        let current_failures = self.failure_count.load(Ordering::Relaxed);
        
        CircuitBreakerStats {
            state: *self.state.read().await,
            total_calls,
            total_failures,
            current_failures,
            success_rate: if total_calls > 0 {
                (total_calls - total_failures) as f64 / total_calls as f64
            } else {
                0.0
            },
            failure_rate: if total_calls > 0 {
                total_failures as f64 / total_calls as f64
            } else {
                0.0
            },
        }
    }
}
```

### Prometheus Integration

```rust
impl CircuitBreaker {
    pub async fn export_prometheus(&self, name: &str) -> String {
        let state_value = match *self.state.read().await {
            CircuitState::Closed => 0,
            CircuitState::HalfOpen => 1,
            CircuitState::Open => 2,
        };
        
        let stats = self.stats().await;
        
        format!(
            "# HELP {}_state Circuit breaker state (0=closed, 1=half-open, 2=open)\n\
             # TYPE {}_state gauge\n\
             {}_state {}\n\
             \n\
             # HELP {}_calls_total Total calls through circuit breaker\n\
             # TYPE {}_calls_total counter\n\
             {}_calls_total {}\n\
             \n\
             # HELP {}_failures_total Total failures\n\
             # TYPE {}_failures_total counter\n\
             {}_failures_total {}\n\
             \n\
             # HELP {}_success_rate Success rate percentage\n\
             # TYPE {}_success_rate gauge\n\
             {}_success_rate {}\n",
            name, name, name, state_value,
            name, name, name, stats.total_calls,
            name, name, name, stats.total_failures,
            name, name, name, stats.success_rate * 100.0
        )
    }
}
```

### Health Monitoring

```rust
pub async fn check_vector_db_health(&self) -> ComponentHealth {
    // Test basic connectivity
    match self.vector_db.health_check().await {
        Ok(_) => {
            // Check circuit breaker state
            let stats = self.circuit_breaker.stats().await;
            match stats.state {
                CircuitState::Closed => ComponentHealth {
                    name: "Vector Database".to_string(),
                    status: HealthStatus::Healthy,
                    message: Some("Operating normally".to_string()),
                },
                CircuitState::HalfOpen => ComponentHealth {
                    name: "Vector Database".to_string(),
                    status: HealthStatus::Degraded,
                    message: Some("Recovering from failure".to_string()),
                },
                CircuitState::Open => ComponentHealth {
                    name: "Vector Database".to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Circuit breaker open".to_string()),
                },
            }
        }
        Err(e) => ComponentHealth {
            name: "Vector Database".to_string(),
            status: HealthStatus::Unhealthy,
            message: Some(format!("Connection failed: {}", e)),
        },
    }
}
```

## Advanced Features

### Multi-Region Replication

```rust
pub struct ReplicatedVectorDb {
    primary: Arc<dyn VectorStore>,
    replicas: Vec<Arc<dyn VectorStore>>,
    circuit_breakers: Vec<Arc<CircuitBreaker>>,
}

impl ReplicatedVectorDb {
    pub async fn search_with_fallback(
        &self,
        collection: &str,
        params: SearchParams,
    ) -> Result<Vec<SearchResult>> {
        // Try primary first
        if let Ok(results) = self.primary.search(collection, params.clone()).await {
            return Ok(results);
        }
        
        // Try replicas in order
        for (replica, circuit_breaker) in self.replicas.iter().zip(&self.circuit_breakers) {
            if circuit_breaker.allow_request().await {
                match replica.search(collection, params.clone()).await {
                    Ok(results) => {
                        circuit_breaker.record_success().await;
                        return Ok(results);
                    }
                    Err(_) => {
                        circuit_breaker.record_failure().await;
                        continue;
                    }
                }
            }
        }
        
        Err(VectorDbError::AllReplicasFailed.into())
    }
}
```

### Automatic Failover

```rust
pub struct FailoverManager {
    primary: Arc<ProtectedVectorDbClient>,
    backup: Arc<ProtectedVectorDbClient>,
    failover_threshold: usize,
    consecutive_failures: Arc<AtomicUsize>,
}

impl FailoverManager {
    pub async fn execute_with_failover<F, T>(&self, operation: F) -> Result<T>
    where
        F: Fn(Arc<ProtectedVectorDbClient>) -> Pin<Box<dyn Future<Output = Result<T>> + Send>>,
    {
        // Try primary
        match operation(self.primary.clone()).await {
            Ok(result) => {
                self.consecutive_failures.store(0, Ordering::Relaxed);
                return Ok(result);
            }
            Err(_) => {
                let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
                
                if failures >= self.failover_threshold {
                    warn!("Primary failed {} times, switching to backup", failures);
                    return operation(self.backup.clone()).await;
                }
            }
        }
        
        Err(VectorDbError::OperationFailed.into())
    }
}
```

This comprehensive vector database integration with circuit breaker patterns provides enterprise-grade reliability and performance for the HiRAG system, ensuring high availability and graceful degradation under failure conditions.