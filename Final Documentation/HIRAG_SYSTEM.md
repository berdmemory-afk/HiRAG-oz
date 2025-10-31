# HiRAG (Hierarchical Retrieval-Augmented Generation) System

## Overview

The HiRAG system is the core innovation of Rust-HiRAG, implementing a sophisticated hierarchical approach to context management and retrieval. It combines the principles of Retrieval-Augmented Generation (RAG) with multi-level context storage to provide intelligent, efficient, and scalable context management for AI systems.

## Hierarchical Architecture

### Three-Tier Context Model

The HiRAG system organizes context into three distinct levels, each optimized for specific access patterns and storage requirements:

#### L1 - Immediate Context (Working Memory)

**Purpose**: Current conversation context and immediate user interactions
- **Storage**: In-memory DashMap for ultra-fast access
- **Access Pattern**: Frequent reads/writes, sub-millisecond latency
- **Size**: Limited (default: 10 contexts)
- **Eviction**: LRU-based with timestamp priority
- **Use Cases**: 
  - Current conversation messages
  - Immediate user preferences
  - Session-specific context
  - Real-time interaction data

**Technical Implementation**:
```rust
pub struct HiRAGManagerV2 {
    l1_cache: Arc<DashMap<Uuid, Context>>,
    l1_cache_size: Arc<AtomicUsize>,
    // ... other fields
}

impl HiRAGManagerV2 {
    async fn update_l1_cache(&self, context: Context) {
        let context_id = context.id;
        
        // Insert or update context (atomic operation)
        self.l1_cache.insert(context_id, context);
        
        // Maintain size limit by removing oldest entries
        if self.l1_cache.len() > self.config.l1_size {
            self.evict_oldest_l1_entries().await;
        }
    }
}
```

#### L2 - Short-term Context (Recent Memory)

**Purpose**: Session-persistent context with medium-term relevance
- **Storage**: Qdrant vector database with optimized indexing
- **Access Pattern**: Moderate frequency, millisecond latency
- **Size**: Medium (default: 100 contexts)
- **TTL**: 1 hour (configurable)
- **Use Cases**:
  - Session history
  - Recent user interactions
  - Temporary preferences
  - Context from previous queries

**Collection Strategy**:
```rust
fn collection_name(&self, level: ContextLevel) -> String {
    format!("contexts_{}", level.as_str().to_lowercase())
}

// Creates separate collections: contexts_immediate, contexts_shortterm, contexts_longterm
```

#### L3 - Long-term Context (Persistent Memory)

**Purpose**: Historical context and learned user patterns
- **Storage**: Qdrant vector database with full indexing
- **Access Pattern**: Lower frequency, optimized for relevance
- **Size**: Unlimited (subject to storage constraints)
- **TTL**: 24 hours (configurable)
- **Use Cases**:
  - User preferences and settings
  - Historical interaction patterns
  - Learned behaviors
  - Long-term knowledge base

## Retrieval Strategy

### Multi-Level Retrieval Algorithm

The HiRAG system implements an intelligent retrieval strategy that balances relevance, recency, and diversity across all context levels.

#### Token Allocation Strategy

```rust
pub struct RetrievalStrategy {
    pub l1_allocation: f32,    // 30% of max tokens
    pub l2_allocation: f32,    // 40% of max tokens  
    pub l3_allocation: f32,    // 30% of max tokens
    pub min_contexts_per_level: usize, // 1 minimum per level
}
```

**Allocation Logic**:
1. **Calculate Token Budget**: Divide max_tokens across levels
2. **Minimum Guarantees**: Ensure at least one context per level
3. **Dynamic Adjustment**: Adapt based on available contexts
4. **Relevance Filtering**: Apply minimum relevance thresholds

#### Parallel Retrieval Process

```rust
async fn retrieve_context(&self, request: ContextRequest) -> Result<ContextResponse> {
    // Calculate token allocations
    let (l1_tokens, l2_tokens, l3_tokens) = self.retriever.calculate_allocations(request.max_tokens);
    
    let mut all_contexts = Vec::new();
    let mut tasks = Vec::new();
    
    // Parallel retrieval from each level
    for level in levels {
        match level {
            ContextLevel::Immediate => {
                // Synchronous L1 cache access
                let contexts = self.get_l1_contexts(l1_tokens).await;
                all_contexts.extend(contexts);
            }
            ContextLevel::ShortTerm | ContextLevel::LongTerm => {
                // Async vector database search
                tasks.push(tokio::spawn(async move {
                    self.retriever.retrieve_from_level(&collection, embedding, max_tokens, filters).await
                }));
            }
        }
    }
    
    // Wait for all parallel tasks with partial failure handling
    for task in tasks {
        match task.await {
            Ok(Ok(contexts)) => all_contexts.extend(contexts),
            Ok(Err(e)) => warn!("Error retrieving contexts: {}", e), // Continue with other levels
            Err(e) => warn!("Task join error: {}", e),
        }
    }
    
    // Rank and filter results
    let ranked_contexts = self.ranker.rank_contexts(all_contexts);
    // ... apply token limits and return response
}
```

