//! Tool abstractions for autonomous software development

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

pub mod git;
pub mod runner;
pub mod codegen;
pub mod policy;
pub mod search;
pub mod static_analysis;

/// Tool execution context
#[derive(Clone, Debug)]
pub struct ToolContext {
    /// Working directory for this task
    pub workdir: PathBuf,
    /// Repository URL
    pub repo_url: String,
    /// Base branch
    pub base_branch: String,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Execution timeout
    pub timeout: Duration,
    /// Task ID for tracking
    pub task_id: uuid::Uuid,
}

/// Tool execution errors
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("Execution error: {0}")]
    Exec(String),
    
    #[error("Policy denied: {0}")]
    Policy(String),
    
    #[error("Invalid input: {0}")]
    Invalid(String),
    
    #[error("Upstream error: {0}")]
    Upstream(String),
    
    #[error("Timeout after {0:?}")]
    Timeout(Duration),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Git error: {0}")]
    Git(String),
    
    #[error("Build failed: {0}")]
    Build(String),
    
    #[error("Tests failed: {0}")]
    TestFailed(String),
}

/// Tool trait for autonomous operations
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (must be unique)
    fn name(&self) -> &'static str;
    
    /// Tool description
    fn description(&self) -> &'static str {
        "No description available"
    }
    
    /// Execute the tool with given input
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError>;
    
    /// Validate input before execution (optional)
    fn validate_input(&self, _input: &Value) -> Result<(), ToolError> {
        Ok(())
    }
}

/// Tool registry for managing available tools
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }
    
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }
    
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }
    
    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTool;
    
    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &'static str {
            "mock"
        }
        
        async fn invoke(&self, input: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
            Ok(input)
        }
    }

    #[test]
    fn test_tool_registry() {
        let mut registry = ToolRegistry::new();
        registry.register(Box::new(MockTool));
        
        assert!(registry.get("mock").is_some());
        assert!(registry.get("nonexistent").is_none());
        assert_eq!(registry.list().len(), 1);
    }
}