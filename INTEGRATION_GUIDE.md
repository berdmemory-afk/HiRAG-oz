# Integration Guide: Enhanced HiRAG System

## Overview

This guide explains how to integrate the new token budget management, vision API, and facts store features into your HiRAG application.

## Table of Contents

1. [Configuration](#configuration)
2. [Router Integration](#router-integration)
3. [HiRAG Manager Enhancement](#hirag-manager-enhancement)
4. [Vision API Usage](#vision-api-usage)
5. [Facts Store Usage](#facts-store-usage)
6. [Monitoring & Observability](#monitoring--observability)
7. [Testing](#testing)

## Configuration

### Update config.toml

Add the following sections to your `config.toml`:

```toml
[token_budget]
system_tokens = 700          # System/Instructions: 600-800 tokens
running_brief = 1200         # Running Brief: 1,000-1,500 tokens
recent_turns = 450           # Recent Turns: 300-600 tokens
retrieved_context = 3750     # Retrieved Context: 3,000-4,500 tokens
completion = 1000            # Completion: 800-1,200 tokens
max_total = 8000             # Maximum total tokens per turn

[vision]
service_url = "http://localhost:8080"  # DeepSeek service endpoint
timeout_ms = 5000                       # Request timeout
max_regions_per_request = 16            # Maximum regions per decode
default_fidelity = "10x"                # Default fidelity level

[facts]
collection_name = "facts"               # Qdrant collection name
dedup_enabled = true                    # Enable deduplication
confidence_threshold = 0.8              # Minimum confidence
max_facts_per_query = 100               # Maximum facts per query
```

### Load Configuration

```rust
use context_manager::config::Config;

let config = Config::from_file("config.toml")?;

// Access new configuration sections
if let Some(token_budget) = &config.token_budget {
    println!("Max tokens: {}", token_budget.max_total);
}

if let Some(vision) = &config.vision {
    println!("Vision service: {}", vision.service_url);
}

if let Some(facts) = &config.facts {
    println!("Facts collection: {}", facts.collection_name);
}
```

## Router Integration

### Complete Router Setup

```rust
use context_manager::{
    api::{
        build_router,
        integration::{
            init_vision_service,
            init_facts_store,
            build_vision_routes,
            build_facts_routes,
        },
    },
    config::Config,
};
use std::sync::Arc;

pub async fn build_complete_router(
    config: Config,
    // ... other dependencies
) -> Result<Router> {
    // Build base router
    let base_router = build_router(
        app_state,
        health_checker,
        metrics,
        rate_limiter.clone(),
        auth_middleware.clone(),
        body_limiter.clone(),
    );
    
    // Initialize vision service
    let vision_state = init_vision_service(&config).await?;
    let vision_routes = build_vision_routes(
        vision_state,
        rate_limiter.clone(),
        auth_middleware.clone(),
        body_limiter.clone(),
    );
    
    // Initialize facts store
    let facts_state = init_facts_store(&config, qdrant_client).await?;
    let facts_routes = build_facts_routes(
        facts_state,
        rate_limiter.clone(),
        auth_middleware.clone(),
        body_limiter.clone(),
    );
    
    // Merge all routes
    Ok(base_router
        .merge(vision_routes)
        .merge(facts_routes))
}
```

### Available Endpoints

After integration, the following endpoints will be available:

**Vision API**:
- `POST /api/v1/vision/search` - Search regions by query
- `POST /api/v1/vision/decode` - Decode regions to text
- `POST /api/v1/vision/index` - Index documents
- `GET /api/v1/vision/index/jobs/{job_id}` - Get job status

**Facts API**:
- `POST /api/v1/facts` - Insert a fact
- `POST /api/v1/facts/query` - Query facts

## HiRAG Manager Enhancement

### Using EnhancedHiRAGManager

```rust
use context_manager::{
    hirag::{HiRAGManager, EnhancedHiRAGManager},
    context::{TokenBudgetManager, AdaptiveContextManager},
};
use std::sync::Arc;

// Create base HiRAG manager
let base_manager = Arc::new(HiRAGManager::new(/* ... */));

// Create enhanced manager with token budget
let enhanced_manager = EnhancedHiRAGManager::with_defaults(base_manager)?;

// Or with custom configuration
let budget_manager = TokenBudgetManager::new(token_budget_config)?;
let context_manager = AdaptiveContextManager::new(budget_manager.clone());
let enhanced_manager = EnhancedHiRAGManager::new(
    base_manager,
    context_manager,
    budget_manager,
);
```

### Storing Context with Budget Awareness

```rust
use std::collections::HashMap;

let content = "This is the context content to store.";
let metadata = HashMap::new();

let context_id = enhanced_manager
    .store_context_with_budget(content, metadata)
    .await?;

println!("Stored context: {}", context_id);
```

### Retrieving Context with Adaptive Selection

```rust
let query = "What is Rust?";
let max_results = 10;

let artifacts = enhanced_manager
    .retrieve_context_adaptive(query, max_results)
    .await?;

for artifact in artifacts {
    println!(
        "Context {}: {} tokens, relevance: {:.2}",
        artifact.id,
        artifact.token_count,
        artifact.relevance.total
    );
}
```

### Building Adaptive Context

```rust
let system_prompt = "You are a helpful assistant.".to_string();
let running_brief = "User is working on Rust.".to_string();
let recent_turns = vec![
    "What is async?".to_string(),
    "How do I use tokio?".to_string(),
];

let context = enhanced_manager
    .build_adaptive_context(
        query,
        system_prompt,
        running_brief,
        recent_turns,
        max_results,
    )
    .await?;

println!("Total tokens: {}", context.total_tokens());
println!("Within budget: {}", context.is_within_budget(8000));
```

## Vision API Usage

### Searching Regions

```rust
use context_manager::api::vision::{
    VisionServiceClient,
    VisionSearchRequest,
};
use std::collections::HashMap;

let client = VisionServiceClient::default()?;

let request = VisionSearchRequest {
    query: "Find tables with pricing information".to_string(),
    top_k: 12,
    filters: HashMap::from([
        ("doc_type".to_string(), "pdf".to_string()),
        ("section".to_string(), "pricing".to_string()),
    ]),
};

let response = client.search_regions(request).await?;

for region in response.regions {
    println!(
        "Region {}: page {}, score {:.2}",
        region.region_id,
        region.page,
        region.score
    );
}
```

### Decoding Regions

```rust
use context_manager::api::vision::{DecodeRequest, FidelityLevel};

let request = DecodeRequest {
    region_ids: vec!["r_123".to_string(), "r_456".to_string()],
    fidelity: FidelityLevel::Balanced, // 10x
};

let response = client.decode_regions(request).await?;

for result in response.results {
    println!(
        "Region {}: {} (confidence: {:.2})",
        result.region_id,
        result.text,
        result.confidence
    );
}
```

### Indexing Documents

```rust
use context_manager::api::vision::IndexRequest;

let request = IndexRequest {
    doc_url: "s3://docs/contract.pdf".to_string(),
    metadata: HashMap::from([
        ("repo".to_string(), "legal/contracts".to_string()),
        ("doc_type".to_string(), "pdf".to_string()),
    ]),
    force_reindex: false,
};

let response = client.index_document(request).await?;
println!("Job ID: {}, Status: {:?}", response.job_id, response.status);

// Check job status
let status = client.get_job_status(&response.job_id).await?;
println!("Job status: {:?}", status.status);
```

## Facts Store Usage

### Inserting Facts

```rust
use context_manager::facts::{
    FactStore,
    FactInsertRequest,
    SourceAnchor,
};

let store = /* ... initialized FactStore ... */;

let request = FactInsertRequest {
    subject: "Rust".to_string(),
    predicate: "is_a".to_string(),
    object: "programming_language".to_string(),
    datatype: Some("string".to_string()),
    source_doc: Some("doc_123".to_string()),
    source_anchor: SourceAnchor::new()
        .with_doc("doc_123".to_string(), Some(5))
        .with_region("r_789".to_string(), None),
    confidence: 0.95,
};

let response = store.insert_fact(request).await?;

if response.duplicate {
    println!("Fact already exists: {}", response.fact_id);
} else {
    println!("Inserted fact: {}", response.fact_id);
}
```

### Querying Facts

```rust
use context_manager::facts::{FactQuery, FactQueryRequest};

let query = FactQuery {
    subject: Some("Rust".to_string()),
    predicate: Some("is_a".to_string()),
    object: None,
    source_doc: None,
    min_confidence: Some(0.8),
    limit: 50,
};

let request = FactQueryRequest { query };
let response = store.query_facts(request.query).await?;

println!("Found {} facts", response.total);
for fact in response.facts {
    println!(
        "{} {} {} (confidence: {:.2})",
        fact.subject,
        fact.predicate,
        fact.object,
        fact.confidence
    );
}
```

## Monitoring & Observability

### Token Usage Metrics

```rust
use context_manager::observability::MetricsCollector;

// Track token usage
metrics.record_gauge("token_budget.used", context.total_tokens() as f64);
metrics.record_gauge("token_budget.remaining", 
    (8000 - context.total_tokens()) as f64);

// Track budget overflows
if context.total_tokens() > 8000 {
    metrics.increment_counter("token_budget.overflows");
}
```

### Vision API Metrics

```rust
// Track vision API calls
metrics.increment_counter("vision.search.requests");
metrics.record_histogram("vision.search.latency_ms", latency_ms);
metrics.record_gauge("vision.search.regions_returned", regions.len() as f64);

// Track decode operations
metrics.increment_counter("vision.decode.requests");
metrics.record_gauge("vision.decode.regions_count", region_ids.len() as f64);
```

### Facts Store Metrics

```rust
// Track fact operations
metrics.increment_counter("facts.insert.requests");
metrics.increment_counter("facts.query.requests");
metrics.record_gauge("facts.query.results", facts.len() as f64);

// Track deduplication
if response.duplicate {
    metrics.increment_counter("facts.duplicates.detected");
}
```

## Testing

### Unit Tests

```rust
#[tokio::test]
async fn test_token_budget_enforcement() {
    let manager = TokenBudgetManager::default().unwrap();
    
    // Test within budget
    assert!(manager.check_budget(7000).is_ok());
    
    // Test exceeds budget
    assert!(manager.check_budget(9000).is_err());
}

#[tokio::test]
async fn test_adaptive_context_building() {
    let manager = AdaptiveContextManager::default().unwrap();
    
    let context = manager.build_context(
        "System prompt".to_string(),
        "Brief".to_string(),
        vec!["Turn 1".to_string()],
        vec![],
    ).await.unwrap();
    
    assert!(context.is_within_budget(8000));
}
```

### Integration Tests

```rust
#[tokio::test]
#[ignore] // Requires running Qdrant
async fn test_facts_store_integration() {
    let client = QdrantClient::from_url("http://localhost:6334")
        .build()
        .unwrap();
    
    let store = FactStore::new(client, FactStoreConfig::default())
        .await
        .unwrap();
    
    // Test fact insertion and query
    // ...
}
```

## Best Practices

### Token Budget Management

1. **Monitor Usage**: Always log token usage and budget remaining
2. **Handle Overflows**: Implement graceful degradation when budget is exceeded
3. **Tune Allocations**: Adjust budget allocations based on your use case
4. **Test Limits**: Test with content that approaches the token limit

### Vision API

1. **Batch Requests**: Group related regions for efficient decoding
2. **Choose Fidelity**: Use appropriate fidelity level for your needs
3. **Cache Results**: Cache decoded text to avoid redundant API calls
4. **Handle Errors**: Implement retry logic for transient failures

### Facts Store

1. **Set Confidence**: Use appropriate confidence thresholds for your domain
2. **Provenance**: Always include source anchors for traceability
3. **Query Efficiently**: Use specific filters to reduce result sets
4. **Monitor Duplicates**: Track deduplication rate to optimize insertion

## Troubleshooting

### Token Budget Issues

**Problem**: Context exceeds 8k tokens
**Solution**: 
- Reduce retrieved context allocation
- Implement more aggressive summarization
- Increase compression of running brief

**Problem**: Summarization loses important information
**Solution**:
- Adjust relevance scoring weights
- Increase priority for critical contexts
- Implement custom summarization logic

### Vision API Issues

**Problem**: Decode requests timeout
**Solution**:
- Reduce number of regions per request
- Increase timeout_ms in configuration
- Use lower fidelity level for faster processing

**Problem**: Low confidence scores
**Solution**:
- Check image quality
- Try different fidelity levels
- Verify region bounding boxes are correct

### Facts Store Issues

**Problem**: Too many duplicates
**Solution**:
- Verify hash computation is working
- Check source anchor consistency
- Review fact insertion logic

**Problem**: Query returns too many results
**Solution**:
- Add more specific filters
- Increase confidence threshold
- Reduce limit parameter

## Next Steps

1. Review the [API Documentation](docs/API.md)
2. Check [Examples](examples/) for complete usage patterns
3. Read [Performance Tuning](docs/PERFORMANCE.md) guide
4. See [Deployment Guide](DEPLOYMENT.md) for production setup

## Support

For issues or questions:
- GitHub Issues: https://github.com/berdmemory-afk/HiRAG-oz/issues
- Documentation: See docs/ directory
- Examples: See examples/ directory