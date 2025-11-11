//! Code generation tool using LLM

use super::{Tool, ToolContext, ToolError};
use crate::autodev::schemas::CodegenResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, error, info};

/// LLM-based code generation tool
pub struct CodegenTool {
    api_key: String,
    model: String,
    api_url: String,
    max_tokens: u32,
    temperature: f32,
}

impl CodegenTool {
    pub fn new(
        api_key: String,
        model: String,
        api_url: String,
        max_tokens: u32,
        temperature: f32,
    ) -> Self {
        Self {
            api_key,
            model,
            api_url,
            max_tokens,
            temperature,
        }
    }
    
    fn build_system_prompt(&self) -> String {
        r#"You are an autonomous software engineer operating in a strict budget and policy environment.

Your job: produce minimal, correct patches that pass tests, clippy, and policy checks.

Constraints:
- Do not introduce new dependencies unless asked and policy allows.
- Do not change public API without explicit instruction.
- Fix root causes; prefer small focused diffs; include tests when possible.
- Provide a concise commit message and rationale.

Output format: Unified diff patch. Include a brief rationale and commit message in JSON format:
{
  "patch": "--- a/file.rs\n+++ b/file.rs\n...",
  "rationale": "Brief explanation of the fix",
  "commit_message": "Short commit message"
}

No extra text outside the JSON."#.to_string()
    }
    
    async fn generate_code(
        &self,
        instruction: &str,
        context: &str,
    ) -> Result<CodegenResult, ToolError> {
        let client = reqwest::Client::new();
        
        #[derive(Serialize)]
        struct ChatMessage {
            role: String,
            content: String,
        }
        
        #[derive(Serialize)]
        struct ChatRequest {
            model: String,
            messages: Vec<ChatMessage>,
            max_tokens: u32,
            temperature: f32,
        }
        
        #[derive(Deserialize)]
        struct ChatResponse {
            choices: Vec<Choice>,
        }
        
        #[derive(Deserialize)]
        struct Choice {
            message: Message,
        }
        
        #[derive(Deserialize)]
        struct Message {
            content: String,
        }
        
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: self.build_system_prompt(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: format!("Context:\n{}\n\nInstruction:\n{}", context, instruction),
            },
        ];
        
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        };
        
        debug!("Sending codegen request to LLM");
        
        let response = client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| ToolError::Upstream(e.to_string()))?;
        
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            error!("LLM API error {}: {}", status, text);
            return Err(ToolError::Upstream(format!("LLM API error: {}", status)));
        }
        
        let chat_response: ChatResponse = response.json().await
            .map_err(|e| ToolError::Upstream(e.to_string()))?;
        
        if chat_response.choices.is_empty() {
            return Err(ToolError::Upstream("No response from LLM".to_string()));
        }
        
        let content = &chat_response.choices[0].message.content;
        
        // Try to parse as JSON
        let result: CodegenResult = serde_json::from_str(content)
            .map_err(|e| {
                error!("Failed to parse LLM response as JSON: {}", e);
                error!("Response: {}", content);
                ToolError::Upstream(format!("Invalid LLM response format: {}", e))
            })?;
        
        info!("Generated code patch ({} bytes)", result.patch.len());
        
        Ok(result)
    }
}

#[derive(Debug, Deserialize)]
struct CodegenInput {
    instruction: String,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    context: Option<String>,
}

#[async_trait]
impl Tool for CodegenTool {
    fn name(&self) -> &'static str {
        "codegen"
    }
    
    fn description(&self) -> &'static str {
        "Generate code changes using LLM based on instruction and context"
    }
    
    async fn invoke(&self, input: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let input: CodegenInput = serde_json::from_value(input)?;
        
        // Build context from files if provided
        let mut context = input.context.unwrap_or_default();
        
        if !input.files.is_empty() {
            context.push_str("\n\nRelevant files:\n");
            for file in &input.files {
                let file_path = ctx.workdir.join(file);
                if file_path.exists() {
                    match tokio::fs::read_to_string(&file_path).await {
                        Ok(content) => {
                            context.push_str(&format!("\n--- {} ---\n{}\n", file, content));
                        }
                        Err(e) => {
                            error!("Failed to read file {}: {}", file, e);
                        }
                    }
                }
            }
        }
        
        let result = self.generate_code(&input.instruction, &context).await?;
        
        Ok(serde_json::to_value(result)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codegen_tool_name() {
        let tool = CodegenTool::new(
            "key".to_string(),
            "gpt-4".to_string(),
            "https://api.openai.com/v1/chat/completions".to_string(),
            4096,
            0.2,
        );
        assert_eq!(tool.name(), "codegen");
    }

    #[test]
    fn test_system_prompt_contains_constraints() {
        let tool = CodegenTool::new(
            "key".to_string(),
            "gpt-4".to_string(),
            "url".to_string(),
            4096,
            0.2,
        );
        let prompt = tool.build_system_prompt();
        assert!(prompt.contains("Constraints"));
        assert!(prompt.contains("unified diff"));
    }
}