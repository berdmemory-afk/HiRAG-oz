//! Policy enforcement tool using OPA (Open Policy Agent)

use super::{Tool, ToolContext, ToolError};
use crate::autodev::schemas::{PolicyInput, PolicyDecision};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, info};

/// OPA policy enforcement tool
pub struct PolicyTool {
    opa_url: String,
    policy_package: String,
}

impl PolicyTool {
    pub fn new(opa_url: String, policy_package: String) -> Self {
        Self {
            opa_url,
            policy_package,
        }
    }
    
    async fn check_policy(&self, input: &PolicyInput) -> Result<PolicyDecision, ToolError> {
        let client = reqwest::Client::new();
        
        #[derive(Serialize)]
        struct OpaRequest {
            input: PolicyInput,
        }
        
        #[derive(Deserialize)]
        struct OpaResponse {
            result: PolicyResult,
        }
        
        #[derive(Deserialize)]
        struct PolicyResult {
            allow: bool,
            #[serde(default)]
            deny: Vec<String>,
            #[serde(default)]
            warnings: Vec<String>,
        }
        
        let url = format!("{}/v1/data/{}", self.opa_url, self.policy_package.replace("::", "/"));
        
        debug!("Checking policy at {}", url);
        
        let request = OpaRequest {
            input: input.clone(),
        };
        
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ToolError::Upstream(format!("OPA request failed: {}", e)))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("OPA API error {}: {}", status, text);
            return Err(ToolError::Upstream(format!("OPA API error: {}", status)));
        }
        
        let opa_response: OpaResponse = response.json().await
            .map_err(|e| ToolError::Upstream(format!("Failed to parse OPA response: {}", e)))?;
        
        let decision = PolicyDecision {
            allow: opa_response.result.allow,
            deny_reasons: opa_response.result.deny,
            warnings: opa_response.result.warnings,
        };
        
        if !decision.allow {
            info!("Policy denied: {:?}", decision.deny_reasons);
        } else {
            info!("Policy allowed");
        }
        
        Ok(decision)
    }
}

#[async_trait]
impl Tool for PolicyTool {
    fn name(&self) -> &'static str {
        "policy"
    }
    
    fn description(&self) -> &'static str {
        "Check policy compliance using OPA"
    }
    
    async fn invoke(&self, input: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let policy_input: PolicyInput = serde_json::from_value(input)?;
        
        let decision = self.check_policy(&policy_input).await?;
        
        if !decision.allow {
            return Err(ToolError::Policy(
                decision.deny_reasons.join("; ")
            ));
        }
        
        Ok(serde_json::to_value(decision)?)
    }
}

/// Local policy checker (fallback when OPA is not available)
pub struct LocalPolicyTool;

impl LocalPolicyTool {
    pub fn new() -> Self {
        Self
    }
    
    fn check_local_policy(&self, input: &PolicyInput) -> PolicyDecision {
        let mut deny_reasons = Vec::new();
        let mut warnings = Vec::new();
        
        // High-risk changes require human review
        if matches!(input.risk_tier, crate::autodev::schemas::RiskTier::High) {
            deny_reasons.push("High-risk changes require human review".to_string());
        }
        
        // Secrets found
        if input.secrets_found {
            deny_reasons.push("Secrets detected in changes".to_string());
        }
        
        // Tests must pass
        if !input.tests_passed {
            deny_reasons.push("Tests must pass before merge".to_string());
        }
        
        // Clippy warnings
        if input.clippy_warnings > 0 {
            warnings.push(format!("{} clippy warnings found", input.clippy_warnings));
        }
        
        // New dependencies
        if !input.new_dependencies.is_empty() {
            warnings.push(format!(
                "New dependencies added: {}",
                input.new_dependencies.join(", ")
            ));
        }
        
        // SQL files require approval
        if input.files_changed.iter().any(|f| f.ends_with(".sql")) {
            deny_reasons.push("Database schema changes require DBA approval".to_string());
        }
        
        PolicyDecision {
            allow: deny_reasons.is_empty(),
            deny_reasons,
            warnings,
        }
    }
}

#[async_trait]
impl Tool for LocalPolicyTool {
    fn name(&self) -> &'static str {
        "policy_local"
    }
    
    fn description(&self) -> &'static str {
        "Check policy compliance using local rules (fallback)"
    }
    
    async fn invoke(&self, input: Value, _ctx: &ToolContext) -> Result<Value, ToolError> {
        let policy_input: PolicyInput = serde_json::from_value(input)?;
        
        let decision = self.check_local_policy(&policy_input);
        
        if !decision.allow {
            return Err(ToolError::Policy(
                decision.deny_reasons.join("; ")
            ));
        }
        
        Ok(serde_json::to_value(decision)?)
    }
}

impl Default for LocalPolicyTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_policy_tool_name() {
        let tool = PolicyTool::new(
            "http://localhost:8181".to_string(),
            "autodev/merge".to_string(),
        );
        assert_eq!(tool.name(), "policy");
    }

    #[test]
    fn test_local_policy_denies_high_risk() {
        let tool = LocalPolicyTool::new();
        let input = PolicyInput {
            task_id: Uuid::new_v4(),
            risk_tier: crate::autodev::schemas::RiskTier::High,
            diff: String::new(),
            files_changed: vec![],
            new_dependencies: vec![],
            clippy_warnings: 0,
            tests_passed: true,
            secrets_found: false,
        };
        
        let decision = tool.check_local_policy(&input);
        assert!(!decision.allow);
        assert!(decision.deny_reasons.iter().any(|r| r.contains("High-risk")));
    }

    #[test]
    fn test_local_policy_denies_secrets() {
        let tool = LocalPolicyTool::new();
        let input = PolicyInput {
            task_id: Uuid::new_v4(),
            risk_tier: crate::autodev::schemas::RiskTier::Low,
            diff: String::new(),
            files_changed: vec![],
            new_dependencies: vec![],
            clippy_warnings: 0,
            tests_passed: true,
            secrets_found: true,
        };
        
        let decision = tool.check_local_policy(&input);
        assert!(!decision.allow);
        assert!(decision.deny_reasons.iter().any(|r| r.contains("Secrets")));
    }

    #[test]
    fn test_local_policy_allows_clean_low_risk() {
        let tool = LocalPolicyTool::new();
        let input = PolicyInput {
            task_id: Uuid::new_v4(),
            risk_tier: crate::autodev::schemas::RiskTier::Low,
            diff: String::new(),
            files_changed: vec!["src/main.rs".to_string()],
            new_dependencies: vec![],
            clippy_warnings: 0,
            tests_passed: true,
            secrets_found: false,
        };
        
        let decision = tool.check_local_policy(&input);
        assert!(decision.allow);
    }
}