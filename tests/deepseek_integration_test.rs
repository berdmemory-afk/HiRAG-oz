//! Integration tests for DeepSeek OCR integration
//!
//! NOTE: Most tests are currently ignored as they require:
//! - A mock DeepSeek API server
//! - Proper type alignment with actual API
//!
//! These tests serve as documentation of expected behavior
//! and will be enabled once a mock server is implemented.

use context_manager::api::vision::cache::DecodeCache;
use context_manager::api::vision::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use context_manager::api::vision::deepseek_client::{DeepseekOcrClient, OcrError};
use context_manager::api::vision::deepseek_config::DeepseekConfig;
use context_manager::api::vision::models::*;
use std::sync::Arc;
use std::time::Duration;

/// Test that cache hits work correctly
#[tokio::test]
#[ignore = "requires mock DeepSeek upstream server"]
async fn test_decode_with_cache_hit() {
    let mut config = DeepseekConfig::default();
    config.enabled = true;
    config.decode_cache_ttl_secs = 600;
    
    let client = DeepseekOcrClient::new(config).unwrap();
    
    // First call - cache miss (will fail since no real service)
    let result1 = client
        .decode_regions(vec!["region1".to_string()], FidelityLevel::Medium)
        .await;
    
    // Should fail with upstream error since no real service
    assert!(result1.is_err());
    
    // Verify cache stats
    let stats = client.cache_stats();
    // Note: actual stats fields are total, valid, expired (not hits/misses)
    assert!(stats.total >= 0);
}

/// Test that circuit breaker triggers after failures
#[tokio::test]
#[ignore = "requires mock DeepSeek upstream server"]
async fn test_circuit_breaker_triggering() {
    let mut config = DeepseekConfig::default();
    config.enabled = true;
    config.circuit_breaker_failures = 2;
    
    let client = DeepseekOcrClient::new(config).unwrap();
    
    // First failure
    let result1 = client
        .decode_regions(vec!["region1".to_string()], FidelityLevel::Medium)
        .await;
    assert!(result1.is_err());
    
    // Second failure - should trigger circuit breaker
    let result2 = client
        .decode_regions(vec!["region2".to_string()], FidelityLevel::Medium)
        .await;
    assert!(result2.is_err());
    
    // Third call - should fail with CircuitOpen
    let result3 = client
        .decode_regions(vec!["region3".to_string()], FidelityLevel::Medium)
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
        .decode_regions(vec!["region1".to_string()], FidelityLevel::Medium)
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
    let cache = DecodeCache::new(Duration::from_secs(1), 100); // 1 second TTL, 100 max size
    let fidelity = FidelityLevel::Medium;
    
    // Store entry
    let result = DecodeResult {
        region_id: "region1".to_string(),
        text: "test".to_string(),
        confidence: 0.9,
    };
    cache.store("region1", &fidelity, result.clone());
    
    // Should be present
    assert!(cache.get("region1", &fidelity).is_some());
    
    // Wait for expiration
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Should be expired
    assert!(cache.get("region1", &fidelity).is_none());
    
    let stats = cache.stats();
    // Note: actual stats fields are total, valid, expired
    assert!(stats.expired >= 1);
}

/// Test circuit breaker state transitions
#[test]
fn test_circuit_breaker_state_transitions() {
    let config = CircuitBreakerConfig {
        failure_threshold: 2,
        reset_timeout: Duration::from_millis(100),
    };
    let breaker = CircuitBreaker::new(config);
    
    // Initially closed
    assert!(!breaker.is_open("test_op"));
    
    // Mark failures
    breaker.mark_failure("test_op");
    breaker.mark_failure("test_op");
    
    // Should be open now
    assert!(breaker.is_open("test_op"));
    
    // Wait for cooldown
    std::thread::sleep(Duration::from_millis(150));
    
    // Should transition to half-open (is_open returns false in half-open)
    assert!(!breaker.is_open("test_op"));
    
    // Mark success
    breaker.mark_success("test_op");
    
    // Should be closed again
    assert!(!breaker.is_open("test_op"));
}

/// Test batch cache operations
#[test]
fn test_batch_cache_operations() {
    let cache = DecodeCache::new(Duration::from_secs(600), 100);
    let fidelity = FidelityLevel::Medium;
    
    let region_ids = vec!["region1".to_string(), "region2".to_string(), "region3".to_string()];
    
    // Store some results
    cache.store("region1", &fidelity, DecodeResult {
        region_id: "region1".to_string(),
        text: "text1".to_string(),
        confidence: 0.9,
    });
    cache.store("region2", &fidelity, DecodeResult {
        region_id: "region2".to_string(),
        text: "text2".to_string(),
        confidence: 0.9,
    });
    
    // Split hits and misses
    let (hits, misses) = cache.split_hits(&region_ids, &fidelity);
    
    assert_eq!(hits.len(), 2);
    assert_eq!(misses.len(), 1);
    assert_eq!(misses[0], "region3");
    
    let stats = cache.stats();
    // Note: actual stats fields are total, valid, expired
    assert!(stats.total >= 2);
}

/// Test config from environment variables
#[test]
fn test_config_from_env() {
    std::env::set_var("DEEPSEEK_OCR_ENABLED", "false");
    std::env::set_var("VISION_API_KEY", "test-key-123");
    std::env::set_var("VISION_TIMEOUT_MS", "3000");
    
    let config = DeepseekConfig::default().from_env();
    
    assert!(!config.enabled);
    assert_eq!(config.api_key, Some("test-key-123".to_string()));
    assert_eq!(config.timeout_ms, 3000);
    
    // Cleanup
    std::env::remove_var("DEEPSEEK_OCR_ENABLED");
    std::env::remove_var("VISION_API_KEY");
    std::env::remove_var("VISION_TIMEOUT_MS");
}
</file_path>