### Intelligent Ranking System

#### Multi-Factor Scoring

The ranking system combines multiple factors to determine context relevance:

```rust
pub struct RankingWeights {
    pub similarity_weight: f32,    // 0.5 - Vector similarity
    pub recency_weight: f32,       // 0.2 - Time-based relevance
    pub level_weight: f32,         // 0.2 - Hierarchical importance
    pub frequency_weight: f32,     // 0.1 - Access frequency
}
```

**Scoring Algorithm**:
```rust
fn calculate_final_score(&self, context: &Context, query_embedding: &[f32]) -> f32 {
    let similarity = self.cosine_similarity(query_embedding, &context.embedding);
    let recency = self.calculate_recency_score(context.timestamp);
    let level = self.level_priority(context.level);
    let frequency = self.access_frequency(context.id);
    
    (similarity * self.weights.similarity_weight) +
    (recency * self.weights.recency_weight) +
    (level * self.weights.level_weight) +
    (frequency * self.weights.frequency_weight)
}
```

#### Recency Calculation

```rust
fn calculate_recency_score(&self, timestamp: i64) -> f32 {
    let now = Utc::now().timestamp();
    let age_hours = (now - timestamp) as f32 / 3600.0;
    
    // Exponential decay function
    (1.0 / (1.0 + age_hours * 0.1)).min(1.0)
}
```

#### Level Priority

```rust
fn level_priority(&self, level: ContextLevel) -> f32 {
    match level {
        ContextLevel::Immediate => 1.0,    // Highest priority
        ContextLevel::ShortTerm => 0.7,    // Medium priority
        ContextLevel::LongTerm => 0.4,     // Lower priority
    }
}
```

## Context Lifecycle Management

### Storage Flow

1. **Input Validation**: Validate text and metadata
2. **Embedding Generation**: Create vector representation
3. **Level Determination**: Assign to appropriate level
4. **Storage**: Store in designated storage system
5. **Cache Update**: Update L1 cache if immediate context

```rust
async fn store_context(
    &self,
    text: &str,
    level: ContextLevel,
    metadata: HashMap<String, serde_json::Value>,
) -> Result<Uuid> {
    // Generate embedding
    let embedding = self.embedding_client.embed_single(text).await?;
    
    // Create context point
    let point = VectorPoint {
        id: Uuid::new_v4(),
        vector: embedding,
        payload: Payload {
            text: text.to_string(),
            level,
            timestamp: Utc::now().timestamp(),
            agent_id: "default".to_string(),
            session_id: None,
            metadata: metadata.clone(),
        },
    };
    
    // Store in vector database
    let collection = self.collection_name(level);
    self.vector_db.insert_points(&collection, vec![point]).await?;
    
    // Update L1 cache if immediate context
    if level == ContextLevel::Immediate {
        let context = Context::from_point(point);
        self.update_l1_cache(context).await;
    }
    
    Ok(point.id)
}
```

### Retrieval Flow

1. **Query Processing**: Generate query embedding
2. **Level Selection**: Determine which levels to search
3. **Parallel Search**: Concurrent retrieval from all levels
4. **Deduplication**: Remove duplicate contexts
5. **Ranking**: Apply multi-factor ranking
6. **Token Limiting**: Enforce token constraints
7. **Response Assembly**: Construct final response

### Garbage Collection

#### Background Cleanup

```rust
pub struct BackgroundTasks {
    gc_enabled: bool,
    gc_interval_secs: u64,
    l2_ttl_secs: i64,
    l3_ttl_secs: i64,
}

impl BackgroundTasks {
    async fn start_gc_task(&self) {
        if !self.gc_enabled {
            return;
        }
        
        let interval = Duration::from_secs(self.gc_interval_secs);
        let mut ticker = tokio::time::interval(interval);
        
        loop {
            ticker.tick().await;
            self.cleanup_expired_contexts().await;
        }
    }
    
    async fn cleanup_expired_contexts(&self) {
        let now = Utc::now().timestamp();
        
        // Clean L2 contexts
        if now - self.l2_ttl_secs > 0 {
            self.cleanup_level(ContextLevel::ShortTerm, now - self.l2_ttl_secs).await;
        }
        
        // Clean L3 contexts
        if now - self.l3_ttl_secs > 0 {
            self.cleanup_level(ContextLevel::LongTerm, now - self.l3_ttl_secs).await;
        }
    }
}
```

