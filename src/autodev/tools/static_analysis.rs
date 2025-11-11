//! Static analysis tools (clippy, secrets scanning, etc.)

use super::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info};

/// Clippy static analysis tool
pub struct ClippyTool {
    image: String,
}

impl ClippyTool {
    pub fn new(image: String) -> Self {
        Self { image }
    }
    
    async fn run_clippy(&self, workdir: &std::path::Path) -> Result<ClippyResult, ToolError> {
        info!("Running clippy analysis");
        
        let output = Command::new("docker")
            .args(&[
                "run",
                "--rm",
                "-v", &format!("{}:/workspace", workdir.display()),
                "-w", "/workspace",
                &self.image,
                "cargo", "clippy", "--", "-D", "warnings",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        
        // Parse warnings count from output
        let warnings = stderr.lines()
            .filter(|l| l.contains("warning:"))
            .count() as u32;
        
        info!("Clippy found {} warnings", warnings);
        
        Ok(ClippyResult {
            warnings,
            passed: exit_code == 0,
            output: stderr,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ClippyResult {
    warnings: u32,
    passed: bool,
    output: String,
}

#[async_trait]
impl Tool for ClippyTool {
    fn name(&self) -> &'static str {
        "clippy"
    }
    
    fn description(&self) -> &'static str {
        "Run Rust clippy static analysis"
    }
    
    async fn invoke(&self, _input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let result = self.run_clippy(&ctx.workdir).await?;
        Ok(serde_json::to_value(result)?)
    }
}

/// Secrets scanning tool using gitleaks
pub struct SecretsScanner;

impl SecretsScanner {
    pub fn new() -> Self {
        Self
    }
    
    async fn scan_secrets(&self, workdir: &std::path::Path) -> Result<SecretsResult, ToolError> {
        info!("Scanning for secrets");
        
        // Check if gitleaks is available
        let gitleaks_check = Command::new("which")
            .arg("gitleaks")
            .output()
            .await?;
        
        if !gitleaks_check.status.success() {
            debug!("gitleaks not found, using simple pattern matching");
            return self.simple_secrets_scan(workdir).await;
        }
        
        let output = Command::new("gitleaks")
            .args(&[
                "detect",
                "--source", workdir.to_str().unwrap(),
                "--no-git",
                "--report-format", "json",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        // Exit code 1 means secrets found
        let secrets_found = output.status.code() == Some(1);
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        
        info!("Secrets scan: {}", if secrets_found { "FOUND" } else { "CLEAN" });
        
        Ok(SecretsResult {
            secrets_found,
            details: if secrets_found { Some(stdout.to_string()) } else { None },
        })
    }
    
    async fn simple_secrets_scan(&self, workdir: &std::path::Path) -> Result<SecretsResult, ToolError> {
        // Simple pattern matching for common secrets
        let patterns = vec![
            r"(?i)(api[_-]?key|apikey)\s*[:=]\s*['&quot;]?[a-zA-Z0-9]{20,}",
            r"(?i)(secret[_-]?key|secretkey)\s*[:=]\s*['&quot;]?[a-zA-Z0-9]{20,}",
            r"(?i)(password|passwd|pwd)\s*[:=]\s*['&quot;]?[a-zA-Z0-9]{8,}",
            r"(?i)(token)\s*[:=]\s*['&quot;]?[a-zA-Z0-9]{20,}",
        ];
        
        for pattern in patterns {
            let output = Command::new("rg")
                .args(&[
                    "-i",
                    "--no-filename",
                    "--no-line-number",
                    pattern,
                ])
                .current_dir(workdir)
                .output()
                .await?;
            
            if output.status.success() && !output.stdout.is_empty() {
                return Ok(SecretsResult {
                    secrets_found: true,
                    details: Some("Potential secrets detected".to_string()),
                });
            }
        }
        
        Ok(SecretsResult {
            secrets_found: false,
            details: None,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SecretsResult {
    secrets_found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

#[async_trait]
impl Tool for SecretsScanner {
    fn name(&self) -> &'static str {
        "secrets_scan"
    }
    
    fn description(&self) -> &'static str {
        "Scan for secrets and credentials in code"
    }
    
    async fn invoke(&self, _input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let result = self.scan_secrets(&ctx.workdir).await?;
        Ok(serde_json::to_value(result)?)
    }
}

impl Default for SecretsScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Dependency checker tool
pub struct DependencyChecker;

impl DependencyChecker {
    pub fn new() -> Self {
        Self
    }
    
    async fn check_dependencies(&self, workdir: &std::path::Path) -> Result<DepsResult, ToolError> {
        info!("Checking dependencies");
        
        // Read Cargo.toml
        let cargo_toml = workdir.join("Cargo.toml");
        if !cargo_toml.exists() {
            return Ok(DepsResult {
                new_dependencies: vec![],
                total_dependencies: 0,
            });
        }
        
        let content = tokio::fs::read_to_string(&cargo_toml).await?;
        
        // Simple parsing - count dependencies
        let deps_count = content.lines()
            .filter(|l| l.contains("=") && !l.trim().starts_with('#'))
            .count();
        
        Ok(DepsResult {
            new_dependencies: vec![], // Would need git diff to detect new ones
            total_dependencies: deps_count,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DepsResult {
    new_dependencies: Vec<String>,
    total_dependencies: usize,
}

#[async_trait]
impl Tool for DependencyChecker {
    fn name(&self) -> &'static str {
        "check_deps"
    }
    
    fn description(&self) -> &'static str {
        "Check project dependencies"
    }
    
    async fn invoke(&self, _input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let result = self.check_dependencies(&ctx.workdir).await?;
        Ok(serde_json::to_value(result)?)
    }
}

impl Default for DependencyChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clippy_tool_name() {
        let tool = ClippyTool::new("rust:1.82".to_string());
        assert_eq!(tool.name(), "clippy");
    }

    #[test]
    fn test_secrets_scanner_name() {
        let tool = SecretsScanner::new();
        assert_eq!(tool.name(), "secrets_scan");
    }

    #[test]
    fn test_dependency_checker_name() {
        let tool = DependencyChecker::new();
        assert_eq!(tool.name(), "check_deps");
    }
}