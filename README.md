# HiRAG-oz

This is a Rust-based Hierarchical Retrieval-Augmented Generation (HiRAG) system that implements a multi-layered approach to document processing, storage, and retrieval with advanced features including token budget management, vision API integration, and neuro-symbolic reasoning.

## Features

### Core Features
- Hierarchical document processing (document -> chunks -> sub-chunks)
- Vector database integration with Qdrant
- Circuit breaker pattern for resilience
- LLM middleware for content generation
- API layer with rate limiting and authentication
- Caching mechanisms for performance
- Comprehensive testing framework

### Enhanced Features (New)
- **Token Budget Management**: ≤8k token enforcement with adaptive context orchestration
- **Vision API**: DeepSeek OCR integration for multi-modal document processing
- **Facts Store**: Neuro-symbolic reasoning with RDF-style triple storage
- **Adaptive Context**: Smart prioritization and information-preserving summarization

## Architecture

The system implements a multi-layered architecture:

1. **API Layer**: Handles requests and authentication
   - Context management endpoints
   - Vision API endpoints (search, decode, index)
   - Facts API endpoints (insert, query)
2. **Token Budget Management**: Enforces ≤8k token limits with adaptive context
3. **LLM Middleware**: Processes content generation requests
4. **Circuit Breaker**: Provides resilience against failures
5. **Caching Layer**: Caches results for faster retrieval
6. **Vector Database**: Stores and retrieves embeddings
7. **Document Processing**: Handles document ingestion and processing
8. **Vision Processing**: Multi-modal document understanding with DeepSeek OCR
9. **Facts Store**: Neuro-symbolic reasoning with provenance tracking

## Setup

1. Ensure you have Rust installed
2. Install and run Qdrant vector database
3. Set up your configuration file
4. Build and run the application

## Configuration

Configuration is handled through `config.toml` file with settings for:
- API endpoints
- Vector database connection
- Circuit breaker parameters
- Caching settings
- LLM provider credentials
- Token budget allocation (system, brief, turns, context, completion)
- Vision API configuration (service URL, timeout, fidelity)
- Facts store settings (collection name, confidence threshold, deduplication)

See [INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md) for detailed configuration options.

## Usage

After setup, the system can be used to:
- Ingest documents into the HiRAG system
- Query the system for information retrieval with token budget enforcement
- Process documents hierarchically with adaptive context management
- Leverage caching and circuit breakers for reliability
- Search and decode visual content from documents (Vision API)
- Store and query facts with provenance tracking (Facts Store)
- Build adaptive contexts that stay within ≤8k token limits

### Quick Start

```rust
use context_manager::prelude::*;

// Create enhanced HiRAG manager with token budget
let enhanced_manager = EnhancedHiRAGManager::with_defaults(base_manager)?;

// Build adaptive context
let context = enhanced_manager.build_adaptive_context(
    query,
    system_prompt,
    running_brief,
    recent_turns,
    max_results,
).await?;

// Use vision API
let vision_client = VisionServiceClient::default()?;
let regions = vision_client.search_regions(search_request).await?;

// Store facts
let fact_store = FactStore::new(qdrant_client, config).await?;
let response = fact_store.insert_fact(fact_request).await?;
```

See [INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md) for comprehensive usage examples.

## Testing

The project includes comprehensive testing:
- **Unit Tests**: 31+ tests for token budget, vision API, and facts store
- **Integration Tests**: End-to-end testing with real services
- **E2E Tests**: Complete workflow validation

Run tests with:
```bash
cargo test                    # Run all tests
cargo test --lib             # Run library tests only
cargo test integration       # Run integration tests
cargo test --test integration_enhanced  # Run enhanced integration tests
```

## API Endpoints

### Context Management
- `POST /api/v1/contexts` - Store context
- `POST /api/v1/contexts/search` - Search contexts
- `POST /api/v1/contexts/delete` - Delete context
- `POST /api/v1/contexts/clear` - Clear level

### Vision API (New)
- `POST /api/v1/vision/search` - Search regions by query
- `POST /api/v1/vision/decode` - Decode regions to text
- `POST /api/v1/vision/index` - Index documents
- `GET /api/v1/vision/index/jobs/{job_id}` - Get job status

### Facts API (New)
- `POST /api/v1/facts` - Insert a fact
- `POST /api/v1/facts/query` - Query facts

## Documentation

- [INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md) - Complete integration guide
- [IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md) - Implementation status
- [COMPLETE_IMPLEMENTATION_SUMMARY.md](COMPLETE_IMPLEMENTATION_SUMMARY.md) - Technical details

## Recent Updates

### v0.2.0 - Enhanced Features
- ✅ Token budget management with ≤8k enforcement
- ✅ Vision API for multi-modal document processing
- ✅ Facts store for neuro-symbolic reasoning
- ✅ Adaptive context management
- ✅ 31+ new unit tests
- ✅ Comprehensive documentation