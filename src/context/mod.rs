//! Context management with token budget enforcement
//!
//! This module provides adaptive context management with strict token budget
//! enforcement (≤8k tokens per turn) as specified in the brainstorming document.

pub mod token_budget;
pub mod adaptive_manager;
pub mod models;

pub use token_budget::{TokenBudgetManager, BudgetAllocation, BudgetError};
pub use adaptive_manager::{AdaptiveContextManager, AdaptiveContext};
pub use models::{ContextPriority, RelevanceScore};