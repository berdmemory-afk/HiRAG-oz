//! Data models for context management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Priority level for context items
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ContextPriority {
    Critical = 4,
    High = 3,
    Medium = 2,
    Low = 1,
}

/// Relevance score for context prioritization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevanceScore {
    /// Task relevance (0.0-1.0)
    pub task_relevance: f32,
    /// Recency score (0.0-1.0)
    pub recency: f32,
    /// Complexity importance (0.0-1.0)
    pub complexity: f32,
    /// Cross-reference density (0.0-1.0)
    pub reference_density: f32,
    /// Overall weighted score
    pub total: f32,
}

impl RelevanceScore {
    /// Calculate weighted total score
    /// Weights: task_relevance (40%), recency (20%), complexity (20%), reference_density (20%)
    pub fn calculate_total(&mut self) {
        self.total = self.task_relevance * 0.4
            + self.recency * 0.2
            + self.complexity * 0.2
            + self.reference_density * 0.2;
    }

    /// Create a new relevance score with calculated total
    pub fn new(
        task_relevance: f32,
        recency: f32,
        complexity: f32,
        reference_density: f32,
    ) -> Self {
        let mut score = Self {
            task_relevance,
            recency,
            complexity,
            reference_density,
            total: 0.0,
        };
        score.calculate_total();
        score
    }
}

/// Context artifact with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextArtifact {
    pub id: String,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub priority: ContextPriority,
    pub relevance: RelevanceScore,
    pub token_count: usize,
}

impl ContextArtifact {
    pub fn new(
        id: String,
        content: String,
        metadata: HashMap<String, String>,
        priority: ContextPriority,
        relevance: RelevanceScore,
        token_count: usize,
    ) -> Self {
        Self {
            id,
            content,
            metadata,
            priority,
            relevance,
            token_count,
        }
    }
}