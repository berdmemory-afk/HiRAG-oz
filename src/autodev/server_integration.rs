//! Example server integration for autonomous development API
//!
//! This module shows how to integrate the autodev routes into your main application.

use crate::autodev::{init_autodev, AutodevConfig};
use crate::autodev::api::build_autodev_routes;
use axum::Router;
use std::sync::Arc;

/// Build complete application router with autodev routes
///
/// # Example
///
/// ```rust,no_run
/// use context_manager::autodev::server_integration::build_app_with_autodev;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let app = build_app_with_autodev().await?;
///     
///     let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
///     axum::serve(listener, app).await?;
///     
///     Ok(())
/// }
/// ```
pub async fn build_app_with_autodev() -> anyhow::Result<Router> {
    // Build your existing application router
    let mut app = Router::new();
    // Add your existing routes here...
    // app = app.route("/api/v1/health", get(health_check));
    // etc.
    
    // Initialize autodev if enabled
    let autodev_cfg = AutodevConfig::from_env();
    
    if autodev_cfg.enabled {
        tracing::info!("Initializing autonomous development system");
        
        let orchestrator = Arc::new(init_autodev(autodev_cfg).await?);
        let autodev_routes = build_autodev_routes(orchestrator);
        
        // Merge autodev routes (they already have /api/v1/autodev prefix)
        app = app.merge(autodev_routes);
        
        tracing::info!("Autonomous development routes mounted");
    } else {
        tracing::info!("Autonomous development system is disabled");
    }
    
    Ok(app)
}

/// Alternative: Nest autodev routes under a specific path
///
/// # Example
///
/// ```rust,no_run
/// use context_manager::autodev::server_integration::build_app_with_nested_autodev;
///
/// #[tokio::main]
/// async fn main() -> anyhow::Result<()> {
///     let app = build_app_with_nested_autodev().await?;
///     
///     let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
///     axum::serve(listener, app).await?;
///     
///     Ok(())
/// }
/// ```
pub async fn build_app_with_nested_autodev() -> anyhow::Result<Router> {
    let mut app = Router::new();
    
    let autodev_cfg = AutodevConfig::from_env();
    
    if autodev_cfg.enabled {
        let orchestrator = Arc::new(init_autodev(autodev_cfg).await?);
        let autodev_routes = build_autodev_routes(orchestrator);
        
        // Nest under a specific path (routes will be /autodev/api/v1/autodev/tasks)
        app = app.nest("/autodev", autodev_routes);
    }
    
    Ok(app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_app_with_autodev() {
        // This will use default config (may be disabled)
        let result = build_app_with_autodev().await;
        assert!(result.is_ok());
    }
}