//! Example: Using the complete router with all features
//!
//! This example demonstrates how to build and use the complete router
//! with token budget management, vision API, and facts store.

use context_manager::{
    api::{build_complete_router, handlers::AppState},
    config::Config,
    middleware::{
        auth::{AuthMiddleware, AuthConfig},
        rate_limiter::{RateLimiter, RateLimitConfig},
        BodyLimiter,
    },
    observability::{HealthChecker, MetricsCollector},
    hirag::EnhancedHiRAGManager,
};
use std::sync::Arc;
use qdrant_client::client::QdrantClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::from_file("config.toml")?;

    // Initialize Qdrant client
    let qdrant_client = QdrantClient::from_url(&config.vector_db.url)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create Qdrant client: {}", e))?;

    // Initialize middleware components
    let health_checker = Arc::new(HealthChecker::new());
    let metrics = Arc::new(MetricsCollector::new());
    let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));
    let auth_middleware = Arc::new(AuthMiddleware::new(AuthConfig::default()));
    let body_limiter = Arc::new(BodyLimiter::new(10 * 1024 * 1024)); // 10MB

    // Create app state (simplified - you'd initialize HiRAG manager here)
    let app_state = AppState {
        // Initialize your HiRAG manager and other state
        health_checker: health_checker.clone(),
        circuit_breaker: None,
    };

    // Build complete router with all features
    let router = build_complete_router(
        app_state,
        config.clone(),
        health_checker,
        metrics,
        rate_limiter,
        auth_middleware,
        body_limiter,
        qdrant_client,
    )
    .await?;

    println!("✅ Complete router built successfully!");
    println!("📍 Available endpoints:");
    println!("   - POST /api/v1/contexts");
    println!("   - POST /api/v1/contexts/search");
    println!("   - POST /api/v1/vision/search");
    println!("   - POST /api/v1/vision/decode");
    println!("   - POST /api/v1/vision/index");
    println!("   - GET  /api/v1/vision/index/jobs/{{job_id}}");
    println!("   - POST /api/v1/facts");
    println!("   - POST /api/v1/facts/query");

    // Start server
    let listener = tokio::net::TcpListener::bind(&format!(
        "{}:{}",
        config.server.host, config.server.port
    ))
    .await?;

    println!("🚀 Server listening on {}:{}", config.server.host, config.server.port);

    axum::serve(listener, router).await?;

    Ok(())
}