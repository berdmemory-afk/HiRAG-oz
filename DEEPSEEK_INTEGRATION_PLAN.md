# DeepSeek OCR Integration - Implementation Plan

## Overview
Replace the stub VisionServiceClient with a production-grade DeepSeek OCR integration including caching, circuit breaker, retries, and opt-out controls.

## Implementation Tasks

### 1. Core Infrastructure ✅
- [x] Create DeepseekOcrClient with retry/backoff
- [x] Implement LRU cache with TTL
- [x] Add circuit breaker
- [x] Add concurrency semaphore

### 2. Configuration ✅
- [x] Extend vision config with new fields
- [x] Add environment variable support
- [x] Add per-request opt-out support

### 3. Metrics ✅
- [x] Add DeepSeek-specific metrics
- [x] Cache hit/miss metrics
- [x] Circuit breaker metrics

### 4. Security & Privacy ✅
- [x] Log redaction for OCR text
- [x] API key from environment
- [x] No VT blob exposure

### 5. Testing ✅
- [x] Unit tests for cache
- [x] Unit tests for circuit breaker
- [x] Integration tests (ignored)

### 6. Documentation ✅
- [x] Update configuration guide
- [x] Add opt-out documentation
- [x] Add rollout plan

## Files to Create/Modify

### New Files
1. `src/api/vision/deepseek_client.rs` - Main client implementation
2. `src/api/vision/cache.rs` - LRU cache with TTL
3. `src/api/vision/circuit_breaker.rs` - Circuit breaker implementation
4. `src/api/vision/config.rs` - Extended configuration
5. `tests/deepseek_integration_tests.rs` - Integration tests

### Modified Files
1. `src/api/vision/mod.rs` - Export new modules
2. `src/api/vision/handlers.rs` - Use DeepseekOcrClient
3. `src/metrics/mod.rs` - Add DeepSeek metrics
4. `config.toml` - Add new configuration sections
5. `README.md` - Document opt-out and configuration

## Implementation Order
1. Cache implementation (foundation)
2. Circuit breaker (foundation)
3. Configuration extension
4. DeepseekOcrClient (core)
5. Metrics integration
6. Handler updates
7. Tests
8. Documentation

## Success Criteria
- [ ] All unit tests pass
- [ ] Integration tests pass (with mock server)
- [ ] Metrics exposed correctly
- [ ] Opt-out works globally and per-request
- [ ] Cache improves latency
- [ ] Circuit breaker prevents cascading failures
- [ ] No VT blobs exposed
- [ ] Documentation complete