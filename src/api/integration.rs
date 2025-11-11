//! Integration module for wiring all API routes together
//!
//! This module provides functions to integrate the new vision and facts
//! routes into the main application router.

use axum::Router;
use std::sync::Arc;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tower_http::limit::RequestBodyLimitLayer;

use crate::{
    api::{
        handlers::AppState,
        vision::{VisionServiceClient, VisionState},
    },
    config::Config,
    facts::{FactStore, FactStoreConfig, FactsState},
    middleware::{
        auth::AuthMiddleware,
        rate_limiter::RateLimiter,
        BodyLimiter,
    },
};
use axum::routing::{get, post};

/// Build vision API routes
pub fn build_vision_routes(
    vision_state: VisionState,
    rate_limiter: Arc<RateLimiter>,
    auth_middleware: Arc<AuthMiddleware>,
    body_limiter: Arc<BodyLimiter>,
) -> Router {
    use crate::api::vision::handlers;
    use crate::api::routes::{rate_limit_middleware, auth_middleware_fn};
    
    Router::new()
        .route("/api/v1/vision/search", post(handlers::search_regions))
        .route("/api/v1/vision/decode", post(handlers::decode_regions))
        .route("/api/v1/vision/index", post(handlers::index_document))
        .route("/api/v1/vision/index/jobs/:job_id", get(handlers::get_job_status))
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
        .with_state(vision_state)
}

/// Build facts API routes
pub fn build_facts_routes(
    facts_state: FactsState,
    rate_limiter: Arc<RateLimiter>,
    auth_middleware: Arc<AuthMiddleware>,
    body_limiter: Arc<BodyLimiter>,
) -> Router {
    use crate::facts::handlers;
    use crate::api::routes::{rate_limit_middleware, auth_middleware_fn};
    
    Router::new()
        .route("/api/v1/facts", post(handlers::insert_fact))
        .route("/api/v1/facts/query", post(handlers::query_facts))
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
        .with_state(facts_state)
}

/// Initialize vision service from configuration
pub async fn init_vision_service(
    config: &Config,
) -> crate::error::Result<VisionState> {
    use crate::api::vision::client::VisionServiceConfig;
    use crate::api::vision::deepseek_client::DeepseekOcrClient;
    use crate::api::vision::deepseek_config::DeepseekConfig;
    use std::time::Duration;
    
    let vision_config = if let Some(ref cfg) = config.vision {
        VisionServiceConfig {
            service_url: cfg.service_url.clone(),
            timeout: Duration::from_millis(cfg.timeout_ms),
            max_regions_per_request: cfg.max_regions_per_request,
        }
    } else {
        VisionServiceConfig::default()
    };
    
    let client = VisionServiceClient::new(vision_config)?;
    
    // Initialize DeepseekOcrClient from environment variables
    let deepseek_config = DeepseekConfig::default().from_env();
    let deepseek_client = DeepseekOcrClient::new(deepseek_config)
        .map_err(|e| crate::error::Error::Internal(format!("Failed to create DeepseekOcrClient: {}", e)))?;
    
    Ok(VisionState {
        client: Arc::new(client),
        deepseek_client: Arc::new(deepseek_client),
    })
}

/// Initialize facts store from configuration
pub async fn init_facts_store(
    config: &Config,
    qdrant_client: qdrant_client::client::QdrantClient,
) -> crate::error::Result<FactsState> {
    let facts_config = if let Some(ref cfg) = config.facts {
        FactStoreConfig {
            collection_name: cfg.collection_name.clone(),
            dedup_enabled: cfg.dedup_enabled,
            confidence_threshold: cfg.confidence_threshold,
            max_facts_per_query: cfg.max_facts_per_query,
            vector_size: config.vector_db.vector_size,
        }
    } else {
        FactStoreConfig {
            vector_size: config.vector_db.vector_size,
            ..Default::default()
        }
    };
    
    let store = FactStore::new(qdrant_client, facts_config).await?;
    
    Ok(FactsState {
        store: Arc::new(store),
    })
}

/// Example integration into main router
///
/// ```rust,ignore
/// use crate::api::integration::{init_vision_service, init_facts_store, build_vision_routes, build_facts_routes};
///
/// // In your main router building function:
/// pub async fn build_complete_router(
///     app_state: AppState,
///     config: Config,
///     health_checker: Arc<HealthChecker>,
///     metrics: Arc<MetricsCollector>,
///     rate_limiter: Arc<RateLimiter>,
///     auth_middleware: Arc<AuthMiddleware>,
///     body_limiter: Arc<BodyLimiter>,
///     qdrant_client: QdrantClient,
/// ) -> Result<Router> {
///     // Build base routes
///     let base_router = build_router(
///         app_state,
///         health_checker,
///         metrics,
///         rate_limiter.clone(),
///         auth_middleware.clone(),
///         body_limiter.clone(),
///     );
///     
///     // Initialize and add vision routes
///     let vision_state = init_vision_service(&config).await?;
///     let vision_routes = build_vision_routes(
///         vision_state,
///         rate_limiter.clone(),
///         auth_middleware.clone(),
///         body_limiter.clone(),
///     );
///     
///     // Initialize and add facts routes
///     let facts_state = init_facts_store(&config, qdrant_client).await?;
///     let facts_routes = build_facts_routes(
///         facts_state,
///         rate_limiter.clone(),
///         auth_middleware.clone(),
///         body_limiter.clone(),
///     );
///     
///     // Merge all routes
///     Ok(base_router.merge(vision_routes).merge(facts_routes))
/// }
/// ```

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_config_defaults() {
        use crate::config::VisionConfig;
        let config = VisionConfig::default();
        assert_eq!(config.service_url, "http://localhost:8080");
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_regions_per_request, 16);
        assert_eq!(config.default_fidelity, "10x");
    }

    #[test]
    fn test_facts_config_defaults() {
        use crate::config::FactsConfig;
        let config = FactsConfig::default();
        assert_eq!(config.collection_name, "facts");
        assert_eq!(config.dedup_enabled, true);
        assert_eq!(config.confidence_threshold, 0.8);
        assert_eq!(config.max_facts_per_query, 100);
    }

    #[test]
    fn test_token_budget_config_defaults() {
        use crate::config::TokenBudgetConfig;
        let config = TokenBudgetConfig::default();
        assert_eq!(config.system_tokens, 700);
        assert_eq!(config.running_brief, 1200);
        assert_eq!(config.recent_turns, 450);
        assert_eq!(config.retrieved_context, 3750);
        assert_eq!(config.completion, 1000);
        assert_eq!(config.max_total, 8000);
    }
}