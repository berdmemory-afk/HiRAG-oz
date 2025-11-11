//! Data models for autonomous software development tasks and plans

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Risk tier for task execution and merge policy
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskTier {
    Low,    // Auto-merge allowed
    Medium, // PR + human review
    High,   // PR + approvers + policy override
}

impl Default for RiskTier {
    fn default() -> Self {
        Self::Low
    }
}

/// Task status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Planning,
    Executing,
    Verifying,
    PrCreated,
    Merged,
    Failed,
    Cancelled,
}

/// Autonomous development task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub repo: String,
    pub base_branch: String,
    #[serde(default)]
    pub risk_tier: RiskTier,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub metrics: TaskMetrics,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Task metrics and SLAs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskMetrics {
    #[serde(default = "default_sla_minutes")]
    pub sla_minutes: u32,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
}

fn default_sla_minutes() -> u32 {
    60
}

fn default_max_iterations() -> u32 {
    8
}

impl Default for TaskMetrics {
    fn default() -> Self {
        Self {
            sla_minutes: default_sla_minutes(),
            max_iterations: default_max_iterations(),
        }
    }
}

/// Execution plan with steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub task_id: Uuid,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub created_at: i64,
}

/// Individual step in a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub tool: String,
    pub input: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub status: StepStatus,
}

/// Step execution status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

impl Default for StepStatus {
    fn default() -> Self {
        Self::Pending
    }
}

/// Policy decision input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyInput {
    pub task_id: Uuid,
    pub risk_tier: RiskTier,
    pub diff: String,
    pub files_changed: Vec<String>,
    pub new_dependencies: Vec<String>,
    pub clippy_warnings: u32,
    pub tests_passed: bool,
    pub secrets_found: bool,
}

/// Policy decision output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub allow: bool,
    #[serde(default)]
    pub deny_reasons: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Git operation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitResult {
    pub branch: String,
    pub commit: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,
}

/// Runner execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts_path: Option<String>,
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub file: String,
    pub line: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Codegen result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegenResult {
    pub patch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
}

/// Task creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: String,
    pub repo: String,
    #[serde(default = "default_base_branch")]
    pub base_branch: String,
    #[serde(default)]
    pub risk_tier: RiskTier,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub metrics: TaskMetrics,
}

fn default_base_branch() -> String {
    "main".to_string()
}

/// Task list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListResponse {
    pub tasks: Vec<Task>,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_serialization() {
        let task = Task {
            id: Uuid::new_v4(),
            title: "Fix flaky test".to_string(),
            description: "Test times out intermittently".to_string(),
            repo: "https://github.com/org/repo.git".to_string(),
            base_branch: "main".to_string(),
            risk_tier: RiskTier::Low,
            constraints: vec!["No API changes".to_string()],
            acceptance: vec!["Tests pass".to_string()],
            metrics: TaskMetrics::default(),
            status: TaskStatus::Pending,
            pr_url: None,
            error: None,
        };

        let json = serde_json::to_string(&task).unwrap();
        let deserialized: Task = serde_json::from_str(&json).unwrap();
        assert_eq!(task.id, deserialized.id);
        assert_eq!(task.title, deserialized.title);
    }

    #[test]
    fn test_risk_tier_default() {
        let tier = RiskTier::default();
        assert_eq!(tier, RiskTier::Low);
    }

    #[test]
    fn test_policy_decision() {
        let decision = PolicyDecision {
            allow: false,
            deny_reasons: vec!["High-risk change".to_string()],
            warnings: vec![],
        };
        assert!(!decision.allow);
        assert_eq!(decision.deny_reasons.len(), 1);
    }
}