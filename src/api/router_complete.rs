//! Complete router builder with all integrated routes
//!
//! This module provides a complete router that includes base routes,
//! vision routes, and facts routes with proper middleware.

use axum::Router;
use std::sync::Arc;
use qdrant_client::client::QdrantClient;

use crate::{
    api::{
        handlers::AppState,
        integration::{init_vision_service, init_facts_store, build_vision_routes, build_facts_routes},
        routes::build_router,
    },
    config::Config,
    error::Result,
    middleware::{
        auth::AuthMiddleware,
        rate_limiter::RateLimiter,
        BodyLimiter,
    },
    observability::{HealthChecker, MetricsCollector},
};

/// Build complete router with all features integrated
pub async fn build_complete_router(
    app_state: AppState,
    config: Config,
    health_checker: Arc<HealthChecker>,
    metrics: Arc<MetricsCollector>,
    rate_limiter: Arc<RateLimiter>,
    auth_middleware: Arc<AuthMiddleware>,
    body_limiter: Arc<BodyLimiter>,
    qdrant_client: QdrantClient,
) -> Result<Router> {
    // Build base router with existing context management endpoints
    let base_router = build_router(
        app_state,
        health_checker,
        metrics,
        rate_limiter.clone(),
        auth_middleware.clone(),
        body_limiter.clone(),
    );
    
    // Initialize and build vision routes
    let vision_state = init_vision_service(&config).await?;
    let vision_routes = build_vision_routes(
        vision_state,
        rate_limiter.clone(),
        auth_middleware.clone(),
        body_limiter.clone(),
    );
    
    // Initialize and build facts routes
    let facts_state = init_facts_store(&config, qdrant_client).await?;
    let facts_routes = build_facts_routes(
        facts_state,
        rate_limiter.clone(),
        auth_middleware.clone(),
        body_limiter.clone(),
    );
    
    // Merge all routes
    Ok(base_router
        .merge(vision_routes)
        .merge(facts_routes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_router_builder_signature() {
        // This test just verifies the function signature compiles
        // Actual testing requires running services
    }
}