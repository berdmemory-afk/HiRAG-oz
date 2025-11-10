//! Vision API route configuration
//!
//! This file contains the vision API routes that should be added to the main router.
//! To integrate, add these routes to the build_router function in routes.rs

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::api::vision::{handlers as vision_handlers, VisionState};
use crate::middleware::{
    auth::AuthMiddleware,
    rate_limiter::RateLimiter,
    BodyLimiter,
};

/// Build vision API routes
///
/// These routes should be merged into the main router in routes.rs
pub fn build_vision_routes(
    vision_state: VisionState,
    rate_limiter: Arc<RateLimiter>,
    auth_middleware: Arc<AuthMiddleware>,
    body_limiter: Arc<BodyLimiter>,
) -> Router {
    Router::new()
        // Vision API endpoints
        .route("/api/v1/vision/search", post(vision_handlers::search_regions))
        .route("/api/v1/vision/decode", post(vision_handlers::decode_regions))
        .route("/api/v1/vision/index", post(vision_handlers::index_document))
        .route("/api/v1/vision/index/jobs/:job_id", get(vision_handlers::get_job_status))
        // Apply middleware layers
        .layer(RequestBodyLimitLayer::new(body_limiter.max_body_size()))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(axum::middleware::from_fn_with_state(
                    rate_limiter,
                    super::routes::rate_limit_middleware,
                ))
                .layer(axum::middleware::from_fn_with_state(
                    auth_middleware,
                    super::routes::auth_middleware_fn,
                ))
        )
        .with_state(vision_state)
}

/// Example integration into main router:
///
/// ```rust,ignore
/// // In routes.rs build_router function:
/// 
/// // Create vision client and state
/// let vision_client = VisionServiceClient::default()?;
/// let vision_state = VisionState {
///     client: Arc::new(vision_client),
/// };
/// 
/// // Build vision routes
/// let vision_routes = build_vision_routes(
///     vision_state,
///     rate_limiter.clone(),
///     auth_middleware.clone(),
///     body_limiter.clone(),
/// );
/// 
/// // Merge with existing routes
/// public_routes.merge(api_routes).merge(vision_routes)
/// ```