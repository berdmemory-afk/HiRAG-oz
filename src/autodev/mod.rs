//! Autonomous software development system
//!
//! This module implements a complete autonomous software development system
//! capable of planning, coding, testing, verifying, and creating PRs.

pub mod schemas;
pub mod tools;
pub mod orchestrator;
pub mod config;
pub mod metrics;
pub mod api;
pub mod server_integration;

pub use schemas::{Task, Plan, Step, RiskTier, TaskStatus, CreateTaskRequest};
pub use orchestrator::Orchestrator;
pub use config::AutodevConfig;
pub use metrics::AUTODEV_METRICS;

use anyhow::Result;
use std::sync::Arc;
use tracing::info;

/// Initialize the autonomous development system
pub async fn init_autodev(config: AutodevConfig) -> Result<Orchestrator> {
    info!("Initializing autonomous development system");
    
    if !config.enabled {
        info!("Autonomous development is disabled");
    }
    
    // Initialize tools
    let tools = create_tools(&config)?;
    
    // Create orchestrator
    let orchestrator = Orchestrator::new(tools, config);
    
    info!("Autonomous development system initialized");
    
    Ok(orchestrator)
}

/// Create all available tools
fn create_tools(config: &AutodevConfig) -> Result<Vec<Arc<dyn tools::Tool>>> {
    let mut tools: Vec<Arc<dyn tools::Tool>> = Vec::new();
    
    // Git tools
    tools.push(Arc::new(tools::git::GitCloneTool));
    tools.push(Arc::new(tools::git::GitTool::new(
        config.git.git_author_name.clone(),
        config.git.git_author_email.clone(),
    )));
    
    // GitHub PR and push tools (if token available)
    if let Ok(token) = std::env::var(&config.git.github_token_env) {
        tools.push(Arc::new(tools::git::GitHubPrTool::new(token)));
        tools.push(Arc::new(tools::git::GitPushTool::new(config.git.github_token_env.clone())));
    }
    
    // Runner tools
    tools.push(Arc::new(tools::runner::RunnerTool::new(
        config.sandbox_image.clone(),
        config.runner_timeout_secs as u64,
    )));
    tools.push(Arc::new(tools::runner::BuildTool::new(
        config.sandbox_image.clone(),
        config.runner_timeout_secs as u64,
    )));
    tools.push(Arc::new(tools::runner::TestTool::new(
        config.sandbox_image.clone(),
        config.runner_timeout_secs as u64,
    )));
    
    // Codegen tool (if API key available)
    if let Ok(api_key) = std::env::var(&config.llm.api_key_env) {
        tools.push(Arc::new(tools::codegen::CodegenTool::new(
            api_key,
            config.llm.model.clone(),
            config.llm.api_url.clone(),
            config.llm.max_tokens,
            config.llm.temperature,
        )));
    }
    
    // Policy tools
    if let Some(ref opa_url) = config.opa_url {
        tools.push(Arc::new(tools::policy::PolicyTool::new(
            opa_url.clone(),
            config.policy_package.clone(),
        )));
    }
    tools.push(Arc::new(tools::policy::LocalPolicyTool::new()));
    
    // Search tools
    tools.push(Arc::new(tools::search::RepoSearchTool::new(100)));
    tools.push(Arc::new(tools::search::FileListTool::new()));
    
    // Static analysis tools
    tools.push(Arc::new(tools::static_analysis::ClippyTool::new(
        config.sandbox_image.clone(),
    )));
    tools.push(Arc::new(tools::static_analysis::SecretsScanner::new()));
    tools.push(Arc::new(tools::static_analysis::DependencyChecker::new()));
    
    info!("Initialized {} tools", tools.len());
    
    Ok(tools)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_init_autodev() {
        let config = AutodevConfig::default();
        let result = init_autodev(config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_tools() {
        let config = AutodevConfig::default();
        let tools = create_tools(&config).unwrap();
        assert!(!tools.is_empty());
    }
}