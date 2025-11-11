use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

use crate::api::error_codes;
use crate::api::models::ApiError;
use crate::api::vision::models::{
    DecodeRequest, DecodeResponse, IndexRequest, IndexResponse, JobStatusResponse,
    VisionSearchRequest, VisionSearchResponse,
};
use crate::api::vision::{VisionServiceClient, DeepseekOcrClient};
use crate::api::vision::deepseek_client::OcrError;
use crate::metrics::METRICS;

/// Vision API state
#[derive(Clone)]
pub struct VisionState {
    pub client: Arc<VisionServiceClient>,
    pub deepseek_client: Arc<DeepseekOcrClient>,
}

/// Search for regions by query
///
/// POST /api/v1/vision/search
pub async fn search_regions(
    State(state): State<VisionState>,
    Json(request): Json<VisionSearchRequest>,
) -> Result<Json<VisionSearchResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    
    info!("Vision search request: query={}", request.query);

    // Validate request
    if request.query.is_empty() {
        METRICS.record_vision_search(false);
        METRICS.vision_request_duration
            .with_label_values(&["search"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "Query cannot be empty",
            )),
        ));
    }

    if request.top_k > 50 {
        METRICS.record_vision_search(false);
        METRICS.vision_request_duration
            .with_label_values(&["search"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "top_k cannot exceed 50",
            )),
        ));
    }

    // Use stub client for now
    match state.client.search_regions(request).await {
        Ok(response) => {
            METRICS.record_vision_search(true);
            METRICS.vision_request_duration
                .with_label_values(&["search"])
                .observe(start.elapsed().as_secs_f64());
            Ok(Json(response))
        }
        Err(e) => {
            METRICS.record_vision_search(false);
            METRICS.vision_request_duration
                .with_label_values(&["search"])
                .observe(start.elapsed().as_secs_f64());
            error!("Vision search failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(error_codes::INTERNAL_ERROR, e.to_string())),
            ))
        }
    }
}

/// Helper function to check if OCR should be used
fn should_use_ocr(headers: &HeaderMap) -> bool {
    headers
        .get("X-Use-OCR")
        .and_then(|v| v.to_str().ok())
        .map(|v| {
            let v = v.to_ascii_lowercase();
            v == "true" || v == "1" || v == "yes" || v == "on"
        })
        .unwrap_or(true) // Default to enabled
}

/// Decode regions to text
///
/// POST /api/v1/vision/decode
pub async fn decode_regions(
    State(state): State<VisionState>,
    headers: HeaderMap,
    Json(request): Json<DecodeRequest>,
) -> Result<Json<DecodeResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    
    info!("Vision decode request: {} regions", request.region_ids.len());

    // Check per-request opt-out
    if !should_use_ocr(&headers) {
        warn!("OCR disabled for this request via X-Use-OCR header");
        METRICS.record_vision_decode(false);
        METRICS.vision_request_duration
            .with_label_values(&["decode"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                error_codes::UPSTREAM_DISABLED,
                "OCR disabled for this request",
            )),
        ));
    }

    // Validate request
    if request.region_ids.is_empty() {
        METRICS.record_vision_decode(false);
        METRICS.vision_request_duration
            .with_label_values(&["decode"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "region_ids cannot be empty",
            )),
        ));
    }

    if request.region_ids.len() > 16 {
        METRICS.record_vision_decode(false);
        METRICS.vision_request_duration
            .with_label_values(&["decode"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "region_ids cannot exceed 16",
            )),
        ));
    }
    
    // Note: BBox validation can be added when region metadata includes page dimensions
    // Example: if let Some(region) = get_region(&region_id) {
    //     region.bbox.validate(region.page_width, region.page_height)?;
    // }

    // Use DeepSeek OCR client
    match state.deepseek_client.decode_regions(request.region_ids, request.fidelity).await {
        Ok(results) => {
            METRICS.record_vision_decode(true);
            METRICS.vision_request_duration
                .with_label_values(&["decode"])
                .observe(start.elapsed().as_secs_f64());
            Ok(Json(DecodeResponse { results }))
        }
        Err(e) => {
            METRICS.record_vision_decode(false);
            METRICS.vision_request_duration
                .with_label_values(&["decode"])
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
                OcrError::Timeout(msg) => (
                    StatusCode::GATEWAY_TIMEOUT,
                    error_codes::TIMEOUT,
                    msg
                ),
                _ => (
                    StatusCode::BAD_GATEWAY,
                    error_codes::UPSTREAM_ERROR,
                    e.to_string()
                ),
            };
            
            error!("Vision decode failed: {}", message);
            Err((status, Json(ApiError::new(code, message))))
        }
    }
}

