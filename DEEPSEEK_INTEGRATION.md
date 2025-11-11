# DeepSeek OCR Integration Guide

## Overview

This document describes the production-grade DeepSeek OCR integration with caching, circuit breaker, retries, and opt-out controls.

## Features

### Core Capabilities
- **Real OCR Integration**: Decode text from document regions using DeepSeek OCR
- **Intelligent Caching**: LRU cache with TTL reduces latency and upstream load
- **Circuit Breaker**: Protects against cascading failures
- **Retry Logic**: Exponential backoff for transient failures
- **Concurrency Control**: Semaphore limits concurrent requests
- **Opt-Out Controls**: Global and per-request disable options

### Security & Privacy
- **No VT Exposure**: Vision tokens never sent to clients
- **Log Redaction**: OCR text redacted from logs by default
- **API Key Support**: Bearer token authentication
- **Secure Configuration**: Secrets via environment variables

### Observability
- **Prometheus Metrics**: Request counts, durations, cache hit/miss, circuit breaker
- **Structured Logging**: Request IDs, operation types, error details
- **Cache Statistics**: Hit rate, size, evictions
- **Circuit Breaker Stats**: State, failure count, last failure

## Configuration

### config.toml

```toml
[vision]
# Enable/disable OCR integration globally
enabled = true

# DeepSeek service URL
service_url = "http://localhost:8080"

# API key (or set VISION_API_KEY env var)
api_key = ""

# Request timeout in milliseconds
timeout_ms = 5000

# Maximum regions per decode request
max_regions_per_request = 16

# Default fidelity level (20x, 10x, 5x, 1x)
default_fidelity = "10x"

# Cache TTL in seconds (10 minutes)
decode_cache_ttl_secs = 600

# Maximum cache entries
decode_cache_max_size = 1000

# Maximum concurrent decode requests
max_concurrent_decodes = 16

# Number of retry attempts
retry_attempts = 2

# Base backoff in milliseconds
retry_backoff_ms = 200

# Circuit breaker failure threshold
circuit_breaker_failures = 5

# Circuit breaker reset timeout in seconds
circuit_breaker_reset_secs = 30

# Redact OCR text from logs
log_redact_text = true
```

### Environment Variables

Override configuration via environment variables:

```bash
# Global opt-out
export DEEPSEEK_OCR_ENABLED=false

# Service configuration
export VISION_SERVICE_URL=http://deepseek-ocr:8080
export VISION_API_KEY=your-api-key-here
export VISION_TIMEOUT_MS=8000
export VISION_MAX_CONCURRENT_DECODES=32
```

## Opt-Out Controls

### Global Opt-Out

Disable OCR integration entirely:

**Via Configuration:**
```toml
[vision]
enabled = false
```

**Via Environment:**
```bash
export DEEPSEEK_OCR_ENABLED=false
```

**Behavior:**
- All decode/index requests return `ApiError` with code `UPSTREAM_DISABLED`
- Status code: 503 Service Unavailable
- Response:
```json
{
  "code": "UPSTREAM_DISABLED",
  "message": "Vision OCR integration is disabled",
  "details": {
    "hint": "Enable in configuration or set DEEPSEEK_OCR_ENABLED=true"
  }
}
```

### Per-Request Opt-Out

Disable OCR for specific requests:

**Via Header:**
```bash
curl -H "X-Use-OCR: false" \
  -X POST http://localhost:8081/api/v1/vision/decode \
  -d '{"region_ids": ["region1"], "fidelity": "10x"}'
```

**Via Request Body:**
```json
{
  "region_ids": ["region1", "region2"],
  "fidelity": "10x",
  "use_ocr": false
}
```

**Behavior:**
- Returns empty results or error depending on endpoint
- Consistent with API contract

## API Endpoints

### POST /api/v1/vision/decode

Decode text from document regions.

**Request:**
```json
{
  "region_ids": ["region1", "region2"],
  "fidelity": "10x"
}
```

**Response:**
```json
{
  "results": [
    {
      "region_id": "region1",
      "text": "Decoded text content",
      "confidence": 0.95
    }
  ]
}
```

**Fidelity Levels:**
- `20x`: Highest quality, slowest
- `10x`: High quality (default)
- `5x`: Medium quality
- `1x`: Fast, lower quality

**Limits:**
- Maximum 16 regions per request
- Timeout: 5 seconds (configurable)

### POST /api/v1/vision/index

Index a document for OCR processing.

