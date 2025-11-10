//! Adaptive context manager with token budget enforcement
//!
//! Implements adaptive context orchestration with:
//! - Smart context prioritization
//! - Token budget enforcement
//! - Summarize-then-retry logic
//! - Information-preserving summarization

use super::models::{ContextArtifact, ContextPriority, RelevanceScore};
use super::token_budget::{BudgetAllocation, BudgetError, TokenBudgetManager};
use super::summarizer::{Summarizer, LLMSummarizer, ConcatenationSummarizer, SummarizerConfig};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Adaptive context with budget tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveContext {
    pub system_prompt: String,
    pub running_brief: String,
    pub recent_turns: Vec<String>,
    pub retrieved_snippets: Vec<ContextArtifact>,
    pub budget_allocation: BudgetAllocation,
    pub metadata: HashMap<String, String>,
}

impl AdaptiveContext {
    /// Get the total token count
    pub fn total_tokens(&self) -> usize {
        self.budget_allocation.total_allocated
    }

    /// Check if context is within budget
    pub fn is_within_budget(&self, max_tokens: usize) -> bool {
        self.total_tokens() <= max_tokens
    }
}

/// Adaptive context manager
pub struct AdaptiveContextManager {
    budget_manager: TokenBudgetManager,
    summarizer: Arc<dyn Summarizer>,
}

impl AdaptiveContextManager {
    /// Create a new adaptive context manager with custom summarizer
    pub fn new(budget_manager: TokenBudgetManager, summarizer: Arc<dyn Summarizer>) -> Self {
        Self { budget_manager, summarizer }
    }

    /// Create with default budget configuration and LLM summarizer (with fallback)
    pub fn default() -> Result<Self> {
        let budget_manager = TokenBudgetManager::default()
            .map_err(|e| crate::error::ContextError::Configuration(e.to_string()))?;
        
        // Try LLM summarizer first, fallback to concatenation for resilience
        let summarizer: Arc<dyn Summarizer> = LLMSummarizer::default()
            .map(|s| Arc::new(s) as Arc<dyn Summarizer>)
            .unwrap_or_else(|_| {
                warn!("LLM summarizer initialization failed, falling back to concatenation");
                Arc::new(ConcatenationSummarizer::default())
            });
        
        Ok(Self {
            budget_manager,
            summarizer,
        })
    }

    /// Create with LLM summarizer (production recommended)
    pub fn with_llm_summarizer(
        budget_manager: TokenBudgetManager,
        config: SummarizerConfig,
    ) -> Result<Self> {
        let summarizer = LLMSummarizer::new(config)
            .map_err(|e| crate::error::ContextError::Configuration(e.to_string()))?;
        Ok(Self {
            budget_manager,
            summarizer: Arc::new(summarizer),
        })
    }

    /// Create with concatenation summarizer (fallback)
    pub fn with_concat_summarizer(budget_manager: TokenBudgetManager) -> Result<Self> {
        let summarizer = ConcatenationSummarizer::default();
        Ok(Self {
            budget_manager,
            summarizer: Arc::new(summarizer),
        })
    }

    /// Build adaptive context from components
    pub async fn build_context(
        &self,
        system_prompt: String,
        running_brief: String,
        recent_turns: Vec<String>,
        artifacts: Vec<ContextArtifact>,
    ) -> Result<AdaptiveContext> {
        // Estimate tokens for each component
        let system_tokens = self.budget_manager.estimate_tokens(&system_prompt);
        let brief_tokens = self.budget_manager.estimate_tokens(&running_brief);
        let turns_tokens: usize = recent_turns
            .iter()
            .map(|t| self.budget_manager.estimate_tokens(t))
            .sum();

        // Prioritize and select artifacts
        let selected_artifacts = self.prioritize_artifacts(artifacts).await?;
        let context_tokens: usize = selected_artifacts.iter().map(|a| a.token_count).sum();

        // Reserve space for completion
        let completion_tokens = self.budget_manager.config().completion;

        // Check if we're within budget
        let total_tokens =
            system_tokens + brief_tokens + turns_tokens + context_tokens + completion_tokens;

        debug!(
            "Token allocation: system={}, brief={}, turns={}, context={}, completion={}, total={}",
            system_tokens, brief_tokens, turns_tokens, context_tokens, completion_tokens, total_tokens
        );

        if total_tokens > self.budget_manager.max_total() {
            warn!(
                "Context exceeds budget: {} > {}",
                total_tokens,
                self.budget_manager.max_total()
            );
            // Trigger summarize-then-retry
            return self
                .summarize_and_retry(
                    system_prompt,
                    running_brief,
                    recent_turns,
                    selected_artifacts,
                )
                .await;
        }

        // Create budget allocation
        let allocation = self.budget_manager.allocate(
            system_tokens,
            brief_tokens,
            turns_tokens,
            context_tokens,
            completion_tokens,
        )?;

        Ok(AdaptiveContext {
            system_prompt,
            running_brief,
            recent_turns,
            retrieved_snippets: selected_artifacts,
            budget_allocation: allocation,
            metadata: HashMap::new(),
        })
    }

