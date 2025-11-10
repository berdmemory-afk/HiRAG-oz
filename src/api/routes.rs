//! API route configuration

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::{
    middleware::{
        auth::AuthMiddleware,
        rate_limiter::RateLimiter,
        BodyLimiter,
    },
    observability::{HealthChecker, MetricsCollector},
};
use tower_http::limit::RequestBodyLimitLayer;

use super::handlers::{self, AppState};



/// Build the complete API router with middleware
pub fn build_router(
    app_state: AppState,
    _health_checker: Arc<HealthChecker>,
    metrics: Arc<MetricsCollector>,
    rate_limiter: Arc<RateLimiter>,
    auth_middleware: Arc<AuthMiddleware>,
    body_limiter: Arc<BodyLimiter>,
) -> Router {
    // Public routes (no auth)
    let public_routes = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/health/live", get(liveness_handler))
        .route("/health/ready", get(readiness_handler))
        .route("/metrics", get(metrics_handler))
        .with_state((app_state.clone(), metrics.clone()));

    // Protected API routes (with auth + rate limiting + body size limit)
    let api_routes = Router::new()
        .route("/api/v1/contexts", post(handlers::store_context))
        .route("/api/v1/contexts/search", post(handlers::search_contexts))
        .route("/api/v1/contexts/delete", post(handlers::delete_context))
        .route("/api/v1/contexts/clear", post(handlers::clear_level))
        .layer(RequestBodyLimitLayer::new(body_limiter.max_body_size()))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(axum::middleware::from_fn_with_state(
                    rate_limiter,
                    rate_limit_middleware,
                ))
                .layer(axum::middleware::from_fn_with_state(
                    auth_middleware,
                    auth_middleware_fn,
                ))
        )
        .with_state(app_state);

    // Combine routes
    public_routes.merge(api_routes)
}

/// Root handler
async fn root_handler() -> impl axum::response::IntoResponse {
    use axum::Json;
    Json(serde_json::json!({
        "service": "Context Manager",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "running"
    }))
}

/// Health check handler
async fn health_handler(
    axum::extract::State((app_state, _)): axum::extract::State<(AppState, Arc<MetricsCollector>)>,
) -> impl axum::response::IntoResponse {
    use axum::{http::StatusCode, Json};
    
    let status = app_state.health_checker.check_health().await;
    let status_code = match status.status {
        crate::observability::HealthStatus::Healthy => StatusCode::OK,
        crate::observability::HealthStatus::Degraded => StatusCode::OK,
        crate::observability::HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };

    (status_code, Json(status))
}

/// Liveness probe handler - always returns 200
async fn liveness_handler() -> impl axum::response::IntoResponse {
    use axum::{http::StatusCode, Json};
    use serde_json::json;
    
    (StatusCode::OK, Json(json!({"status": "alive"})))
}

/// Readiness probe handler - checks if service is ready to serve traffic
async fn readiness_handler(
    axum::extract::State((app_state, _)): axum::extract::State<(AppState, Arc<MetricsCollector>)>,
) -> impl axum::response::IntoResponse {
    use axum::{http::StatusCode, Json};
    use serde_json::json;
    
    let status = app_state.health_checker.check_health().await;
    let status_code = match status.status {
        crate::observability::HealthStatus::Healthy => StatusCode::OK,
        crate::observability::HealthStatus::Degraded => StatusCode::OK,
        crate::observability::HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };
    
    let readiness_status = match status_code {
        StatusCode::OK => "ready",
        _ => "not_ready",
    };
    
    (status_code, Json(json!({"status": readiness_status, "details": status})))
}

/// Metrics handler
async fn metrics_handler(
    axum::extract::State((app_state, metrics)): axum::extract::State<(AppState, Arc<MetricsCollector>)>,
) -> impl axum::response::IntoResponse {
    use crate::metrics::METRICS;
    
    // Export core metrics
    let mut output = metrics.export_prometheus();
    
    // Append new production metrics from METRICS singleton
    output.push_str("\n\n# Production Infrastructure Metrics\n");
    output.push_str(&METRICS.export_prometheus());
    
    // Append circuit breaker metrics if available
    if let Some(circuit_breaker) = &app_state.circuit_breaker {
        output.push_str("\n\n");
        output.push_str(&circuit_breaker.export_prometheus("vector_db_circuit_breaker").await);
    }
    
    // Return as plain text for Prometheus scraping
    output
}