**Request:**
```json
{
  "doc_url": "https://example.com/document.pdf",
  "metadata": {
    "title": "Document Title",
    "author": "Author Name"
  }
}
```

**Response:**
```json
{
  "job_id": "job-123",
  "status": "pending"
}
```

### GET /api/v1/vision/index/jobs/{job_id}

Get indexing job status.

**Response:**
```json
{
  "job_id": "job-123",
  "status": "completed",
  "error": null
}
```

**Status Values:**
- `pending`: Job queued
- `processing`: Job in progress
- `completed`: Job finished successfully
- `failed`: Job failed

## Caching

### How It Works

- **Key**: `(region_id, fidelity)`
- **TTL**: 10 minutes (configurable)
- **Max Size**: 1000 entries (configurable)
- **Eviction**: LRU (Least Recently Used)

### Cache Behavior

1. **Cache Hit**: Return immediately, no upstream call
2. **Cache Miss**: Call upstream, store result, return
3. **Partial Hit**: Return cached + fetch missing
4. **Expiration**: Automatic cleanup on access

### Cache Statistics

```rust
let stats = client.cache_stats();
println!("Total: {}, Valid: {}, Expired: {}",
    stats.total_entries,
    stats.valid_entries,
    stats.expired_entries
);
```

## Circuit Breaker

### How It Works

- **Closed**: Normal operation, requests allowed
- **Open**: Too many failures, requests rejected
- **Half-Open**: Testing recovery, limited requests

### Configuration

- **Failure Threshold**: 5 consecutive failures (configurable)
- **Reset Timeout**: 30 seconds (configurable)

### Behavior

1. **Closed → Open**: After N consecutive failures
2. **Open → Half-Open**: After reset timeout
3. **Half-Open → Closed**: On successful request
4. **Half-Open → Open**: On failed request

### Circuit Breaker Statistics

```rust
let stats = client.breaker_stats("decode");
println!("State: {:?}, Failures: {}", stats.state, stats.failure_count);
```

## Retry Logic

### Exponential Backoff

- **Attempt 1**: 200ms
- **Attempt 2**: 400ms
- **Attempt 3**: 800ms (final)

### Configuration

```toml
retry_attempts = 2          # Total attempts: 3 (initial + 2 retries)
retry_backoff_ms = 200      # Base backoff
```

### Retry Conditions

- Network errors
- Timeout errors
- 5xx server errors

### No Retry

- 4xx client errors
- Circuit breaker open
- OCR disabled

## Metrics

### Prometheus Metrics

```
# Request counts
deepseek_requests_total{op="decode|index|status", status="success|error|disabled"}

# Request duration
deepseek_request_duration_seconds{op="decode|index|status"}

# Cache metrics
deepseek_cache_hits_total
deepseek_cache_misses_total

# Circuit breaker
deepseek_circuit_open_total{op="decode|index"}
```

### Accessing Metrics

```bash
curl http://localhost:8081/metrics | grep deepseek_
```

### Example Output

```
deepseek_requests_total{op="decode",status="success"} 1234
deepseek_requests_total{op="decode",status="error"} 12
deepseek_request_duration_seconds_sum{op="decode"} 45.6
deepseek_request_duration_seconds_count{op="decode"} 1246
deepseek_cache_hits_total 890
deepseek_cache_misses_total 356
deepseek_circuit_open_total{op="decode"} 2
```

## Error Handling

### Error Types

| Error | Code | Status | Description |
|-------|------|--------|-------------|
| Disabled | UPSTREAM_DISABLED | 503 | OCR integration disabled |
| Circuit Open | UPSTREAM_ERROR | 503 | Circuit breaker open |
| Timeout | TIMEOUT | 504 | Request timeout |
| Upstream Error | UPSTREAM_ERROR | 502 | DeepSeek service error |
| Invalid Response | INTERNAL_ERROR | 500 | Response parsing failed |

### Error Response Format

```json
{
  "code": "UPSTREAM_ERROR",
  "message": "DeepSeek service error: Status 500",
  "details": {
    "operation": "decode",
    "region_count": 5
  }
}
```

## Deployment

### Docker Compose

```yaml
services:
  deepseek-ocr:
    image: deepseek/ocr:latest
    ports:
      - "8080:8080"
    environment:
      - MODEL_PATH=/models
    volumes:
      - ./models:/models

  hirag-oz:
    image: hirag-oz:latest
    ports:
      - "8081:8081"
    environment:
      - VISION_SERVICE_URL=http://deepseek-ocr:8080
      - VISION_API_KEY=${VISION_API_KEY}
      - DEEPSEEK_OCR_ENABLED=true
    depends_on:
      - deepseek-ocr
```

