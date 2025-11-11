# Handler Integration - Remaining Tasks

## Status
Handler integration is 50% complete. The decode_regions handler has been updated, but index_document and get_job_status need completion.

## Completed ✅

### 1. Imports Updated
```rust
use super::deepseek_client::{DeepseekOcrClient, OcrError};
use axum::http::{HeaderMap, StatusCode};
use tracing::{error, info, warn};
```

### 2. VisionState Extended
```rust
pub struct VisionState {
    pub client: Arc<VisionServiceClient>,
    pub deepseek_client: Arc<DeepseekOcrClient>,  // Added
}
```

### 3. Opt-Out Helper Function
```rust
fn should_use_ocr(headers: &HeaderMap) -> bool {
    headers
        .get("X-Use-OCR")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase() != "false")
        .unwrap_or(true)
}
```

### 4. decode_regions Handler Updated
- Added HeaderMap parameter for opt-out check
- Check X-Use-OCR header before processing
- Use DeepseekOcrClient instead of VisionServiceClient
- Map OcrError to appropriate HTTP status codes
- Return proper ApiError responses

## Remaining Tasks ⏳

### 1. Complete index_document Handler
**File**: `src/api/vision/handlers.rs` (line ~223)

**Current**:
```rust
match state.client.index_document(request).await {
```

**Should be**:
```rust
// Add HeaderMap parameter to function signature
pub async fn index_document(
    State(state): State<VisionState>,
    headers: HeaderMap,  // Add this
    Json(request): Json<IndexRequest>,
) -> Result<Json<IndexResponse>, (StatusCode, Json<ApiError>)> {
    
    // Check opt-out
    if !should_use_ocr(&headers) {
        warn!("OCR disabled for this request via X-Use-OCR header");
        METRICS.record_vision_index(false);
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                error_codes::UPSTREAM_DISABLED,
                "OCR disabled for this request",
            )),
        ));
    }
    
    // Use DeepSeek client
    match state.deepseek_client.index_document(request.doc_url, request.metadata).await {
        Ok(response) => {
            METRICS.record_vision_index(true);
            METRICS.vision_request_duration
                .with_label_values(&["index"])
                .observe(start.elapsed().as_secs_f64());
            Ok(Json(response))
        }
        Err(e) => {
            METRICS.record_vision_index(false);
            METRICS.vision_request_duration
                .with_label_values(&["index"])
                .observe(start.elapsed().as_secs_f64());
            
            let (status, code, message) = match e {
                OcrError::Disabled => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    error_codes::UPSTREAM_DISABLED,
                    "Vision OCR integration is disabled".to_string()
                ),
                OcrError::CircuitOpen(op) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    error_codes::UPSTREAM_ERROR,
                    format!("Circuit breaker is open for {}", op)
                ),
                _ => (
                    StatusCode::BAD_GATEWAY,
                    error_codes::UPSTREAM_ERROR,
                    e.to_string()
                ),
            };
            
            error!("Vision index failed: {}", message);
            Err((status, Json(ApiError::new(code, message))))
        }
    }
}
```

### 2. Update get_job_status Handler
**File**: `src/api/vision/handlers.rs` (line ~250)

**Current**:
```rust
match state.client.get_job_status(job_id).await {
```

**Should be**:
```rust
// Add HeaderMap parameter
pub async fn get_job_status(
    State(state): State<VisionState>,
    headers: HeaderMap,  // Add this
    Path(job_id): Path<String>,
) -> Result<Json<JobStatusResponse>, (StatusCode, Json<ApiError>)> {
    
    // Check opt-out
    if !should_use_ocr(&headers) {
        warn!("OCR disabled for this request via X-Use-OCR header");
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                error_codes::UPSTREAM_DISABLED,
                "OCR disabled for this request",
            )),
        ));
    }
    
    // Use DeepSeek client
    match state.deepseek_client.get_job_status(job_id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            let (status, code, message) = match e {
                OcrError::Disabled => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    error_codes::UPSTREAM_DISABLED,
                    "Vision OCR integration is disabled".to_string()
                ),
                _ => (
                    StatusCode::BAD_GATEWAY,
                    error_codes::UPSTREAM_ERROR,
                    e.to_string()
                ),
            };
            
            error!("Get job status failed: {}", message);
            Err((status, Json(ApiError::new(code, message))))
        }
    }
}
```

### 3. Update Startup/Main to Create DeepseekOcrClient
**File**: `src/main.rs` or router builder

**Add**:
```rust
use crate::api::vision::{DeepseekOcrClient, DeepseekConfig};

// Load config
let vision_config = DeepseekConfig::default().from_env();

// Create DeepSeek client
let deepseek_client = Arc::new(
    DeepseekOcrClient::new(vision_config)
        .expect("Failed to create DeepSeek OCR client")
);

// Create vision state
let vision_state = VisionState {
    client: vision_service_client,
    deepseek_client,
};
```

### 4. Add error_codes::UPSTREAM_DISABLED
**File**: `src/api/models.rs` or wherever error_codes is defined

**Add**:
```rust
pub mod error_codes {
    pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
    pub const RATE_LIMIT: &str = "RATE_LIMIT";
    pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
    pub const FORBIDDEN: &str = "FORBIDDEN";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const CONFLICT: &str = "CONFLICT";
    pub const TIMEOUT: &str = "TIMEOUT";
    pub const UPSTREAM_ERROR: &str = "UPSTREAM_ERROR";
    pub const UPSTREAM_DISABLED: &str = "UPSTREAM_DISABLED";  // Add this
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
}
```

## Testing Checklist

### Unit Tests
- [ ] Test should_use_ocr() helper function
- [ ] Test opt-out with X-Use-OCR: false
- [ ] Test opt-out with X-Use-OCR: true
- [ ] Test opt-out with missing header (default true)

### Integration Tests
- [ ] Test decode with DeepSeek client
- [ ] Test index with DeepSeek client
- [ ] Test job status with DeepSeek client
- [ ] Test global opt-out (enabled=false)
- [ ] Test per-request opt-out (header)
- [ ] Test circuit breaker triggering
- [ ] Test cache hit/miss
- [ ] Test retry logic

### Manual Testing
- [ ] Start with enabled=false, verify 503 responses
- [ ] Start with enabled=true, verify requests work
- [ ] Send X-Use-OCR: false header, verify 503
- [ ] Trigger circuit breaker, verify protection
- [ ] Check metrics endpoint for new metrics
- [ ] Verify cache statistics

## Estimated Time
- Complete index_document handler: 15 minutes
- Complete get_job_status handler: 15 minutes
- Add UPSTREAM_DISABLED constant: 5 minutes
- Update startup/main: 30 minutes
- Testing: 1 hour
- **Total**: ~2 hours

## Files to Modify
1. `src/api/vision/handlers.rs` - Complete remaining handlers
2. `src/api/models.rs` or error module - Add UPSTREAM_DISABLED
3. `src/main.rs` or router builder - Wire DeepseekOcrClient
4. `tests/` - Add integration tests

## Success Criteria
- [ ] All handlers use DeepseekOcrClient
- [ ] All handlers support X-Use-OCR header
- [ ] All handlers map OcrError correctly
- [ ] Startup creates DeepseekOcrClient
- [ ] All tests pass
- [ ] Metrics are collected
- [ ] Documentation is updated