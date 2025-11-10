//! Vision API handlers

use super::client::VisionServiceClient;
use super::models::*;
use crate::metrics::METRICS;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use std::time::Instant;
use tracing::{error, info};

/// Application state for vision handlers
#[derive(Clone)]
pub struct VisionState {
    pub client: Arc<VisionServiceClient>,
}

/// Search regions by query
///
/// POST /api/v1/vision/search
pub async fn search_regions(
    State(state): State<VisionState>,
    Json(request): Json<VisionSearchRequest>,
) -> Result<Json<VisionSearchResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    METRICS.vision_search_requests.inc();
    
    info!("Vision search request: query={}", request.query);

    // Validate request
    if request.query.is_empty() {
        METRICS.vision_search_errors.inc();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "Query cannot be empty",
            )),
        ));
    }

    if request.top_k > 50 {
        METRICS.vision_search_errors.inc();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "top_k cannot exceed 50",
            )),
        ));
    }

    // Call vision service
    let result = match state.client.search_regions(request).await {
        Ok(response) => {
            METRICS.vision_search_duration.observe(start.elapsed().as_secs_f64());
            Ok(Json(response))
        }
        Err(e) => {
            METRICS.vision_search_errors.inc();
            error!("Vision search failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(error_codes::INTERNAL_ERROR, e.to_string())),
            ))
        }
    };
    
    result
}

/// Decode regions to text
///
/// POST /api/v1/vision/decode
pub async fn decode_regions(
    State(state): State<VisionState>,
    Json(request): Json<DecodeRequest>,
) -> Result<Json<DecodeResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    METRICS.vision_decode_requests.inc();
    
    info!("Vision decode request: {} regions", request.region_ids.len());

    // Validate request
    if request.region_ids.is_empty() {
        METRICS.vision_decode_errors.inc();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "region_ids cannot be empty",
            )),
        ));
    }

    if request.region_ids.len() > 16 {
        METRICS.vision_decode_errors.inc();
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

    // Call vision service
    let result = match state.client.decode_regions(request).await {
        Ok(response) => {
            METRICS.vision_decode_duration.observe(start.elapsed().as_secs_f64());
            Ok(Json(response))
        }
        Err(e) => {
            METRICS.vision_decode_errors.inc();
            error!("Vision decode failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(error_codes::INTERNAL_ERROR, e.to_string())),
            ))
        }
    };
    
    result
}

/// Index a document
///
/// POST /api/v1/vision/index
pub async fn index_document(
    State(state): State<VisionState>,
    Json(request): Json<IndexRequest>,
) -> Result<Json<IndexResponse>, (StatusCode, Json<ApiError>)> {
    let start = Instant::now();
    METRICS.vision_index_requests.inc();
    
    info!("Vision index request: doc_url={}", request.doc_url);

    // Validate request
    if request.doc_url.is_empty() {
        METRICS.vision_index_errors.inc();
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::new(
                error_codes::VALIDATION_ERROR,
                "doc_url cannot be empty",
            )),
        ));
    }

    // Call vision service
    let result = match state.client.index_document(request).await {
        Ok(response) => {
            METRICS.vision_index_duration.observe(start.elapsed().as_secs_f64());
            Ok(Json(response))
        }
        Err(e) => {
            METRICS.vision_index_errors.inc();
            error!("Vision index failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::new(error_codes::INTERNAL_ERROR, e.to_string())),
            ))
        }
    };
    
    result
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