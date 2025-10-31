# HiRAG-oz

This is a Rust-based Hierarchical Retrieval-Augmented Generation (HiRAG) system that implements a multi-layered approach to document processing, storage, and retrieval with caching and circuit breaker mechanisms.

## Features

- Hierarchical document processing (document -> chunks -> sub-chunks)
- Vector database integration with Qdrant
- Circuit breaker pattern for resilience
- LLM middleware for content generation
- API layer with rate limiting and authentication
- Caching mechanisms for performance
- Comprehensive testing framework

## Architecture

The system implements a multi-layered architecture:

1. **API Layer**: Handles requests and authentication
2. **LLM Middleware**: Processes content generation requests
3. **Circuit Breaker**: Provides resilience against failures
4. **Caching Layer**: Caches results for faster retrieval
5. **Vector Database**: Stores and retrieves embeddings
6. **Document Processing**: Handles document ingestion and processing

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

## Usage

After setup, the system can be used to:
- Ingest documents into the HiRAG system
- Query the system for information retrieval
- Process documents hierarchically
- Leverage caching and circuit breakers for reliability

## Testing

The project includes comprehensive unit tests, integration tests, and E2E tests in the test files.