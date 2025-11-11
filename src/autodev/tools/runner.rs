//! Sandbox runner tool for executing build and test commands in Docker

use super::{Tool, ToolContext, ToolError};
use crate::autodev::schemas::RunnerResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info};

/// Docker-based sandbox runner
pub struct RunnerTool {
    image: String,
    timeout_secs: u64,
}

impl RunnerTool {
    pub fn new(image: String, timeout_secs: u64) -> Self {
        Self {
            image,
            timeout_secs,
        }
    }
    
    /// Check if Docker is available
    async fn check_docker() -> Result<(), ToolError> {
        let output = Command::new("which")
            .arg("docker")
            .output()
            .await
            .map_err(|e| ToolError::Exec(format!("Failed to check for docker: {}", e)))?;
        
        if !output.status.success() {
            return Err(ToolError::Exec(
                "Docker is not installed or not in PATH. Please install Docker to use the runner tool.".to_string()
            ));
        }
        
        Ok(())
    }
    
    async fn run_in_docker(
        &self,
        cmd: &[String],
        workdir: &std::path::Path,
        timeout: std::time::Duration,
    ) -> Result<RunnerResult, ToolError> {
        info!("Running command in Docker: {:?}", cmd);
        
        // Build docker run command
        let mut docker_args = vec![
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            format!("{}:/workspace", workdir.display()),
            "-w".to_string(),
            "/workspace".to_string(),
            "--network".to_string(),
            "none".to_string(), // Isolated network
            self.image.clone(),
        ];
        docker_args.extend_from_slice(cmd);
        
        debug!("Docker command: docker {}", docker_args.join(" "));
        
        // Execute with timeout
        let child = Command::new("docker")
            .args(&docker_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| ToolError::Timeout(timeout))?
            .map_err(|e| ToolError::Io(e))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        if exit_code != 0 {
            error!("Command failed with exit code {}", exit_code);
            debug!("stdout: {}", stdout);
            debug!("stderr: {}", stderr);
        } else {
            info!("Command succeeded");
        }
        
        Ok(RunnerResult {
            exit_code,
            stdout,
            stderr,
            artifacts_path: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RunnerInput {
    cmd: Vec<String>,
    #[serde(default)]
    timeout_override: Option<u64>,
}

#[async_trait]
impl Tool for RunnerTool {
    fn name(&self) -> &'static str {
        "runner"
    }
    
    fn description(&self) -> &'static str {
        "Execute build and test commands in a sandboxed Docker container"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let input: RunnerInput = serde_json::from_value(input)?;
        
        if input.cmd.is_empty() {
            return Err(ToolError::Invalid("Command cannot be empty".to_string()));
        }
        
        // Check Docker availability
        Self::check_docker().await?;
        
        let timeout_secs = input.timeout_override.unwrap_or(self.timeout_secs);
        let timeout = std::time::Duration::from_secs(timeout_secs);
        
        let result = self.run_in_docker(&input.cmd, &ctx.workdir, timeout).await?;
        
        Ok(serde_json::to_value(result)?)
    }
}

/// Build tool (convenience wrapper around runner)
pub struct BuildTool {
    runner: RunnerTool,
}

impl BuildTool {
    pub fn new(image: String, timeout_secs: u64) -> Self {
        Self {
            runner: RunnerTool::new(image, timeout_secs),
        }
    }
}

#[async_trait]
impl Tool for BuildTool {
    fn name(&self) -> &'static str {
        "build"
    }
    
    fn description(&self) -> &'static str {
        "Build the project (cargo build)"
    }
    
    async fn invoke(&self, _input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let cmd = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cargo build --release".to_string(),
        ];
        
        let input = serde_json::json!({ "cmd": cmd });
        let result = self.runner.invoke(input, ctx).await?;
        
        let runner_result: RunnerResult = serde_json::from_value(result)?;
        
        if runner_result.exit_code != 0 {
            return Err(ToolError::Build(format!(
                "Build failed with exit code {}: {}",
                runner_result.exit_code,
                runner_result.stderr
            )));
        }
        
        Ok(serde_json::to_value(runner_result)?)
    }
}

/// Test tool (convenience wrapper around runner)
pub struct TestTool {
    runner: RunnerTool,
}

impl TestTool {
    pub fn new(image: String, timeout_secs: u64) -> Self {
        Self {
            runner: RunnerTool::new(image, timeout_secs),
        }
    }
}

#[async_trait]
impl Tool for TestTool {
    fn name(&self) -> &'static str {
        "test"
    }
    
    fn description(&self) -> &'static str {
        "Run tests (cargo test)"
    }
    
    async fn invoke(&self, _input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let cmd = vec![
            "bash".to_string(),
            "-lc".to_string(),
            "cargo test --all --quiet".to_string(),
        ];
        
        let input = serde_json::json!({ "cmd": cmd });
        let result = self.runner.invoke(input, ctx).await?;
        
        let runner_result: RunnerResult = serde_json::from_value(result)?;
        
        if runner_result.exit_code != 0 {
            return Err(ToolError::TestFailed(format!(
                "Tests failed with exit code {}: {}",
                runner_result.exit_code,
                runner_result.stderr
            )));
        }
        
        Ok(serde_json::to_value(runner_result)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_tool_name() {
        let tool = RunnerTool::new("rust:1.82".to_string(), 600);
        assert_eq!(tool.name(), "runner");
    }

    #[test]
    fn test_build_tool_name() {
        let tool = BuildTool::new("rust:1.82".to_string(), 600);
        assert_eq!(tool.name(), "build");
    }

    #[test]
    fn test_test_tool_name() {
        let tool = TestTool::new("rust:1.82".to_string(), 600);
        assert_eq!(tool.name(), "test");
    }
}