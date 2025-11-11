//! Git operations tool for cloning, branching, committing, and PR creation

use super::{Tool, ToolContext, ToolError};
use crate::autodev::schemas::{GitResult, CodegenResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info};

/// Git operations tool
pub struct GitTool {
    author_name: String,
    author_email: String,
}

impl GitTool {
    pub fn new(author_name: String, author_email: String) -> Self {
        Self {
            author_name,
            author_email,
        }
    }
    
    async fn run_git_command(
        &self,
        args: &[&str],
        workdir: &std::path::Path,
    ) -> Result<String, ToolError> {
        debug!("Running git command: git {}", args.join(" "));
        
        let output = Command::new("git")
            .args(args)
            .current_dir(workdir)
            .env("GIT_AUTHOR_NAME", &self.author_name)
            .env("GIT_AUTHOR_EMAIL", &self.author_email)
            .env("GIT_COMMITTER_NAME", &self.author_name)
            .env("GIT_COMMITTER_EMAIL", &self.author_email)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Git command failed: {}", stderr);
            return Err(ToolError::Git(stderr.to_string()));
        }
        
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[derive(Debug, Deserialize)]
struct GitApplyInput {
    branch: String,
    patch: String,
    #[serde(default)]
    commit_message: Option<String>,
}

#[async_trait]
impl Tool for GitTool {
    fn name(&self) -> &'static str {
        "git_apply"
    }
    
    fn description(&self) -> &'static str {
        "Create a new branch, apply a patch, and commit changes"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let input: GitApplyInput = serde_json::from_value(input)?;
        
        info!("Creating branch {} and applying patch", input.branch);
        
        // Create and checkout new branch
        self.run_git_command(&["checkout", "-b", &input.branch], &ctx.workdir).await?;
        
        // Apply patch
        let patch_file = ctx.workdir.join("temp.patch");
        tokio::fs::write(&patch_file, &input.patch).await?;
        
        // Try to apply patch with fallback strategies
        let apply_result = self.run_git_command(&["apply", "--reject", patch_file.to_str().unwrap()], &ctx.workdir).await;
        
        if apply_result.is_err() {
            // Try with -p1 if initial apply failed
            debug!("Trying patch apply with -p1");
            self.run_git_command(&["apply", "--reject", "-p1", patch_file.to_str().unwrap()], &ctx.workdir).await?;
        }
        
        // Stage all changes
        self.run_git_command(&["add", "-A"], &ctx.workdir).await?;
        
        // Check if there are changes to commit
        let status_output = self.run_git_command(&["status", "--porcelain"], &ctx.workdir).await?;
        
        if status_output.trim().is_empty() {
            info!("No changes to commit after applying patch");
            return Err(ToolError::Git("No changes to commit (patch may have already been applied)".to_string()));
        }
        
        // Commit
        let commit_msg = input.commit_message.unwrap_or_else(|| "AutoDev: Apply changes".to_string());
        self.run_git_command(&["commit", "-m", &commit_msg], &ctx.workdir).await
            .map_err(|e| ToolError::Git(format!("Commit failed: {}", e)))?;
        
        // Get commit hash
        let commit = self.run_git_command(&["rev-parse", "HEAD"], &ctx.workdir).await?;
        
        info!("Created commit {} on branch {}", commit, input.branch);
        
        let result = GitResult {
            branch: input.branch,
            commit,
            pr_url: None,
            pr_number: None,
        };
        
        Ok(serde_json::to_value(result)?)
    }
}

/// GitHub PR creation tool
pub struct GitHubPrTool {
    token: String,
}

impl GitHubPrTool {
    pub fn new(token: String) -> Self {
        Self { token }
    }
    
    async fn create_pr(
        &self,
        owner: &str,
        repo: &str,
        input: &PrInput,
    ) -> Result<GitResult, ToolError> {
        let client = reqwest::Client::new();
        
        #[derive(Serialize)]
        struct CreatePrRequest {
            title: String,
            body: String,
            head: String,
            base: String,
        }
        
        #[derive(Deserialize)]
        struct PrResponse {
            html_url: String,
            number: u64,
        }
        
        let url = format!("https://api.github.com/repos/{}/{}/pulls", owner, repo);
        
        let request = CreatePrRequest {
            title: input.title.clone(),
            body: input.body.clone(),
            head: input.branch.clone(),
            base: input.base.clone(),
        };
        
        let response = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("User-Agent", "AutoDev-Bot")
            .json(&request)
            .send()
            .await
            .map_err(|e| ToolError::Upstream(e.to_string()))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("GitHub API error {}: {}", status, text);
            return Err(ToolError::Upstream(format!("GitHub API error: {}", status)));
        }
        
        let pr: PrResponse = response.json().await
            .map_err(|e| ToolError::Upstream(e.to_string()))?;
        
        info!("Created PR #{}: {}", pr.number, pr.html_url);
        
        Ok(GitResult {
            branch: input.branch.clone(),
            commit: String::new(),
            pr_url: Some(pr.html_url),
            pr_number: Some(pr.number),
        })
    }
}

