//! Integration tests for enhanced HiRAG system
//!
//! These tests verify the integration of token budget management,
//! vision API, and facts store with the existing HiRAG system.

use context_manager::{
    context::{TokenBudgetManager, AdaptiveContextManager},
    config::{Config, TokenBudgetConfig, VisionConfig, FactsConfig},
};

#[test]
fn test_config_loading_with_new_sections() {
    // Test that configuration can be loaded with new sections
    let config_str = r#"
        [embedding]
        api_url = "https://test.com"
        api_token = "test_token"
        
        [vector_db]
        url = "http://localhost:6334"
        
        [hirag]
        l1_size = 10
        l2_size = 100
        
        [protocol]
        version = "1.0.0"
        
        [logging]
        level = "info"
        
        [server]
        port = 8081
        
        [token_budget]
        system_tokens = 700
        running_brief = 1200
        recent_turns = 450
        retrieved_context = 3750
        completion = 1000
        max_total = 8000
        
        [vision]
        service_url = "http://localhost:8080"
        timeout_ms = 5000
        max_regions_per_request = 16
        default_fidelity = "10x"
        
        [facts]
        collection_name = "facts"
        dedup_enabled = true
        confidence_threshold = 0.8
        max_facts_per_query = 100
    "#;
    
    // Note: This would require implementing config parsing from string
    // For now, we test that the structs can be created with defaults
    
    let token_budget = TokenBudgetConfig::default();
    assert_eq!(token_budget.system_tokens, 700);
    assert_eq!(token_budget.max_total, 8000);
    
    let vision = VisionConfig::default();
    assert_eq!(vision.service_url, "http://localhost:8080");
    assert_eq!(vision.max_regions_per_request, 16);
    
    let facts = FactsConfig::default();
    assert_eq!(facts.collection_name, "facts");
    assert_eq!(facts.confidence_threshold, 0.8);
}

#[test]
fn test_token_budget_manager_integration() {
    // Test that TokenBudgetManager can be created and used
    let manager = TokenBudgetManager::default();
    assert!(manager.is_ok());
    
    let manager = manager.unwrap();
    
    // Test token estimation
    let text = "This is a test sentence with multiple words.";
    let tokens = manager.estimate_tokens(text);
    assert!(tokens > 0);
    assert!(tokens < 20); // Should be around 10-15 tokens
    
    // Test budget checking
    assert!(manager.check_budget(7000).is_ok());
    assert!(manager.check_budget(9000).is_err());
    
    // Test allocation
    let allocation = manager.allocate(700, 1200, 450, 3750, 1000);
    assert!(allocation.is_ok());
    
    let alloc = allocation.unwrap();
    assert_eq!(alloc.total_allocated, 8100);
}

#[tokio::test]
async fn test_adaptive_context_manager_integration() {
    // Test that AdaptiveContextManager can be created and used
    let manager = AdaptiveContextManager::default();
    assert!(manager.is_ok());
    
    let manager = manager.unwrap();
    
    // Test relevance calculation
    let artifact = "This is a Rust programming example";
    let query = "Rust programming";
    let score = manager.calculate_relevance(artifact, query, 0.8, 0.6, 5);
    
    assert!(score.total > 0.0);
    assert!(score.total <= 1.0);
    assert!(score.task_relevance > 0.0); // Should have some overlap
}

#[test]
fn test_vision_config_validation() {
    let config = VisionConfig::default();
    
    // Validate default values
    assert_eq!(config.service_url, "http://localhost:8080");
    assert_eq!(config.timeout_ms, 5000);
    assert_eq!(config.max_regions_per_request, 16);
    assert_eq!(config.default_fidelity, "10x");
    
    // Test that timeout is reasonable
    assert!(config.timeout_ms >= 1000); // At least 1 second
    assert!(config.timeout_ms <= 60000); // At most 60 seconds
    
    // Test that max regions is reasonable
    assert!(config.max_regions_per_request > 0);
    assert!(config.max_regions_per_request <= 50);
}

#[test]
fn test_facts_config_validation() {
    let config = FactsConfig::default();
    
    // Validate default values
    assert_eq!(config.collection_name, "facts");
    assert_eq!(config.dedup_enabled, true);
    assert_eq!(config.confidence_threshold, 0.8);
    assert_eq!(config.max_facts_per_query, 100);
    
    // Test that confidence threshold is valid
    assert!(config.confidence_threshold >= 0.0);
    assert!(config.confidence_threshold <= 1.0);
    
    // Test that max facts is reasonable
    assert!(config.max_facts_per_query > 0);
    assert!(config.max_facts_per_query <= 1000);
}

#[test]
fn test_token_budget_config_totals() {
    let config = TokenBudgetConfig::default();
    
    // Calculate total allocated
    let total = config.system_tokens
        + config.running_brief
        + config.recent_turns
        + config.retrieved_context
        + config.completion;
    
    // Should be close to max_total (within 10%)
    let diff = (total as i32 - config.max_total as i32).abs();
    let tolerance = (config.max_total as f32 * 0.1) as i32;
    
    assert!(
        diff <= tolerance,
        "Total allocated ({}) differs from max_total ({}) by more than 10%",
        total,
        config.max_total
    );
}

#[test]
fn test_all_configs_have_defaults() {
    // Test that all config structs can be created with defaults
    let token_budget = TokenBudgetConfig::default();
    let vision = VisionConfig::default();
    let facts = FactsConfig::default();
    
    // Verify they're not empty/zero
    assert!(token_budget.max_total > 0);
    assert!(!vision.service_url.is_empty());
    assert!(!facts.collection_name.is_empty());
}

// Note: The following tests require a running Qdrant instance
// and are marked as ignored by default

#[tokio::test]
#[ignore]
async fn test_facts_store_integration() {
    use context_manager::facts::{FactStore, FactStoreConfig, FactInsertRequest, SourceAnchor};
    use qdrant_client::client::QdrantClient;
    
    let client = QdrantClient::from_url("http://localhost:6334")
        .build()
        .unwrap();
    
    let config = FactStoreConfig::default();
    let store = FactStore::new(client, config).await;
    assert!(store.is_ok());
    
    let store = store.unwrap();
    
    // Test fact insertion
    let request = FactInsertRequest {
        subject: "Rust".to_string(),
        predicate: "is_a".to_string(),
        object: "programming_language".to_string(),
        datatype: None,
        source_doc: None,
        source_anchor: SourceAnchor::default(),
        confidence: 0.95,
    };
    
    let result = store.insert_fact(request).await;
    assert!(result.is_ok());
}

#[tokio::test]
#[ignore]
async fn test_vision_client_integration() {
    use context_manager::api::vision::{
        VisionServiceClient,
        VisionSearchRequest,
    };
    use std::collections::HashMap;
    
    let client = VisionServiceClient::default();
    assert!(client.is_ok());
    
    let client = client.unwrap();
    
    // Test search (will use stub implementation)
    let request = VisionSearchRequest {
        query: "test query".to_string(),
        top_k: 10,
        filters: HashMap::new(),
    };
    
    let result = client.search_regions(request).await;
    assert!(result.is_ok());
}

#[test]
fn test_module_exports() {
    // Test that all new modules are properly exported
    use context_manager::prelude::*;
    
    // These should all compile without errors
    let _: Option<TokenBudgetManager> = None;
    let _: Option<AdaptiveContextManager> = None;
    let _: Option<BudgetAllocation> = None;
    let _: Option<FactStore> = None;
    let _: Option<Fact> = None;
    let _: Option<VisionState> = None;
}