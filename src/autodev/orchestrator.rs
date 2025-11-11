//! Orchestrator for autonomous software development tasks

use crate::autodev::schemas::{Task, Plan, Step, StepStatus, TaskStatus, PolicyInput, RiskTier};
use crate::autodev::tools::{Tool, ToolContext, ToolError};
use crate::autodev::metrics::AUTODEV_METRICS;
use crate::autodev::config::AutodevConfig;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Main orchestrator for autonomous development tasks
pub struct Orchestrator {
    tools: HashMap<String, Arc<dyn Tool>>,
    config: AutodevConfig,
}

impl Orchestrator {
    pub fn new(tools: Vec<Arc<dyn Tool>>, config: AutodevConfig) -> Self {
        let tool_map = tools
            .into_iter()
            .map(|t| (t.name().to_string(), t))
            .collect();
        
        Self {
            tools: tool_map,
            config,
        }
    }
    
    /// Get reference to configuration
    pub fn config(&self) -> &AutodevConfig {
        &self.config
    }
    
    /// Run a complete task from start to finish
    pub async fn run_task(&self, mut task: Task) -> Result<Task> {
        info!("Starting task {}: {}", task.id, task.title);
        AUTODEV_METRICS.tasks_total.inc();
        
        let start = std::time::Instant::now();
        
        // Update status
        task.status = TaskStatus::Planning;
        
        // Create workspace
        let base = self.create_workspace(&task).await?;
        
        // Clone repository
        let workdir = self.clone_repository(&task, &base).await?;
        
        // Generate plan
        task.status = TaskStatus::Planning;
        let plan = self.plan(&task, &workdir).await?;
        
        // Execute plan
        task.status = TaskStatus::Executing;
        match self.execute_plan(&task, &plan, &workdir).await {
            Ok(_) => {
                task.status = TaskStatus::PrCreated;
                AUTODEV_METRICS.tasks_success.inc();
                info!("Task {} completed successfully in {:?}", task.id, start.elapsed());
            }
            Err(e) => {
                task.status = TaskStatus::Failed;
                task.error = Some(e.to_string());
                AUTODEV_METRICS.tasks_failed.inc();
                error!("Task {} failed: {}", task.id, e);
            }
        }
        
        // Cleanup workspace
        if let Err(e) = fs::remove_dir_all(&base).await {
            warn!("Failed to cleanup workspace: {}", e);
        }
        
        AUTODEV_METRICS.task_duration
            .observe(start.elapsed().as_secs_f64());
        
        Ok(task)
    }
    
    /// Create a workspace directory for the task
    async fn create_workspace(&self, task: &Task) -> Result<PathBuf> {
        let base = std::env::temp_dir()
            .join("autodev")
            .join(task.id.to_string());
        
        fs::create_dir_all(&base).await
            .context("Failed to create workspace base")?;
        
        debug!("Created workspace base at {}", base.display());
        
        Ok(base)
    }
    
    /// Clone the repository
    async fn clone_repository(&self, task: &Task, base: &PathBuf) -> Result<PathBuf> {
        info!("Cloning repository {}", task.repo);
        
        let clone_tool = self.tools.get("git_clone")
            .context("git_clone tool not found")?;
        
        let repo_dir = base.join("repo");
        
        let ctx = ToolContext {
            workdir: repo_dir.clone(),
            repo_url: task.repo.clone(),
            base_branch: task.base_branch.clone(),
            env: std::env::vars().collect(),
            timeout: std::time::Duration::from_secs(300),
            task_id: task.id,
        };
        
        let input = serde_json::json!({
            "url": task.repo,
            "branch": task.base_branch,
        });
        
        clone_tool.invoke(input, &ctx).await
            .context("Failed to clone repository")?;
        
        Ok(repo_dir)
    }
    
    /// Generate execution plan for the task
    async fn plan(&self, task: &Task, workdir: &PathBuf) -> Result<Plan> {
        info!("Generating plan for task {}", task.id);
        
        // For now, use a simple heuristic plan
        // In production, this would use LLM to generate a custom plan
        let steps = self.generate_heuristic_plan(task);
        
        Ok(Plan {
            task_id: task.id,
            steps,
            created_at: chrono::Utc::now().timestamp(),
        })
    }
    
