//! Repository search and indexing tools

use super::{Tool, ToolContext, ToolError};
use crate::autodev::schemas::SearchMatch;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{debug, error, info};

/// Repository search tool using ripgrep
pub struct RepoSearchTool {
    max_results: usize,
}

impl RepoSearchTool {
    pub fn new(max_results: usize) -> Self {
        Self { max_results }
    }
    
    async fn search_repo(
        &self,
        pattern: &str,
        workdir: &std::path::Path,
    ) -> Result<Vec<SearchMatch>, ToolError> {
        debug!("Searching for pattern: {}", pattern);
        
        let output = Command::new("rg")
            .args(&[
                "--json",
                "--max-count", &self.max_results.to_string(),
                "--",
                pattern,
            ])
            .current_dir(workdir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        if !output.status.success() && output.status.code() != Some(1) {
            // Exit code 1 means no matches, which is ok
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("ripgrep failed: {}", stderr);
            return Err(ToolError::Exec(stderr.to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut matches = Vec::new();
        
        #[derive(Deserialize)]
        struct RgMatch {
            #[serde(rename = "type")]
            msg_type: String,
            data: Option<RgData>,
        }
        
        #[derive(Deserialize)]
        struct RgData {
            path: Option<RgPath>,
            lines: Option<RgLines>,
            line_number: Option<u32>,
        }
        
        #[derive(Deserialize)]
        struct RgPath {
            text: String,
        }
        
        #[derive(Deserialize)]
        struct RgLines {
            text: String,
        }
        
        for line in stdout.lines() {
            if let Ok(rg_match) = serde_json::from_str::<RgMatch>(line) {
                if rg_match.msg_type == "match" {
                    if let Some(data) = rg_match.data {
                        if let (Some(path), Some(lines), Some(line_num)) = 
                            (data.path, data.lines, data.line_number) {
                            matches.push(SearchMatch {
                                file: path.text,
                                line: line_num,
                                text: lines.text,
                                context: None,
                            });
                        }
                    }
                }
            }
        }
        
        info!("Found {} matches", matches.len());
        
        Ok(matches)
    }
}

#[derive(Debug, Deserialize)]
struct SearchInput {
    pattern: String,
    #[serde(default)]
    max_results: Option<usize>,
}

#[async_trait]
impl Tool for RepoSearchTool {
    fn name(&self) -> &'static str {
        "repo_search"
    }
    
    fn description(&self) -> &'static str {
        "Search repository for text patterns using ripgrep"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let input: SearchInput = serde_json::from_value(input)?;
        
        let max_results = input.max_results.unwrap_or(self.max_results);
        let mut tool = Self::new(max_results);
        
        let matches = tool.search_repo(&input.pattern, &ctx.workdir).await?;
        
        Ok(serde_json::json!({
            "matches": matches,
            "total": matches.len()
        }))
    }
}

/// File list tool
pub struct FileListTool;

impl FileListTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &'static str {
        "file_list"
    }
    
    fn description(&self) -> &'static str {
        "List files in the repository"
    }
    
    async fn invoke(&self, _input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let output = Command::new("find")
            .args(&[
                ".",
                "-type", "f",
                "-not", "-path", "*/.*",
                "-not", "-path", "*/target/*",
                "-not", "-path", "*/node_modules/*",
            ])
            .current_dir(&ctx.workdir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ToolError::Exec(stderr.to_string()));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout
            .lines()
            .map(|l| l.trim_start_matches("./").to_string())
            .collect();
        
        Ok(serde_json::json!({
            "files": files,
            "total": files.len()
        }))
    }
}

impl Default for FileListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_tool_name() {
        let tool = RepoSearchTool::new(100);
        assert_eq!(tool.name(), "repo_search");
    }

    #[test]
    fn test_file_list_tool_name() {
        let tool = FileListTool::new();
        assert_eq!(tool.name(), "file_list");
    }
}