/// Rate limiting middleware
async fn rate_limit_middleware(
    axum::extract::State(rate_limiter): axum::extract::State<Arc<RateLimiter>>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    use axum::http::HeaderValue;
    
    // Extract client ID from IP or header
    let client_id = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            req.extensions()
                .get::<std::net::SocketAddr>()
                .map(|addr| addr.ip().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    match rate_limiter.check_rate_limit(&client_id).await {
        Ok(_) => {
            // Record successful rate limit check in metrics
            crate::metrics::METRICS.record_rate_limit(&client_id, true);
            
            let mut response = next.run(req).await;
            
            // Add rate limit headers
            if let Some((count, _elapsed)) = rate_limiter.get_usage(&client_id).await {
                let stats = rate_limiter.stats().await;
                let limit = stats.config.max_requests;
                let remaining = limit.saturating_sub(count);
                
                if let Ok(limit_val) = HeaderValue::from_str(&limit.to_string()) {
                    response.headers_mut().insert("X-RateLimit-Limit", limit_val);
                }
                if let Ok(remaining_val) = HeaderValue::from_str(&remaining.to_string()) {
                    response.headers_mut().insert("X-RateLimit-Remaining", remaining_val);
                }
                // Reset time is window_duration from now
                let reset_secs = stats.config.window_duration.as_secs();
                if let Ok(reset_val) = HeaderValue::from_str(&reset_secs.to_string()) {
                    response.headers_mut().insert("X-RateLimit-Reset", reset_val);
                }
            }
            
            Ok(response)
        }
        Err(e) => {
            tracing::warn!("Rate limit exceeded for {}: {}", client_id, e);
            
            // Build 429 response with rate limit headers and JSON error body
            use axum::response::IntoResponse;
            use serde_json::json;
            
            let stats = rate_limiter.stats().await;
            let limit = stats.config.max_requests;
            
            // Calculate actual seconds remaining until reset
            let reset_secs = if let Some((_, elapsed)) = rate_limiter.get_usage(&client_id).await {
                stats.config.window_duration
                    .saturating_sub(elapsed)
                    .as_secs()
            } else {
                stats.config.window_duration.as_secs()
            };
            
            // Create ApiError for consistency with other endpoints
            // Note: We use json! here to avoid importing vision models in routes
            // In production, consider creating a shared error module
            let error_body = json!({
                "code": "RATE_LIMIT",
                "message": format!("Rate limit exceeded. Retry after {} seconds.", reset_secs),
                "details": {
                    "limit": limit,
                    "remaining": 0,
                    "reset_in_seconds": reset_secs
                }
            });
            
            let mut response = (
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                axum::Json(error_body)
            ).into_response();
            
            // Record rate limit hit in metrics
            crate::metrics::METRICS.record_rate_limit(&client_id, false);
            
            // Add rate limit headers to 429 response
            if let Ok(limit_val) = HeaderValue::from_str(&limit.to_string()) {
                response.headers_mut().insert("X-RateLimit-Limit", limit_val);
            }
            // Remaining is 0 when rate limited
            if let Ok(remaining_val) = HeaderValue::from_str("0") {
                response.headers_mut().insert("X-RateLimit-Remaining", remaining_val);
            }
            if let Ok(reset_val) = HeaderValue::from_str(&reset_secs.to_string()) {
                response.headers_mut().insert("X-RateLimit-Reset", reset_val);
            }
            
            Ok(response)
        }
    }
}

/// Authentication middleware
async fn auth_middleware_fn(
    axum::extract::State(auth): axum::extract::State<Arc<AuthMiddleware>>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    // Extract token from Authorization header
    let token = req
        .headers()
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));

    match token {
        Some(token) => {
            if auth.validate_token(token) {
                Ok(next.run(req).await)
            } else {
                tracing::warn!("Invalid authentication token");
                Err(axum::http::StatusCode::UNAUTHORIZED)
            }
        }
        None => {
            tracing::warn!("Missing authentication token");
            Err(axum::http::StatusCode::UNAUTHORIZED)
        }
    }
}