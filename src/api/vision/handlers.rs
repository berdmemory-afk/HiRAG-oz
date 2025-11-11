//! Vision API handlers

use super::client::VisionServiceClient;
use super::deepseek_client::{DeepseekOcrClient, OcrError};
use super::models::*;
use crate::metrics::METRICS;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info, warn};

/// Application state for vision handlers
#[derive(Clone)]
pub struct VisionState {
    pub client: Arc<VisionServiceClient>,
    pub deepseek_client: Arc<DeepseekOcrClient>,
}

/// Check if OCR should be used for this request
fn should_use_ocr(headers: &amp;HeaderMap) -> bool {
    headers
        .get("X-Use-OCR")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_lowercase() != "false")
        .unwrap_or(true)
}

/// Search regions by query
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
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "top_k cannot exceed 50",
            )),
        ));
    }

    // Call vision service
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
    if !should_use_ocr(&amp;headers) {
        warn!("OCR disabled for this request via X-Use-OCR header");
        METRICS.record_vision_decode(false);
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
    if !should_use_ocr(&amp;headers) {
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

    // Validate request
    if request.doc_url.is_empty() {
        METRICS.record_vision_index(false);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "doc_url cannot be empty",
            )),
        ));
    }

    // Call vision service
    match state.client.index_document(request).await {
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
            error!("Vision index failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(error_codes::INTERNAL_ERROR, e.to_string())),
            ))
        }
    }
}

/// Get job status
///
/// GET /api/v1/vision/index/jobs/{job_id}
pub async fn get_job_status(
    State(state): State<VisionState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobStatusResponse>, (StatusCode, Json<ApiError>)> {
    info!("Job status request: job_id={}", job_id);

    // Validate job_id
    if job_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "job_id cannot be empty",
            )),
        ));
    }

    // Call vision service
    match state.client.get_job_status(&job_id).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Job status check failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(error_codes::INTERNAL_ERROR, e.to_string())),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::vision::client::VisionServiceClient;

    fn create_test_state() -> VisionState {
        let client = VisionServiceClient::default().unwrap();
        VisionState {
            client: Arc::new(client),
        }
    }

    #[tokio::test]
    async fn test_search_regions_handler() {
        let state = create_test_state();
        let request = VisionSearchRequest {
            query: "test query".to_string(),
            top_k: 10,
            filters: Default::default(),
        };

        let result = search_regions(State(state), Json(request)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_search_regions_empty_query() {
        let state = create_test_state();
        let request = VisionSearchRequest {
            query: "".to_string(),
            top_k: 10,
            filters: Default::default(),
        };

        let result = search_regions(State(state), Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_search_regions_top_k_too_large() {
        let state = create_test_state();
        let request = VisionSearchRequest {
            query: "test".to_string(),
            top_k: 100,
            filters: Default::default(),
        };

        let result = search_regions(State(state), Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_regions_handler() {
        let state = create_test_state();
        let request = DecodeRequest {
            region_ids: vec!["r_1".to_string()],
            fidelity: FidelityLevel::Balanced,
        };

        let result = decode_regions(State(state), Json(request)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_decode_regions_empty() {
        let state = create_test_state();
        let request = DecodeRequest {
            region_ids: vec![],
            fidelity: FidelityLevel::Balanced,
        };

        let result = decode_regions(State(state), Json(request)).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_index_document_handler() {
        let state = create_test_state();
        let request = IndexRequest {
            doc_url: "s3://docs/test.pdf".to_string(),
            metadata: Default::default(),
            force_reindex: false,
        };

        let result = index_document(State(state), Json(request)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_get_job_status_handler() {
        let state = create_test_state();
        let result = get_job_status(State(state), Path("job_123".to_string())).await;
        assert!(result.is_ok());
    }
}