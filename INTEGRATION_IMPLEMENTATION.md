# Production Infrastructure Integration Implementation

## Overview
This document tracks the implementation of critical integrations to wire together the production infrastructure components.

## Implementation Tasks

### 1. Wire TiktokenEstimator into TokenBudgetManager ✅
**Status**: In Progress
**Files**: `src/context/token_budget.rs`
**Changes**:
- Add `TokenEstimator` trait field to `TokenBudgetManager`
- Replace `estimate_tokens()` method to use injected estimator
- Update constructor to accept estimator
- Add `with_tiktoken()` and `with_word_based()` factory methods
- Update tests to use both estimators

### 2. Wire LLMSummarizer into AdaptiveContextManager ✅
**Status**: In Progress
**Files**: `src/context/adaptive_manager.rs`
**Changes**:
- Add `Summarizer` trait field to `AdaptiveContextManager`
- Replace `summarize_turns()` to use injected summarizer
- Update constructor to accept summarizer
- Add `with_llm_summarizer()` and `with_concat_summarizer()` factory methods
- Update tests to use both summarizers

### 3. Add Metrics to Vision/Facts Handlers ✅
**Status**: In Progress
**Files**: 
- `src/api/vision/handlers.rs`
- `src/facts/store.rs`
**Changes**:
- Import METRICS singleton
- Add counter increments for requests
- Add histogram observations for durations
- Add error counters

### 4. Expose /metrics Endpoint ✅
**Status**: In Progress
**Files**: `src/api/routes.rs`, `src/api/router_complete.rs`
**Changes**:
- Create `metrics_handler()` function
- Add GET /metrics route
- Return Prometheus text format

### 5. Update Configuration ✅
**Status**: In Progress
**Files**: `config.toml`
**Changes**:
- Add `[summarizer]` section
- Add `[token_estimator]` section
- Document all new options

### 6. Update Documentation ✅
**Status**: In Progress
**Files**: `README.md`, `INTEGRATION_GUIDE.md`
**Changes**:
- Document new configuration options
- Add usage examples
- Update architecture diagrams

## Implementation Order
1. TokenBudgetManager + TiktokenEstimator (foundational)
2. AdaptiveContextManager + LLMSummarizer (depends on #1)
3. Metrics in handlers (independent)
4. /metrics endpoint (depends on #3)
5. Configuration updates (after all code changes)
6. Documentation updates (final step)

## Testing Strategy
- Unit tests for each component
- Integration tests for wired components
- End-to-end test with real LLM (manual)
- Load test for metrics collection

## Success Criteria
- ✅ All unit tests pass
- ✅ Integration tests pass
- ✅ Metrics endpoint returns valid Prometheus format
- ✅ Token estimation uses tiktoken
- ✅ Summarization uses LLM API
- ✅ Configuration is complete and documented