    /// Prioritize artifacts based on relevance scores
    async fn prioritize_artifacts(
        &self,
        mut artifacts: Vec<ContextArtifact>,
    ) -> Result<Vec<ContextArtifact>> {
        // Sort by priority first, then by relevance score
        artifacts.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| b.relevance.total.partial_cmp(&a.relevance.total).unwrap())
        });

        // Limit to recommended snippet count
        let max_snippets = self.budget_manager.recommended_snippet_count();
        artifacts.truncate(max_snippets);

        debug!(
            "Prioritized {} artifacts (max: {})",
            artifacts.len(),
            max_snippets
        );

        Ok(artifacts)
    }

    /// Summarize older context and retry with smaller budget
    async fn summarize_and_retry(
        &self,
        system_prompt: String,
        running_brief: String,
        recent_turns: Vec<String>,
        artifacts: Vec<ContextArtifact>,
    ) -> Result<AdaptiveContext> {
        info!("Triggering summarize-then-retry due to budget overflow");

        // Summarize older turns into running brief
        let summarized_brief = self.summarize_turns(&running_brief, &recent_turns).await?;

        // Keep only the most recent turn
        let recent_turns = if !recent_turns.is_empty() {
            vec![recent_turns.last().unwrap().clone()]
        } else {
            vec![]
        };

        // Shrink retrieved context by removing lowest priority items
        let shrunk_artifacts = self.shrink_artifacts(artifacts).await?;

        // Retry with summarized context
        let system_tokens = self.budget_manager.estimate_tokens(&system_prompt);
        let brief_tokens = self.budget_manager.estimate_tokens(&summarized_brief);
        let turns_tokens: usize = recent_turns
            .iter()
            .map(|t| self.budget_manager.estimate_tokens(t))
            .sum();
        let context_tokens: usize = shrunk_artifacts.iter().map(|a| a.token_count).sum();
        let completion_tokens = self.budget_manager.config().completion;

        let total_tokens =
            system_tokens + brief_tokens + turns_tokens + context_tokens + completion_tokens;

        if total_tokens > self.budget_manager.max_total() {
            return Err(crate::error::ContextError::Configuration(format!(
                "Cannot fit context within budget even after summarization: {} > {}",
                total_tokens,
                self.budget_manager.max_total()
            )));
        }

        let allocation = self.budget_manager.allocate(
            system_tokens,
            brief_tokens,
            turns_tokens,
            context_tokens,
            completion_tokens,
        )?;

        info!(
            "Successfully reduced context from overflow to {} tokens",
            total_tokens
        );

        Ok(AdaptiveContext {
            system_prompt,
            running_brief: summarized_brief,
            recent_turns,
            retrieved_snippets: shrunk_artifacts,
            budget_allocation: allocation,
            metadata: HashMap::new(),
        })
    }

    /// Summarize turns into running brief using configured summarizer
    async fn summarize_turns(
        &self,
        current_brief: &str,
        turns: &[String],
    ) -> Result<String> {
        if turns.is_empty() {
            return Ok(current_brief.to_string());
        }

        // Combine current brief with turns
        let mut texts_to_summarize = vec![current_brief.to_string()];
        texts_to_summarize.extend(turns.iter().cloned());

        // Calculate target token count (running brief budget)
        let target_tokens = self.budget_manager.config().running_brief;

        // Use configured summarizer
        let summary = self.summarizer
            .summarize(&texts_to_summarize, target_tokens)
            .await
            .map_err(|e| crate::error::ContextError::Configuration(e.to_string()))?;

        debug!(
            "Summarized {} texts into {} tokens",
            texts_to_summarize.len(),
            self.budget_manager.estimate_tokens(&summary)
        );

        Ok(summary)
    }

    /// Shrink artifacts by removing lowest priority items
    async fn shrink_artifacts(
        &self,
        mut artifacts: Vec<ContextArtifact>,
    ) -> Result<Vec<ContextArtifact>> {
        // Remove lowest priority items until we fit
        let target_count = (artifacts.len() * 2) / 3; // Keep 2/3 of artifacts
        artifacts.truncate(target_count.max(4)); // Keep at least 4 artifacts

        debug!(
            "Shrunk artifacts from {} to {} items",
            artifacts.len(),
            target_count
        );

        Ok(artifacts)
    }

    /// Calculate relevance score for an artifact
    pub fn calculate_relevance(
        &self,
        artifact: &str,
        query: &str,
        recency_factor: f32,
        complexity_factor: f32,
        reference_count: usize,
    ) -> RelevanceScore {
        // Simple relevance calculation
        // In production, this should use embedding similarity

        // Task relevance: simple keyword overlap
        let artifact_words: std::collections::HashSet<_> =
            artifact.to_lowercase().split_whitespace().collect();
        let query_words: std::collections::HashSet<_> =
            query.to_lowercase().split_whitespace().collect();
        let overlap = artifact_words.intersection(&query_words).count();
        let task_relevance = (overlap as f32) / (query_words.len().max(1) as f32);

        // Recency: provided as parameter (0.0-1.0)
        let recency = recency_factor.clamp(0.0, 1.0);

        // Complexity: provided as parameter (0.0-1.0)
        let complexity = complexity_factor.clamp(0.0, 1.0);

        // Reference density: normalize reference count
        let reference_density = (reference_count as f32 / 10.0).min(1.0);

        RelevanceScore::new(task_relevance, recency, complexity, reference_density)
    }

    /// Get the budget manager
    pub fn budget_manager(&self) -> &TokenBudgetManager {
        &self.budget_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_adaptive_manager_creation() {
        let manager = AdaptiveContextManager::default();
        assert!(manager.is_ok());
    }

    #[tokio::test]
    async fn test_build_context_within_budget() {
        let manager = AdaptiveContextManager::default().unwrap();

        let system_prompt = "You are a helpful assistant.".to_string();
        let running_brief = "User is working on a Rust project.".to_string();
        let recent_turns = vec!["What is the syntax for async functions?".to_string()];
        let artifacts = vec![];

        let context = manager
            .build_context(system_prompt, running_brief, recent_turns, artifacts)
            .await;

        assert!(context.is_ok());
        let ctx = context.unwrap();
        assert!(ctx.is_within_budget(8000));
    }

    #[tokio::test]
    async fn test_calculate_relevance() {
        let manager = AdaptiveContextManager::default().unwrap();

        let artifact = "This is a Rust async function example";
        let query = "async function syntax";
        let score = manager.calculate_relevance(artifact, query, 0.8, 0.6, 5);

        assert!(score.total > 0.0);
        assert!(score.total <= 1.0);
    }

    #[tokio::test]
    async fn test_prioritize_artifacts() {
        let manager = AdaptiveContextManager::default().unwrap();

        let artifacts = vec![
            ContextArtifact::new(
                "1".to_string(),
                "Low priority".to_string(),
                HashMap::new(),
                ContextPriority::Low,
                RelevanceScore::new(0.3, 0.2, 0.1, 0.1),
                50,
            ),
            ContextArtifact::new(
                "2".to_string(),
                "High priority".to_string(),
                HashMap::new(),
                ContextPriority::High,
                RelevanceScore::new(0.9, 0.8, 0.7, 0.6),
                50,
            ),
        ];

        let prioritized = manager.prioritize_artifacts(artifacts).await.unwrap();
        assert_eq!(prioritized[0].id, "2"); // High priority should be first
    }
}