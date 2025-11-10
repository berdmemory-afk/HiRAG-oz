//! LLM-based summarization for running brief compression

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

/// Summarizer trait for different summarization strategies
#[async_trait]
pub trait Summarizer: Send + Sync {
    /// Summarize a list of text segments into a concise brief
    async fn summarize(&self, texts: &[String], max_tokens: usize) -> Result<String, SummarizerError>;
}

/// Configuration for LLM summarizer
#[derive(Debug, Clone)]
pub struct SummarizerConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout: Duration,
    pub max_retries: usize,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            endpoint: "http://localhost:8080/v1/chat/completions".to_string(),
            api_key: None,
            model: "gpt-3.5-turbo".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
}

/// LLM-based summarizer using OpenAI-compatible API
pub struct LLMSummarizer {
    client: Client,
    config: SummarizerConfig,
}

impl LLMSummarizer {
    /// Create a new LLM summarizer
    pub fn new(config: SummarizerConfig) -> Result<Self, SummarizerError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| SummarizerError::InitializationError(e.to_string()))?;
        
        Ok(Self { client, config })
    }
    
    /// Create with default configuration
    pub fn default() -> Result<Self, SummarizerError> {
        Self::new(SummarizerConfig::default())
    }
    
    /// Build summarization prompt
    fn build_prompt(&self, texts: &[String], max_tokens: usize) -> String {
        let combined = texts.join("\n\n---\n\n");
        format!(
            "Summarize the following conversation turns into a concise running brief. \
            Focus on key decisions, evidence, constraints, and open items. \
            Keep the summary under {} tokens.\n\n{}",
            max_tokens, combined
        )
    }
}

#[async_trait]
impl Summarizer for LLMSummarizer {
    async fn summarize(&self, texts: &[String], max_tokens: usize) -> Result<String, SummarizerError> {
        if texts.is_empty() {
            return Ok(String::new());
        }
        
        debug!("Summarizing {} text segments, target: {} tokens", texts.len(), max_tokens);
        
        let prompt = self.build_prompt(texts, max_tokens);
        
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "You are a concise summarizer. Extract key information and compress it efficiently.".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
            max_tokens: Some(max_tokens),
            temperature: Some(0.3),
        };
        
        // Retry logic
        let mut last_error = None;
        for attempt in 0..self.config.max_retries {
            if attempt > 0 {
                debug!("Retry attempt {} for summarization", attempt);
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
            }
            
            let mut req = self.client
                .post(&self.config.endpoint)
                .json(&request);
            
            if let Some(ref api_key) = self.config.api_key {
                req = req.header("Authorization", format!("Bearer {}", api_key));
            }
            
            match req.send().await {
                Ok(response) => {
                    if !response.status().is_success() {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        last_error = Some(SummarizerError::ApiError(format!(
                            "HTTP {}: {}", status, body
                        )));
                        continue;
                    }
                    
                    match response.json::<ChatCompletionResponse>().await {
                        Ok(resp) => {
                            if let Some(choice) = resp.choices.first() {
                                debug!("Summarization successful");
                                return Ok(choice.message.content.clone());
                            } else {
                                last_error = Some(SummarizerError::ApiError(
                                    "No choices in response".to_string()
                                ));
                            }
                        }
                        Err(e) => {
                            last_error = Some(SummarizerError::ApiError(format!(
                                "Failed to parse response: {}", e
                            )));
                        }
                    }
                }
                Err(e) => {
                    last_error = Some(SummarizerError::NetworkError(e.to_string()));
                }
            }
        }
        
        warn!("Summarization failed after {} attempts", self.config.max_retries);
        Err(last_error.unwrap_or(SummarizerError::Unknown))
    }
}

/// Simple concatenation-based summarizer (fallback)
pub struct ConcatenationSummarizer;

#[async_trait]
impl Summarizer for ConcatenationSummarizer {
    async fn summarize(&self, texts: &[String], _max_tokens: usize) -> Result<String, SummarizerError> {
        Ok(texts.join("\n"))
    }
}

/// Summarizer errors
#[derive(Debug, thiserror::Error)]
pub enum SummarizerError {
    #[error("Initialization error: {0}")]
    InitializationError(String),
    
    #[error("API error: {0}")]
    ApiError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Unknown error")]
    Unknown,
}

// OpenAI-compatible API types
#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_concatenation_summarizer() {
        let summarizer = ConcatenationSummarizer;
        let texts = vec!["Hello".to_string(), "World".to_string()];
        let result = summarizer.summarize(&texts, 100).await.unwrap();
        assert_eq!(result, "Hello\nWorld");
    }

    #[test]
    fn test_summarizer_config_default() {
        let config = SummarizerConfig::default();
        assert_eq!(config.model, "gpt-3.5-turbo");
        assert_eq!(config.max_retries, 3);
    }
}