/// Index a document
///
/// POST /api/v1/vision/index
pub async fn index_document(
    State(state): State<VisionState>,
    headers: HeaderMap,
    Json(request): Json<IndexRequest>,
) -> Result<Json<IndexResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    
    info!("Vision index request: doc_url={}", request.doc_url);

    // Check per-request opt-out
    if !should_use_ocr(&headers) {
        warn!("OCR disabled for this request via X-Use-OCR header");
        METRICS.record_vision_index(false);
        METRICS.vision_request_duration
            .with_label_values(&["index"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                error_codes::UPSTREAM_DISABLED,
                "OCR disabled for this request",
            )),
        ));
    }

    // Validate request
    if request.doc_url.is_empty() {
        METRICS.record_vision_index(false);
        METRICS.vision_request_duration
            .with_label_values(&["index"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "doc_url cannot be empty",
            )),
        ));
    }

    // Use DeepseekOcrClient for indexing
    // Convert HashMap<String, String> to Option<Map<String, Value>>
    let metadata = if request.metadata.is_empty() {
        None
    } else {
        Some(
            request.metadata
                .into_iter()
                .map(|(k, v)| (k, serde_json::Value::String(v)))
                .collect()
        )
    };
    
    match state.deepseek_client.index_document(request.doc_url, metadata).await {
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
                    "OCR service is disabled".to_string(),
                ),
                OcrError::CircuitOpen(_) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    error_codes::UPSTREAM_ERROR,
                    e.to_string(),
                ),
                OcrError::Timeout(_) => (
                    StatusCode::GATEWAY_TIMEOUT,
                    error_codes::TIMEOUT,
                    e.to_string(),
                ),
                _ => (
                    StatusCode::BAD_GATEWAY,
                    error_codes::UPSTREAM_ERROR,
                    e.to_string(),
                ),
            };

            error!("Vision index failed: {}", message);
            Err((status, Json(ApiError::new(code, message))))
        }
    }
}

/// Get job status
///
/// GET /api/v1/vision/index/jobs/{job_id}
pub async fn get_job_status(
    State(state): State<VisionState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> Result<Json<JobStatusResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    
    info!("Job status request: job_id={}", job_id);

    // Check per-request opt-out
    if !should_use_ocr(&headers) {
        warn!("OCR disabled for this request via X-Use-OCR header");
        METRICS.vision_request_duration
            .with_label_values(&["status"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError::new(
                error_codes::UPSTREAM_DISABLED,
                "OCR disabled for this request",
            )),
        ));
    }

    // Validate job_id
    if job_id.is_empty() {
        METRICS.vision_request_duration
            .with_label_values(&["status"])
            .observe(start.elapsed().as_secs_f64());
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "job_id cannot be empty",
            )),
        ));
    }

    // Use DeepseekOcrClient for job status
    match state.deepseek_client.get_job_status(job_id).await {
        Ok(response) => {
            METRICS.vision_request_duration
                .with_label_values(&["status"])
                .observe(start.elapsed().as_secs_f64());
            Ok(Json(response))
        }
        Err(e) => {
            METRICS.vision_request_duration
                .with_label_values(&["status"])
                .observe(start.elapsed().as_secs_f64());
            
            let (status, code, message) = match e {
                OcrError::Disabled => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    error_codes::UPSTREAM_DISABLED,
                    "OCR service is disabled".to_string(),
                ),
                OcrError::Timeout(_) => (
                    StatusCode::GATEWAY_TIMEOUT,
                    error_codes::TIMEOUT,
                    e.to_string(),
                ),
                _ => (
                    StatusCode::BAD_GATEWAY,
                    error_codes::UPSTREAM_ERROR,
                    e.to_string(),
                ),
            };

            error!("Job status check failed: {}", message);
            Err((status, Json(ApiError::new(code, message))))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_use_ocr_default() {
        let headers = HeaderMap::new();
        assert!(should_use_ocr(&headers));
    }

    #[test]
    fn test_should_use_ocr_explicit_true() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Use-OCR", "true".parse().unwrap());
        assert!(should_use_ocr(&headers));
    }

    #[test]
    fn test_should_use_ocr_explicit_false() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Use-OCR", "false".parse().unwrap());
        assert!(!should_use_ocr(&headers));
    }

    #[test]
    fn test_should_use_ocr_numeric() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Use-OCR", "1".parse().unwrap());
        assert!(should_use_ocr(&headers));

        let mut headers = HeaderMap::new();
        headers.insert("X-Use-OCR", "0".parse().unwrap());
        assert!(!should_use_ocr(&headers));
    }
}