    /// Generate a heuristic plan based on task type
    fn generate_heuristic_plan(&self, task: &Task) -> Vec<Step> {
        let mut steps = Vec::new();
        
        // Step 1: Search for relevant code
        steps.push(Step {
            name: "Search repository".to_string(),
            tool: "repo_search".to_string(),
            input: serde_json::json!({
                "pattern": self.extract_search_pattern(&task.description),
                "max_results": 50,
            }),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 2: Generate code changes
        steps.push(Step {
            name: "Generate code changes".to_string(),
            tool: "codegen".to_string(),
            input: serde_json::json!({
                "instruction": task.description,
                "context": format!("Constraints: {}", task.constraints.join(", ")),
            }),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 3: Apply changes
        steps.push(Step {
            name: "Apply changes".to_string(),
            tool: "git_apply".to_string(),
            input: serde_json::json!({
                "branch": format!("autodev/{}", task.id),
                "commit_message": format!("AutoDev: {}", task.title),
            }),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 4: Build
        steps.push(Step {
            name: "Build project".to_string(),
            tool: "build".to_string(),
            input: serde_json::json!({}),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 5: Run tests
        steps.push(Step {
            name: "Run tests".to_string(),
            tool: "test".to_string(),
            input: serde_json::json!({}),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 6: Static analysis
        steps.push(Step {
            name: "Run clippy".to_string(),
            tool: "clippy".to_string(),
            input: serde_json::json!({}),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 7: Secrets scan
        steps.push(Step {
            name: "Scan for secrets".to_string(),
            tool: "secrets_scan".to_string(),
            input: serde_json::json!({}),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 8: Policy check
        let policy_tool = if self.config.opa_url.is_some() {
            "policy"
        } else {
            "policy_local"
        };
        
        steps.push(Step {
            name: "Check policy".to_string(),
            tool: policy_tool.to_string(),
            input: serde_json::json!({
                "task_id": task.id,
                "risk_tier": task.risk_tier,
            }),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        // Step 9: Create PR
        steps.push(Step {
            name: "Create pull request".to_string(),
            tool: "git_pr".to_string(),
            input: serde_json::json!({
                "title": task.title,
                "body": format!("{}\n\nGenerated by AutoDev", task.description),
                "branch": format!("autodev/{}", task.id),
                "base": task.base_branch,
            }),
            output: None,
            error: None,
            status: StepStatus::Pending,
        });
        
        steps
    }
    
    /// Extract search pattern from task description
    fn extract_search_pattern(&self, description: &str) -> String {
        // Simple heuristic: extract first quoted term or first word
        if let Some(start) = description.find('"') {
            if let Some(end) = description[start + 1..].find('"') {
                return description[start + 1..start + 1 + end].to_string();
            }
        }
        
        description
            .split_whitespace()
            .next()
            .unwrap_or("TODO")
            .to_string()
    }
    
    /// Execute the plan
    async fn execute_plan(&self, task: &Task, plan: &Plan, workdir: &PathBuf) -> Result<()> {
        info!("Executing plan with {} steps", plan.steps.len());
        
        let ctx = ToolContext {
            workdir: workdir.clone(),
            repo_url: task.repo.clone(),
            base_branch: task.base_branch.clone(),
            env: std::env::vars().collect(),
            timeout: std::time::Duration::from_secs(self.config.runner_timeout_secs as u64),
            task_id: task.id,
        };
        
        let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();
        
        for (i, step) in plan.steps.iter().enumerate() {
            info!("Executing step {}/{}: {}", i + 1, plan.steps.len(), step.name);
            
            let start = std::time::Instant::now();
            
            match self.execute_step(step, &ctx, &step_outputs).await {
                Ok(output) => {
                    AUTODEV_METRICS.steps_total
                        .with_label_values(&["success"])
                        .inc();
                    AUTODEV_METRICS.step_duration
                        .with_label_values(&[&step.tool])
                        .observe(start.elapsed().as_secs_f64());
                    
                    step_outputs.insert(step.name.clone(), output);
                    info!("Step {} completed successfully", step.name);
                }
                Err(e) => {
                    AUTODEV_METRICS.steps_total
                        .with_label_values(&["error"])
                        .inc();
                    
                    error!("Step {} failed: {}", step.name, e);
                    
                    // Retry logic
                    if i < self.config.max_step_retries as usize {
                        warn!("Retrying step {} (attempt {})", step.name, i + 1);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                    
                    return Err(e.into());
                }
            }
        }
        
        Ok(())
    }
    
    /// Execute a single step
    async fn execute_step(
        &self,
        step: &Step,
        ctx: &ToolContext,
        outputs: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, ToolError> {
        let tool = self.tools.get(&step.tool)
            .ok_or_else(|| ToolError::Invalid(format!("Unknown tool: {}", step.tool)))?;
        
        // Merge step input with previous outputs if needed
        let mut input = step.input.clone();
        
        // Special handling for git_apply - inject patch from codegen output
        if step.tool == "git_apply" {
            if let Some(codegen_output) = outputs.get("Generate code changes") {
                if let Some(patch) = codegen_output.get("patch") {
                    input["patch"] = patch.clone();
                }
            }
        }
        
        // Special handling for policy - build complete input
        if step.tool == "policy_local" || step.tool == "policy" {
            input = self.build_policy_input(ctx, outputs).await?;
        }
        
        // Special handling for git_pr - track PR metrics
        if step.tool == "git_pr" {
            let result = tool.invoke(input, ctx).await?;
            if let Some(pr_url) = result.get("pr_url").and_then(|v| v.as_str()) {
                AUTODEV_METRICS.prs_opened.inc();
                info!("PR created: {}", pr_url);
            }
            return Ok(result);
        }
        
        tool.invoke(input, ctx).await
    }
    
    /// Build policy input from collected data
    async fn build_policy_input(
        &self,
        ctx: &ToolContext,
        outputs: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, ToolError> {
        let clippy_warnings = outputs
            .get("Run clippy")
            .and_then(|v| v.get("warnings"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
        
        let tests_passed = outputs
            .get("Run tests")
            .and_then(|v| v.get("exit_code"))
            .and_then(|v| v.as_i64())
            .map(|c| c == 0)
            .unwrap_or(false);
        
        let secrets_found = outputs
            .get("Scan for secrets")
            .and_then(|v| v.get("secrets_found"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let patch = outputs
            .get("Generate code changes")
            .and_then(|v| v.get("patch"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        
        // Get files changed from git diff
        let files_changed = self.get_files_changed(&ctx.workdir).await
            .unwrap_or_default();
        
        // Get new dependencies
        let new_dependencies = outputs
            .get("Check dependencies")
            .and_then(|v| v.get("new_dependencies"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        
        Ok(serde_json::json!({
            "task_id": ctx.task_id,
            "risk_tier": "low",
            "diff": patch,
            "files_changed": files_changed,
            "new_dependencies": new_dependencies,
            "clippy_warnings": clippy_warnings,
            "tests_passed": tests_passed,
            "secrets_found": secrets_found,
        }))
    }
    
    /// Get list of changed files from git
    async fn get_files_changed(&self, workdir: &std::path::Path) -> Result<Vec<String>, ToolError> {
        use tokio::process::Command;
        
        let output = Command::new("git")
            .args(&["diff", "--name-only", "HEAD"])
            .current_dir(workdir)
            .output()
            .await
            .map_err(|e| ToolError::Git(e.to_string()))?;
        
        if !output.status.success() {
            return Ok(vec![]);
        }
        
        let files = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.to_string())
            .collect();
        
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_search_pattern() {
        let config = AutodevConfig::default();
        let orchestrator = Orchestrator::new(vec![], config);
        
        let pattern = orchestrator.extract_search_pattern("Fix the &quot;timeout&quot; issue in decode");
        assert_eq!(pattern, "timeout");
        
        let pattern = orchestrator.extract_search_pattern("Refactor the code");
        assert_eq!(pattern, "Refactor");
    }
}