#[derive(Debug, Deserialize)]
struct PrInput {
    title: String,
    body: String,
    branch: String,
    base: String,
}

#[async_trait]
impl Tool for GitHubPrTool {
    fn name(&self) -> &'static str {
        "git_pr"
    }
    
    fn description(&self) -> &'static str {
        "Create a GitHub pull request"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let input: PrInput = serde_json::from_value(input)?;
        
        // Parse repo URL to extract owner/repo
        let repo_url = &ctx.repo_url;
        let parts: Vec<&str> = repo_url
            .trim_end_matches(".git")
            .split('/')
            .collect();
        
        if parts.len() < 2 {
            return Err(ToolError::Invalid(format!("Invalid repo URL: {}", repo_url)));
        }
        
        let owner = parts[parts.len() - 2];
        let repo = parts[parts.len() - 1];
        
        let result = self.create_pr(owner, repo, &input).await?;
        
        Ok(serde_json::to_value(result)?)
    }
}

/// Git push tool
pub struct GitPushTool {
    token_env: String,
}

impl GitPushTool {
    pub fn new(token_env: String) -> Self {
        Self { token_env }
    }
    
    async fn get_token(&amp;self) -> Option<String> {
        std::env::var(&amp;self.token_env).ok()
    }
    
    async fn run_git(
        &amp;self,
        args: &amp;[&amp;str],
        workdir: &amp;std::path::Path,
    ) -> Result<String, ToolError> {
        let output = Command::new("git")
            .args(args)
            .current_dir(workdir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&amp;output.stderr);
            return Err(ToolError::Git(stderr.to_string()));
        }
        
        Ok(String::from_utf8_lossy(&amp;output.stdout).trim().to_string())
    }
}

#[derive(Debug, Deserialize)]
struct PushInput {
    branch: String,
    #[serde(default = "default_remote")]
    remote: String,
}

fn default_remote() -> String {
    "origin".to_string()
}

#[async_trait]
impl Tool for GitPushTool {
    fn name(&amp;self) -> &amp;'static str {
        "git_push"
    }
    
    fn description(&amp;self) -> &amp;'static str {
        "Push branch to remote repository"
    }
    
    async fn invoke(&amp;self, input: Value, ctx: &amp;ToolContext) -> Result<Value, ToolError> {
        let input: PushInput = serde_json::from_value(input)?;
        
        info!("Pushing branch {} to {}", input.branch, input.remote);
        
        // Get token for authentication
        if let Some(token) = self.get_token().await {
            // Get origin URL
            let origin_url = self.run_git(&amp;["remote", "get-url", &amp;input.remote], &amp;ctx.workdir).await?;
            
            // If HTTPS GitHub URL, embed token for auth
            if origin_url.starts_with("https://github.com/") {
                let authed = origin_url.replacen(
                    "https://github.com/",
                    &amp;format!("https://x-access-token:{}@github.com/", token),
                    1,
                );
                
                // Set temporary remote with auth
                let _ = self.run_git(&amp;["remote", "remove", "autodev"], &amp;ctx.workdir).await;
                self.run_git(&amp;["remote", "add", "autodev", &amp;authed], &amp;ctx.workdir).await?;
                self.run_git(&amp;["push", "-u", "autodev", &amp;input.branch], &amp;ctx.workdir).await?;
                
                info!("Branch {} pushed successfully", input.branch);
            } else {
                // Try normal push
                self.run_git(&amp;["push", "-u", &amp;input.remote, &amp;input.branch], &amp;ctx.workdir).await?;
            }
        } else {
            // No token; attempt unauthenticated push
            warn!("No GitHub token found, attempting unauthenticated push");
            self.run_git(&amp;["push", "-u", &amp;input.remote, &amp;input.branch], &amp;ctx.workdir).await?;
        }
        
        Ok(serde_json::json!({
            "pushed": true,
            "branch": input.branch,
        }))
    }
}

/// Git clone tool
pub struct GitCloneTool;

#[derive(Debug, Deserialize)]
struct CloneInput {
    url: String,
    branch: String,
}

#[async_trait]
impl Tool for GitCloneTool {
    fn name(&self) -> &'static str {
        "git_clone"
    }
    
    fn description(&self) -> &'static str {
        "Clone a git repository"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let input: CloneInput = serde_json::from_value(input)?;
        
        info!("Cloning {} (branch: {})", input.url, input.branch);
        
        let output = Command::new("git")
            .args(&[
                "clone",
                "--depth", "1",
                "--branch", &input.branch,
                &input.url,
                ctx.workdir.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Git clone failed: {}", stderr);
            return Err(ToolError::Git(stderr.to_string()));
        }
        
        Ok(serde_json::json!({
            "success": true,
            "path": ctx.workdir.to_str().unwrap()
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_tool_name() {
        let tool = GitTool::new("Bot".to_string(), "bot@example.com".to_string());
        assert_eq!(tool.name(), "git_apply");
    }

    #[test]
    fn test_pr_tool_name() {
        let tool = GitHubPrTool::new("token".to_string());
        assert_eq!(tool.name(), "git_pr");
    }
}