//! Configuration for DeepSeek OCR integration

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// DeepSeek OCR client configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepseekConfig {
    /// Enable/disable OCR integration globally
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// DeepSeek service URL
    #[serde(default = "default_service_url")]
    pub service_url: String,

    /// API key (read from env VISION_API_KEY if not set)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Request timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Maximum regions per decode request
    #[serde(default = "default_max_regions")]
    pub max_regions_per_request: usize,

    /// Default fidelity level
    #[serde(default = "default_fidelity")]
    pub default_fidelity: String,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub decode_cache_ttl_secs: u64,

    /// Maximum cache size
    #[serde(default = "default_cache_size")]
    pub decode_cache_max_size: usize,

    /// Maximum concurrent decode requests
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_decodes: usize,

    /// Number of retry attempts
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: usize,

    /// Base backoff in milliseconds
    #[serde(default = "default_retry_backoff_ms")]
    pub retry_backoff_ms: u64,

    /// Circuit breaker failure threshold
    #[serde(default = "default_breaker_failures")]
    pub circuit_breaker_failures: usize,

    /// Circuit breaker reset timeout in seconds
    #[serde(default = "default_breaker_reset")]
    pub circuit_breaker_reset_secs: u64,

    /// Redact OCR text from logs
    ///
    /// Note: By design, decoded OCR text is never logged by the client.
    /// This flag is reserved for future use if logging is added.
    #[serde(default = "default_log_redact")]
    pub log_redact_text: bool,
}

// Default value functions
fn default_enabled() -> bool { true }
fn default_service_url() -> String { "http://localhost:8080".to_string() }
fn default_timeout_ms() -> u64 { 5000 }
fn default_max_regions() -> usize { 16 }
fn default_fidelity() -> String { "10x".to_string() }
fn default_cache_ttl() -> u64 { 600 }
fn default_cache_size() -> usize { 1000 }
fn default_max_concurrent() -> usize { 16 }
fn default_retry_attempts() -> usize { 2 }
fn default_retry_backoff_ms() -> u64 { 200 }
fn default_breaker_failures() -> usize { 5 }
fn default_breaker_reset() -> u64 { 30 }
fn default_log_redact() -> bool { true }

impl Default for DeepseekConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            service_url: default_service_url(),
            api_key: None,
            timeout_ms: default_timeout_ms(),
            max_regions_per_request: default_max_regions(),
            default_fidelity: default_fidelity(),
            decode_cache_ttl_secs: default_cache_ttl(),
            decode_cache_max_size: default_cache_size(),
            max_concurrent_decodes: default_max_concurrent(),
            retry_attempts: default_retry_attempts(),
            retry_backoff_ms: default_retry_backoff_ms(),
            circuit_breaker_failures: default_breaker_failures(),
            circuit_breaker_reset_secs: default_breaker_reset(),
            log_redact_text: default_log_redact(),
        }
    }
}

impl DeepseekConfig {
    /// Load configuration from environment variables
    pub fn from_env(mut self) -> Self {
        // Override with environment variables if present
        if let Ok(val) = std::env::var("DEEPSEEK_OCR_ENABLED") {
            self.enabled = val.to_lowercase() == "true" || val == "1";
        }

        if let Ok(val) = std::env::var("VISION_SERVICE_URL") {
            self.service_url = val;
        }

        if let Ok(val) = std::env::var("VISION_API_KEY") {
            self.api_key = Some(val);
        }

        if let Ok(val) = std::env::var("VISION_TIMEOUT_MS") {
            if let Ok(timeout) = val.parse() {
                self.timeout_ms = timeout;
            }
        }

        if let Ok(val) = std::env::var("VISION_MAX_REGIONS") {
            if let Ok(max) = val.parse() {
                self.max_regions_per_request = max;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_CACHE_TTL_SECS") {
            if let Ok(ttl) = val.parse() {
                self.decode_cache_ttl_secs = ttl;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_CACHE_SIZE") {
            if let Ok(size) = val.parse() {
                self.decode_cache_max_size = size;
            }
        }

        if let Ok(val) = std::env::var("VISION_MAX_CONCURRENT_DECODES") {
            if let Ok(max) = val.parse() {
                self.max_concurrent_decodes = max;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_MAX_CONCURRENT") {
            if let Ok(max) = val.parse() {
                self.max_concurrent_decodes = max;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_MAX_RETRIES") {
            if let Ok(retries) = val.parse() {
                self.retry_attempts = retries;
            }
        }

        if let Ok(val) = std::env::var("VISION_RETRY_BACKOFF_MS") {
            if let Ok(ms) = val.parse() {
                self.retry_backoff_ms = ms;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_RETRY_BACKOFF_MS") {
            if let Ok(ms) = val.parse() {
                self.retry_backoff_ms = ms;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_CIRCUIT_THRESHOLD") {
            if let Ok(threshold) = val.parse() {
                self.circuit_breaker_failures = threshold;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_CIRCUIT_COOLDOWN_SECS") {
            if let Ok(secs) = val.parse() {
                self.circuit_breaker_reset_secs = secs;
            }
        }

        if let Ok(val) = std::env::var("DEEPSEEK_REDACT_API_KEY") {
            self.log_redact_text = val.to_lowercase() == "true" || val == "1";
        }

        self
    }

    /// Get timeout as Duration
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }

    /// Get cache TTL as Duration
    pub fn cache_ttl(&self) -> Duration {
        Duration::from_secs(self.decode_cache_ttl_secs)
    }

    /// Get circuit breaker reset timeout as Duration
    pub fn breaker_reset_timeout(&self) -> Duration {
        Duration::from_secs(self.circuit_breaker_reset_secs)
    }

    /// Get retry backoff as Duration
    pub fn retry_backoff(&self) -> Duration {
        Duration::from_millis(self.retry_backoff_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DeepseekConfig::default();
        assert!(config.enabled);
        assert_eq!(config.service_url, "http://localhost:8080");
        assert_eq!(config.timeout_ms, 5000);
        assert_eq!(config.max_regions_per_request, 16);
        assert!(config.log_redact_text);
    }

    #[test]
    fn test_config_from_env() {
        std::env::set_var("DEEPSEEK_OCR_ENABLED", "false");
        std::env::set_var("VISION_SERVICE_URL", "http://custom:9000");
        std::env::set_var("VISION_API_KEY", "test-key");

        let config = DeepseekConfig::default().from_env();
        
        assert!(!config.enabled);
        assert_eq!(config.service_url, "http://custom:9000");
        assert_eq!(config.api_key, Some("test-key".to_string()));

        // Cleanup
        std::env::remove_var("DEEPSEEK_OCR_ENABLED");
        std::env::remove_var("VISION_SERVICE_URL");
        std::env::remove_var("VISION_API_KEY");
    }

    #[test]
    fn test_duration_conversions() {
        let config = DeepseekConfig::default();
        assert_eq!(config.timeout(), Duration::from_millis(5000));
        assert_eq!(config.cache_ttl(), Duration::from_secs(600));
        assert_eq!(config.breaker_reset_timeout(), Duration::from_secs(30));
        assert_eq!(config.retry_backoff(), Duration::from_millis(200));
    }
}