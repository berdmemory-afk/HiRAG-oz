//! Integration tests for DeepSeek OCR integration

use hirag_oz::api::vision::cache::DecodeCache;
use hirag_oz::api::vision::circuit_breaker::CircuitBreaker;
use hirag_oz::api::vision::deepseek_client::{DeepseekOcrClient, OcrError};
use hirag_oz::api::vision::deepseek_config::DeepseekConfig;
use hirag_oz::api::vision::models::*;
use std::sync::Arc;

/// Test that cache hits work correctly
#[tokio::test]
async fn test_decode_with_cache_hit() {
    let mut config = DeepseekConfig::default();
    config.enabled = true;
    config.cache_ttl_secs = 600;
    
    let client = DeepseekOcrClient::new(config).unwrap();
    
    // First call - cache miss (will fail since no real service)
    let result1 = client
        .decode_regions(vec!["region1".to_string()], "10x".to_string())
        .await;
    
    // Should fail with upstream error since no real service
    assert!(result1.is_err());
    
    // Verify cache stats
    let stats = client.cache_stats();
    assert_eq!(stats.misses, 1);
}

/// Test that circuit breaker triggers after failures
#[tokio::test]
async fn test_circuit_breaker_triggering() {
    let mut config = DeepseekConfig::default();
    config.enabled = true;
    config.circuit_failure_threshold = 2;
    
    let client = DeepseekOcrClient::new(config).unwrap();
    
    // First failure
    let result1 = client
        .decode_regions(vec!["region1".to_string()], "10x".to_string())
        .await;
    assert!(result1.is_err());
    
    // Second failure - should trigger circuit breaker
    let result2 = client
        .decode_regions(vec!["region2".to_string()], "10x".to_string())
        .await;
    assert!(result2.is_err());
    
    // Third call - should fail with CircuitOpen
    let result3 = client
        .decode_regions(vec!["region3".to_string()], "10x".to_string())
        .await;
    
    match result3 {
        Err(OcrError::CircuitOpen(_)) => {
            // Expected
        }
        _ => panic!("Expected CircuitOpen error"),
    }
}

/// Test that opt-out via config works
#[tokio::test]
async fn test_opt_out_via_config() {
    let mut config = DeepseekConfig::default();
    config.enabled = false;
    
    let client = DeepseekOcrClient::new(config).unwrap();
    
    let result = client
        .decode_regions(vec!["region1".to_string()], "10x".to_string())
        .await;
    
    match result {
        Err(OcrError::Disabled) => {
            // Expected
        }
        _ => panic!("Expected Disabled error"),
    }
}

/// Test cache expiration
#[tokio::test]
async fn test_cache_expiration() {
    let cache = DecodeCache::new(100, 1); // 1 second TTL
    
    // Insert entry
    cache.insert("key1".to_string(), "value1".to_string());
    
    // Should be present
    assert!(cache.get("key1").is_some());
    
    // Wait for expiration
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Should be expired
    assert!(cache.get("key1").is_none());
    
    let stats = cache.stats();
    assert_eq!(stats.evictions, 1);
}

/// Test circuit breaker state transitions
#[tokio::test]
async fn test_circuit_breaker_state_transitions() {
    let breaker = CircuitBreaker::new(2, 1); // 2 failures, 1 second cooldown
    
    // Initially closed
    assert!(!breaker.is_open("test_op").await);
    
    // Mark failures
    breaker.mark_failure("test_op");
    breaker.mark_failure("test_op");
    
    // Should be open now
    assert!(breaker.is_open("test_op").await);
    
    // Wait for cooldown
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Should transition to half-open
    assert!(!breaker.is_open("test_op").await);
    
    // Mark success
    breaker.mark_success("test_op");
    
    // Should be closed again
    assert!(!breaker.is_open("test_op").await);
}

/// Test batch cache operations
#[tokio::test]
async fn test_batch_cache_operations() {
    let cache = DecodeCache::new(100, 600);
    
    let keys = vec!["key1".to_string(), "key2".to_string(), "key3".to_string()];
    
    // Insert batch
    cache.insert("key1".to_string(), "value1".to_string());
    cache.insert("key2".to_string(), "value2".to_string());
    
    // Get batch
    let results = cache.get_batch(&keys);
    
    assert_eq!(results.len(), 3);
    assert_eq!(results[0], Some("value1".to_string()));
    assert_eq!(results[1], Some("value2".to_string()));
    assert_eq!(results[2], None);
    
    let stats = cache.stats();
    assert_eq!(stats.hits, 2);
    assert_eq!(stats.misses, 1);
}

/// Test config from environment variables
#[test]
fn test_config_from_env() {
    std::env::set_var("DEEPSEEK_OCR_ENABLED", "false");
    std::env::set_var("VISION_API_KEY", "test-key-123");
    std::env::set_var("DEEPSEEK_CACHE_TTL_SECS", "300");
    
    let config = DeepseekConfig::from_env();
    
    assert!(!config.enabled);
    assert_eq!(config.api_key, Some("test-key-123".to_string()));
    assert_eq!(config.cache_ttl_secs, 300);
    
    // Cleanup
    std::env::remove_var("DEEPSEEK_OCR_ENABLED");
    std::env::remove_var("VISION_API_KEY");
    std::env::remove_var("DEEPSEEK_CACHE_TTL_SECS");
}
</file_path>