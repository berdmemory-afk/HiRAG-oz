//! Enhanced HiRAG manager with token budget integration
//!
//! This module extends the existing HiRAG manager with token budget
//! enforcement and adaptive context management.

use crate::{
    context::{
        AdaptiveContextManager, TokenBudgetManager, ContextArtifact,
        ContextPriority, RelevanceScore,
    },
    error::Result,
    hirag::{HiRAGManager, ContextRequest, ContextResponse},
};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Enhanced HiRAG manager with token budget enforcement
pub struct EnhancedHiRAGManager {
    /// Base HiRAG manager
    base_manager: Arc<HiRAGManager>,
    /// Adaptive context manager
    context_manager: AdaptiveContextManager,
    /// Token budget manager
    budget_manager: TokenBudgetManager,
}

impl EnhancedHiRAGManager {
    /// Create a new enhanced HiRAG manager
    pub fn new(
        base_manager: Arc<HiRAGManager>,
        context_manager: AdaptiveContextManager,
        budget_manager: TokenBudgetManager,
    ) -> Self {
        Self {
            base_manager,
            context_manager,
            budget_manager,
        }
    }

    /// Create with default token budget configuration
    pub fn with_defaults(base_manager: Arc<HiRAGManager>) -> Result<Self> {
        let context_manager = AdaptiveContextManager::default()?;
        let budget_manager = TokenBudgetManager::default()?;
        
        Ok(Self {
            base_manager,
            context_manager,
            budget_manager,
        })
    }

    /// Store context with token budget awareness
    pub async fn store_context_with_budget(
        &self,
        content: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String> {
        // Estimate tokens for the content
        let token_count = self.budget_manager.estimate_tokens(content);
        
        debug!("Storing context: {} tokens", token_count);
        
        // Check if content fits within budget
        if token_count > self.budget_manager.max_total() {
            warn!(
                "Content exceeds maximum token budget: {} > {}",
                token_count,
                self.budget_manager.max_total()
            );
            // Could implement chunking here
        }
        
        // Delegate to base manager
        self.base_manager.store_context(content, metadata).await
    }

    /// Retrieve context with adaptive selection
    pub async fn retrieve_context_adaptive(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<ContextArtifact>> {
        // Get contexts from base manager
        let request = ContextRequest {
            query: query.to_string(),
            max_results,
            filters: HashMap::new(),
        };
        
        let response = self.base_manager.retrieve_contexts(request).await?;
        
        // Convert to ContextArtifacts with relevance scoring
        let mut artifacts = Vec::new();
        
        for (idx, context) in response.contexts.iter().enumerate() {
            let token_count = self.budget_manager.estimate_tokens(&context.content);
            
            // Calculate relevance score
            let relevance = self.context_manager.calculate_relevance(
                &context.content,
                query,
                1.0 - (idx as f32 / max_results as f32), // Recency based on position
                0.5, // Default complexity
                context.metadata.len(), // Reference count approximation
            );
            
            let artifact = ContextArtifact::new(
                context.id.clone(),
                context.content.clone(),
                context.metadata.clone(),
                ContextPriority::Medium, // Could be derived from metadata
                relevance,
                token_count,
            );
            
            artifacts.push(artifact);
        }
        
        info!("Retrieved {} context artifacts", artifacts.len());
        
        Ok(artifacts)
    }

    /// Build adaptive context for a query
    pub async fn build_adaptive_context(
        &self,
        query: &str,
        system_prompt: String,
        running_brief: String,
        recent_turns: Vec<String>,
        max_results: usize,
    ) -> Result<crate::context::AdaptiveContext> {
        // Retrieve relevant contexts
        let artifacts = self.retrieve_context_adaptive(query, max_results).await?;
        
        // Build adaptive context with budget enforcement
        let context = self.context_manager
            .build_context(
                system_prompt,
                running_brief,
                recent_turns,
                artifacts,
            )
            .await?;
        
        info!(
            "Built adaptive context: {} tokens (budget: {})",
            context.total_tokens(),
            self.budget_manager.max_total()
        );
        
        Ok(context)
    }

    /// Get the token budget manager
    pub fn budget_manager(&self) -> &TokenBudgetManager {
        &self.budget_manager
    }

    /// Get the context manager
    pub fn context_manager(&self) -> &AdaptiveContextManager {
        &self.context_manager
    }

    /// Get the base HiRAG manager
    pub fn base_manager(&self) -> &Arc<HiRAGManager> {
        &self.base_manager
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a mock HiRAGManager
    // They are marked as ignored by default

    #[tokio::test]
    #[ignore]
    async fn test_enhanced_manager_creation() {
        // Would need to create a mock HiRAGManager
        // let base_manager = Arc::new(mock_hirag_manager());
        // let manager = EnhancedHiRAGManager::with_defaults(base_manager);
        // assert!(manager.is_ok());
    }

    #[test]
    fn test_token_estimation() {
        // Test that token estimation works
        let budget_manager = TokenBudgetManager::default().unwrap();
        let text = "This is a test sentence.";
        let tokens = budget_manager.estimate_tokens(text);
        assert!(tokens > 0);
    }
}