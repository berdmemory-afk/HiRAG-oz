//! Configuration for autonomous software development

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Autonomous development configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutodevConfig {
    /// Global enable/disable
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    
    /// Git provider (github, gitlab)
    #[serde(default = "default_provider")]
    pub provider: String,
    
    /// Maximum parallel tasks
    #[serde(default = "default_max_parallel")]
    pub max_parallel_tasks: usize,
    
    /// Maximum step retries
    #[serde(default = "default_max_retries")]
    pub max_step_retries: u32,
    
    /// Default risk tier
    #[serde(default = "default_risk_tier")]
    pub default_risk_tier: String,
    
    /// Sandbox Docker image
    #[serde(default = "default_sandbox_image")]
    pub sandbox_image: String,
    
    /// Runner timeout in seconds
    #[serde(default = "default_runner_timeout")]
    pub runner_timeout_secs: u32,
    
    /// OPA URL
    #[serde(default)]
    pub opa_url: Option<String>,
    
    /// Policy package
    #[serde(default = "default_policy_package")]
    pub policy_package: String,
    
    /// Allowed repositories (glob patterns)
    #[serde(default)]
    pub allowlist_repos: Vec<String>,
    
    /// LLM configuration
    #[serde(default)]
    pub llm: LlmConfig,
    
    /// Git configuration
    #[serde(default)]
    pub git: GitConfig,
}

fn default_enabled() -> bool {
    true
}

fn default_provider() -> String {
    "github".to_string()
}

fn default_max_parallel() -> usize {
    4
}

fn default_max_retries() -> u32 {
    2
}

fn default_risk_tier() -> String {
    "low".to_string()
}

fn default_sandbox_image() -> String {
    "rust:1.82".to_string()
}

fn default_runner_timeout() -> u32 {
    1200
}

fn default_policy_package() -> String {
    "autodev/merge".to_string()
}

impl Default for AutodevConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            provider: default_provider(),
            max_parallel_tasks: default_max_parallel(),
            max_step_retries: default_max_retries(),
            default_risk_tier: default_risk_tier(),
            sandbox_image: default_sandbox_image(),
            runner_timeout_secs: default_runner_timeout(),
            opa_url: None,
            policy_package: default_policy_package(),
            allowlist_repos: vec![],
            llm: LlmConfig::default(),
            git: GitConfig::default(),
        }
    }
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider (openai, azure, openrouter)
    #[serde(default = "default_llm_provider")]
    pub provider: String,
    
    /// Model name
    #[serde(default = "default_llm_model")]
    pub model: String,
    
    /// API key environment variable
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    
    /// API URL
    #[serde(default = "default_api_url")]
    pub api_url: String,
    
    /// Max tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    
    /// Temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

fn default_llm_provider() -> String {
    "openai".to_string()
}

fn default_llm_model() -> String {
    "gpt-4".to_string()
}

fn default_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_api_url() -> String {
    "https://api.openai.com/v1/chat/completions".to_string()
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_temperature() -> f32 {
    0.2
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: default_llm_provider(),
            model: default_llm_model(),
            api_key_env: default_api_key_env(),
            api_url: default_api_url(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
        }
    }
}

/// Git configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConfig {
    /// GitHub token environment variable
    #[serde(default = "default_github_token_env")]
    pub github_token_env: String,
    
    /// Git author name
    #[serde(default = "default_git_author_name")]
    pub git_author_name: String,
    
    /// Git author email
    #[serde(default = "default_git_author_email")]
    pub git_author_email: String,
}

fn default_github_token_env() -> String {
    "GITHUB_TOKEN".to_string()
}

fn default_git_author_name() -> String {
    "AutoDev Bot".to_string()
}

fn default_git_author_email() -> String {
    "autodev@example.com".to_string()
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            github_token_env: default_github_token_env(),
            git_author_name: default_git_author_name(),
            git_author_email: default_git_author_email(),
        }
    }
}

impl AutodevConfig {
    /// Load from environment variables
    pub fn from_env() -> Self {
        let mut config = Self::default();
        
        if let Ok(val) = std::env::var("AUTODEV_ENABLED") {
            config.enabled = val.to_lowercase() == "true" || val == "1";
        }
        
        if let Ok(val) = std::env::var("AUTODEV_PROVIDER") {
            config.provider = val;
        }
        
        if let Ok(val) = std::env::var("AUTODEV_MAX_PARALLEL") {
            if let Ok(num) = val.parse() {
                config.max_parallel_tasks = num;
            }
        }
        
        if let Ok(val) = std::env::var("AUTODEV_SANDBOX_IMAGE") {
            config.sandbox_image = val;
        }
        
        if let Ok(val) = std::env::var("OPA_URL") {
            config.opa_url = Some(val);
        }
        
        if let Ok(val) = std::env::var("AUTODEV_ALLOWED_REPOS") {
            config.allowlist_repos = val.split(',').map(|s| s.trim().to_string()).collect();
        }
        
        config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AutodevConfig::default();
        assert!(config.enabled);
        assert_eq!(config.provider, "github");
        assert_eq!(config.max_parallel_tasks, 4);
    }

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, "openai");
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.max_tokens, 4096);
    }

    #[test]
    fn test_git_config_default() {
        let config = GitConfig::default();
        assert_eq!(config.git_author_name, "AutoDev Bot");
    }
}