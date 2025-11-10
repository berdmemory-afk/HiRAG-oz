//! Token estimation using tiktoken

use tiktoken_rs::{cl100k_base, CoreBPE};
use std::sync::Arc;

/// Token estimator trait for different tokenization strategies
pub trait TokenEstimator: Send + Sync {
    /// Estimate the number of tokens in the given text
    fn estimate(&self, text: &str) -> usize;
    
    /// Estimate tokens for multiple texts
    fn estimate_batch(&self, texts: &[&str]) -> Vec<usize> {
        texts.iter().map(|t| self.estimate(t)).collect()
    }
}

/// Tiktoken-based token estimator using cl100k_base (GPT-4, GPT-3.5-turbo)
pub struct TiktokenEstimator {
    bpe: Arc<CoreBPE>,
}

impl TiktokenEstimator {
    /// Create a new tiktoken estimator with cl100k_base encoding
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let bpe = cl100k_base()?;
        Ok(Self {
            bpe: Arc::new(bpe),
        })
    }
    
    /// Create with default encoding (cl100k_base)
    pub fn default() -> Self {
        Self::new().expect("Failed to initialize tiktoken")
    }
}

impl TokenEstimator for TiktokenEstimator {
    fn estimate(&self, text: &str) -> usize {
        self.bpe.encode_with_special_tokens(text).len()
    }
}

/// Word-based token estimator (fallback, ~1.3 tokens per word)
pub struct WordBasedEstimator {
    tokens_per_word: f64,
}

impl WordBasedEstimator {
    pub fn new(tokens_per_word: f64) -> Self {
        Self { tokens_per_word }
    }
    
    pub fn default() -> Self {
        Self::new(1.3)
    }
}

impl TokenEstimator for WordBasedEstimator {
    fn estimate(&self, text: &str) -> usize {
        let word_count = text.split_whitespace().count();
        (word_count as f64 * self.tokens_per_word).ceil() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tiktoken_estimator() {
        let estimator = TiktokenEstimator::default();
        let text = "Hello, world! This is a test.";
        let tokens = estimator.estimate(text);
        assert!(tokens > 0);
        assert!(tokens < 20); // Should be around 8-10 tokens
    }

    #[test]
    fn test_word_based_estimator() {
        let estimator = WordBasedEstimator::default();
        let text = "Hello world test";
        let tokens = estimator.estimate(text);
        assert_eq!(tokens, 4); // 3 words * 1.3 = 3.9 -> 4
    }

    #[test]
    fn test_batch_estimation() {
        let estimator = TiktokenEstimator::default();
        let texts = vec!["Hello", "world", "test"];
        let tokens = estimator.estimate_batch(&texts);
        assert_eq!(tokens.len(), 3);
        assert!(tokens.iter().all(|&t| t > 0));
    }
}