## Performance Optimizations

### Lock-Free Operations

The HiRAG system uses lock-free data structures for maximum performance:

**DashMap for L1 Cache**:
- **Concurrent Access**: Multiple readers and writers
- **Segment-based**: 16 segments reducing contention
- **Memory Efficiency**: Minimal overhead compared to Mutex<RwLock<HashMap>>
- **Performance**: ~10M operations per second

**Atomic Counters**:
- **Statistics**: Lock-free metrics collection
- **Size Tracking**: Atomic cache size management
- **Performance Monitoring**: Real-time performance data

### Caching Strategies

#### Multi-Level Caching

1. **L1 Cache**: In-memory immediate contexts
2. **Embedding Cache**: Vector embedding results
3. **Query Cache**: Frequently used query results
4. **Connection Pooling**: Database connection reuse

#### Cache Invalidation

```rust
async fn invalidate_context(&self, id: Uuid) {
    // Remove from L1 cache
    self.l1_cache.remove(&id);
    
    // Invalidate embedding cache
    self.embedding_cache.invalidate(&id.to_string()).await;
    
    // Mark for database cleanup (async)
    self.schedule_cleanup(id).await;
}
```

### Batch Operations

#### Embedding Batching

```rust
async fn embed_batch_optimized(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    let batch_size = self.config.batch_size;
    let mut results = Vec::new();
    
    for chunk in texts.chunks(batch_size) {
        let batch_results = self.embedding_client.embed_batch(chunk).await?;
        results.extend(batch_results);
    }
    
    Ok(results)
}
```

#### Vector Database Batching

```rust
async fn batch_insert_points(&self, points: Vec<VectorPoint>) -> Result<()> {
    let batch_size = 100; // Optimal batch size for Qdrant
    for chunk in points.chunks(batch_size) {
        self.vector_db.insert_points(&collection, chunk.to_vec()).await?;
    }
    Ok(())
}
```

## Advanced Features

### Context Deduplication

```rust
fn deduplicate_contexts(&self, contexts: Vec<Context>) -> Vec<Context> {
    let mut seen_ids = HashSet::new();
    let mut deduplicated = Vec::new();
    
    for context in contexts {
        if seen_ids.insert(context.id) {
            deduplicated.push(context);
        }
    }
    
    deduplicated
}
```

### Session Context

```rust
pub struct ContextRequest {
    pub query: String,
    pub max_tokens: usize,
    pub levels: Vec<ContextLevel>,
    pub filters: Option<Filter>,
    pub session_id: Option<String>,  // Session-aware retrieval
    pub priority: Priority,
}
```

### Context Filtering

```rust
pub struct Filter {
    pub level: Option<ContextLevel>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub metadata_filters: HashMap<String, serde_json::Value>,
    pub time_range: Option<TimeRange>,
}
```

## Monitoring and Observability

### Performance Metrics

```rust
pub struct HiRAGStats {
    pub total_contexts: usize,
    pub contexts_per_level: HashMap<ContextLevel, usize>,
    pub avg_retrieval_time_ms: f64,
    pub cache_hit_rate: f64,
    pub avg_relevance_score: f32,
    pub storage_size_bytes: usize,
}
```

### Health Monitoring

```rust
async fn check_health(&self) -> SystemHealth {
    let l1_health = self.check_l1_health().await;
    let l2_health = self.check_l2_health().await;
    let l3_health = self.check_l3_health().await;
    let embedding_health = self.check_embedding_health().await;
    
    SystemHealth {
        status: self.aggregate_health(&[l1_health, l2_health, l3_health, embedding_health]),
        components: vec![
            ComponentHealth { name: "L1 Cache", status: l1_health },
            ComponentHealth { name: "L2 Storage", status: l2_health },
            ComponentHealth { name: "L3 Storage", status: l3_health },
            ComponentHealth { name: "Embedding Service", status: embedding_health },
        ],
    }
}
```

The HiRAG system represents a sophisticated approach to context management, combining hierarchical storage, intelligent retrieval, and production-grade performance optimizations to provide a robust foundation for AI applications.