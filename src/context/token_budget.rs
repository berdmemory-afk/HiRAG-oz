//! Token budget management for ≤8k token enforcement
//!
//! Implements the token budget policy from brainstorming.md:
//! - System/Instructions: 600-800 tokens
//! - Running Brief: 1,000-1,500 tokens
//! - Recent Turns: 300-600 tokens
//! - Retrieved Context: 3,000-4,500 tokens (8-12 snippets; 250-400 tokens each)
//! - Completion: 800-1,200 tokens
//! - Total: ≤8,000 tokens

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Token budget configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudgetConfig {
    pub system_tokens: usize,
    pub running_brief: usize,
    pub recent_turns: usize,
    pub retrieved_context: usize,
    pub completion: usize,
    pub max_total: usize,
}

impl Default for TokenBudgetConfig {
    fn default() -> Self {
        Self {
            system_tokens: 700,
            running_brief: 1200,
            recent_turns: 450,
            retrieved_context: 3750,
            completion: 1000,
            max_total: 8000,
        }
    }
}

impl TokenBudgetConfig {
    /// Validate that the budget configuration is consistent
    pub fn validate(&self) -> Result<(), BudgetError> {
        let total = self.system_tokens
            + self.running_brief
            + self.recent_turns
            + self.retrieved_context
            + self.completion;

        if total > self.max_total {
            return Err(BudgetError::ConfigurationInvalid {
                allocated: total,
                max: self.max_total,
            });
        }

        Ok(())
    }
}

/// Budget allocation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetAllocation {
    pub system_tokens: usize,
    pub running_brief: usize,
    pub recent_turns: usize,
    pub retrieved_context: usize,
    pub completion: usize,
    pub total_allocated: usize,
    pub remaining: usize,
}

impl BudgetAllocation {
    /// Check if this allocation fits within the budget
    pub fn is_within_budget(&self, max_total: usize) -> bool {
        self.total_allocated <= max_total
    }
}

/// Token budget errors
#[derive(Debug, Error)]
pub enum BudgetError {
    #[error("Budget exceeded: {used} tokens used, {max} tokens allowed")]
    BudgetExceeded { used: usize, max: usize },

    #[error("Configuration invalid: {allocated} tokens allocated, {max} tokens max")]
    ConfigurationInvalid { allocated: usize, max: usize },

    #[error("Token estimation failed: {0}")]
    EstimationFailed(String),

    #[error("Insufficient budget: need {needed} tokens, have {available} tokens")]
    InsufficientBudget { needed: usize, available: usize },
}

/// Token budget manager
pub struct TokenBudgetManager {
    config: TokenBudgetConfig,
}

impl TokenBudgetManager {
    /// Create a new token budget manager
    pub fn new(config: TokenBudgetConfig) -> Result<Self, BudgetError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Create with default configuration
    pub fn default() -> Result<Self, BudgetError> {
        Self::new(TokenBudgetConfig::default())
    }

    /// Allocate tokens based on current usage
    pub fn allocate(
        &self,
        system_used: usize,
        brief_used: usize,
        turns_used: usize,
        context_used: usize,
        completion_used: usize,
    ) -> Result<BudgetAllocation, BudgetError> {
        let total = system_used + brief_used + turns_used + context_used + completion_used;

        if total > self.config.max_total {
            return Err(BudgetError::BudgetExceeded {
                used: total,
                max: self.config.max_total,
            });
        }

        Ok(BudgetAllocation {
            system_tokens: system_used,
            running_brief: brief_used,
            recent_turns: turns_used,
            retrieved_context: context_used,
            completion: completion_used,
            total_allocated: total,
            remaining: self.config.max_total - total,
        })
    }

    /// Check if a given token count fits within the budget
    pub fn check_budget(&self, tokens: usize) -> Result<(), BudgetError> {
        if tokens > self.config.max_total {
            return Err(BudgetError::BudgetExceeded {
                used: tokens,
                max: self.config.max_total,
            });
        }
        Ok(())
    }

    /// Estimate tokens for text (simple word-based approximation)
    /// In production, this should use tiktoken or model-specific tokenizer
    pub fn estimate_tokens(&self, text: &str) -> usize {
        // Simple approximation: ~1.3 tokens per word
        // This is a rough estimate; real implementation should use proper tokenizer
        let words = text.split_whitespace().count();
        ((words as f32) * 1.3) as usize
    }

    /// Calculate how much to shrink retrieved context to fit budget
    pub fn calculate_shrinkage(
        &self,
        current_total: usize,
        target_total: usize,
    ) -> Result<usize, BudgetError> {
        if current_total <= target_total {
            return Ok(0);
        }

        let excess = current_total - target_total;
        Ok(excess)
    }

    /// Get the maximum allowed tokens for retrieved context
    pub fn max_retrieved_context(&self) -> usize {
        self.config.retrieved_context
    }

    /// Get the maximum total tokens
    pub fn max_total(&self) -> usize {
        self.config.max_total
    }

    /// Get the configuration
    pub fn config(&self) -> &TokenBudgetConfig {
        &self.config
    }

    /// Calculate recommended snippet count based on budget
    pub fn recommended_snippet_count(&self) -> usize {
        // Target 8-12 snippets of 250-400 tokens each
        let avg_snippet_size = 325; // Average of 250-400
        let max_snippets = self.config.retrieved_context / avg_snippet_size;
        max_snippets.min(12).max(8)
    }

    /// Validate a budget allocation
    pub fn validate_allocation(&self, allocation: &BudgetAllocation) -> Result<(), BudgetError> {
        if !allocation.is_within_budget(self.config.max_total) {
            return Err(BudgetError::BudgetExceeded {
                used: allocation.total_allocated,
                max: self.config.max_total,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = TokenBudgetConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_budget_manager_creation() {
        let manager = TokenBudgetManager::default();
        assert!(manager.is_ok());
    }

    #[test]
    fn test_token_estimation() {
        let manager = TokenBudgetManager::default().unwrap();
        let text = "This is a test sentence with ten words in it.";
        let tokens = manager.estimate_tokens(text);
        assert!(tokens > 0);
        assert!(tokens < 20); // Should be around 13 tokens
    }

    #[test]
    fn test_budget_allocation_within_limit() {
        let manager = TokenBudgetManager::default().unwrap();
        let allocation = manager.allocate(700, 1200, 450, 3750, 1000);
        assert!(allocation.is_ok());
        let alloc = allocation.unwrap();
        assert_eq!(alloc.total_allocated, 8100);
    }

    #[test]
    fn test_budget_allocation_exceeds_limit() {
        let manager = TokenBudgetManager::default().unwrap();
        let allocation = manager.allocate(1000, 2000, 1000, 5000, 2000);
        assert!(allocation.is_err());
    }

    #[test]
    fn test_check_budget() {
        let manager = TokenBudgetManager::default().unwrap();
        assert!(manager.check_budget(7000).is_ok());
        assert!(manager.check_budget(9000).is_err());
    }

    #[test]
    fn test_calculate_shrinkage() {
        let manager = TokenBudgetManager::default().unwrap();
        let shrinkage = manager.calculate_shrinkage(9000, 8000).unwrap();
        assert_eq!(shrinkage, 1000);
    }

    #[test]
    fn test_recommended_snippet_count() {
        let manager = TokenBudgetManager::default().unwrap();
        let count = manager.recommended_snippet_count();
        assert!(count >= 8 && count <= 12);
    }
}