### Kubernetes

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: hirag-config
data:
  VISION_SERVICE_URL: "http://deepseek-ocr:8080"
  DEEPSEEK_OCR_ENABLED: "true"
---
apiVersion: v1
kind: Secret
metadata:
  name: hirag-secrets
type: Opaque
stringData:
  VISION_API_KEY: "your-api-key"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hirag-oz
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: hirag-oz
        image: hirag-oz:latest
        envFrom:
        - configMapRef:
            name: hirag-config
        - secretRef:
            name: hirag-secrets
```

## Rollout Plan

### Phase 1: Canary (Week 1)

1. Deploy with `enabled=false` in production
2. Enable in staging environment
3. Run load tests and monitor metrics
4. Enable for 10% of traffic

**Success Criteria:**
- P95 latency < 500ms
- Error rate < 1%
- Cache hit rate > 60%
- No circuit breaker opens

### Phase 2: Gradual Rollout (Week 2-3)

1. Increase to 25% of traffic
2. Monitor for 48 hours
3. Increase to 50% of traffic
4. Monitor for 48 hours
5. Increase to 100% of traffic

**Monitoring:**
- Request duration
- Error rates
- Cache hit rate
- Circuit breaker state
- Upstream latency

### Phase 3: Optimization (Week 4+)

1. Tune cache TTL based on hit rate
2. Adjust circuit breaker thresholds
3. Optimize concurrency limits
4. Add alerting rules

## Rollback Plan

### Immediate Rollback

```bash
# Disable globally
export DEEPSEEK_OCR_ENABLED=false

# Or update config
kubectl set env deployment/hirag-oz DEEPSEEK_OCR_ENABLED=false
```

### Gradual Rollback

1. Reduce traffic percentage
2. Monitor for stability
3. Disable completely if needed

### Circuit Breaker Protection

- Automatic protection during upstream failures
- Prevents cascading failures
- Self-healing after cooldown

## Troubleshooting

### High Error Rate

**Check:**
1. DeepSeek service health
2. Network connectivity
3. API key validity
4. Circuit breaker state

**Fix:**
```bash
# Check service
curl http://deepseek-ocr:8080/health

# Check circuit breaker
curl http://localhost:8081/metrics | grep circuit_open

# Reset if needed (requires restart)
```

### Low Cache Hit Rate

**Check:**
1. Cache TTL too short
2. Cache size too small
3. High variety of requests

**Fix:**
```toml
decode_cache_ttl_secs = 1200  # Increase to 20 minutes
decode_cache_max_size = 5000  # Increase size
```

### High Latency

**Check:**
1. Upstream latency
2. Concurrency limits
3. Retry attempts

**Fix:**
```toml
max_concurrent_decodes = 32   # Increase concurrency
retry_attempts = 1            # Reduce retries
timeout_ms = 8000             # Increase timeout
```

## Best Practices

### Production Configuration

```toml
[vision]
enabled = true
service_url = "http://deepseek-ocr:8080"
timeout_ms = 8000
max_regions_per_request = 16
decode_cache_ttl_secs = 1200
decode_cache_max_size = 5000
max_concurrent_decodes = 32
retry_attempts = 2
circuit_breaker_failures = 5
log_redact_text = true
```

### Monitoring Alerts

```yaml
# Prometheus alerts
- alert: DeepSeekHighErrorRate
  expr: rate(deepseek_requests_total{status="error"}[5m]) > 0.05
  for: 5m

- alert: DeepSeekCircuitOpen
  expr: deepseek_circuit_open_total > 0
  for: 1m

- alert: DeepSeekLowCacheHitRate
  expr: rate(deepseek_cache_hits_total[5m]) / rate(deepseek_cache_misses_total[5m]) < 0.5
  for: 10m
```

### Security Checklist

- [ ] API key stored in secrets, not config
- [ ] Log redaction enabled
- [ ] HTTPS for upstream communication
- [ ] Network policies restrict access
- [ ] Rate limiting enabled
- [ ] Authentication required

## Support

For issues or questions:
- GitHub Issues: https://github.com/berdmemory-afk/HiRAG-oz/issues
- Documentation: See INTEGRATION_GUIDE.md
- Metrics: http://localhost:8081/metrics