//! DeepSeek OCR client with retry, caching, and circuit breaker

use super::cache::DecodeCache;
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig};
use super::deepseek_config::DeepseekConfig;
use super::models::*;
use crate::metrics::METRICS;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// DeepSeek OCR error types
#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("OCR integration is disabled")]
    Disabled,

    #[error("Circuit breaker is open: {0}")]
    CircuitOpen(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Upstream error: {0}")]
    UpstreamError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// DeepSeek OCR client
pub struct DeepseekOcrClient {
    http: Client,
    config: DeepseekConfig,
    cache: Arc<DecodeCache>,
    semaphore: Arc<Semaphore>,
    breaker: Arc<CircuitBreaker>,
}

impl DeepseekOcrClient {
    /// Create a new DeepSeek OCR client
    pub fn new(config: DeepseekConfig) -> Result<Self, OcrError> {
        let http = Client::builder()
            .timeout(config.timeout())
            .build()
            .map_err(|e| OcrError::RequestFailed(e.to_string()))?;

        let cache = Arc::new(DecodeCache::new(
            config.cache_ttl(),
            config.decode_cache_max_size,
        ));

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_decodes));

        let breaker_config = CircuitBreakerConfig {
            failure_threshold: config.circuit_breaker_failures,
            reset_timeout: config.breaker_reset_timeout(),
        };
        let breaker = Arc::new(CircuitBreaker::new(breaker_config));

        Ok(Self {
            http,
            config,
            cache,
            semaphore,
            breaker,
        })
    }

    /// Decode regions to text
    pub async fn decode_regions(
        &self,
        region_ids: Vec<String>,
        fidelity: FidelityLevel,
    ) -> Result<Vec<DecodeResult>, OcrError> {
        let start = Instant::now();

        // Check if OCR is enabled
        if !self.config.enabled {
            METRICS.deepseek_requests
                .with_label_values(&["decode", "disabled"])
                .inc();
            return Err(OcrError::Disabled);
        }

        // Check cache
        let (hits, misses) = self.cache.split_hits(&region_ids, &fidelity);
        METRICS.deepseek_cache_hits.inc_by(hits.len() as f64);
        METRICS.deepseek_cache_misses.inc_by(misses.len() as f64);

        debug!(
            "Cache: {} hits, {} misses for {} regions",
            hits.len(),
            misses.len(),
            region_ids.len()
        );

        // If all cached, return immediately
        if misses.is_empty() {
            METRICS.deepseek_request_duration
                .with_label_values(&["decode"])
                .observe(start.elapsed().as_secs_f64());
            return Ok(hits);
        }

        // Check circuit breaker
        if self.breaker.is_open("decode") {
            METRICS.deepseek_circuit_open.with_label_values(&["decode"]).inc();
            error!("Circuit breaker is open for decode operation");
            return Err(OcrError::CircuitOpen("decode".to_string()));
        }

        // Acquire semaphore for concurrency control
        let _permit = self.semaphore.acquire().await.unwrap();

        // Retry with exponential backoff
        let mut attempt = 0;
        let decoded = loop {
            attempt += 1;

            match self.call_decode_api(&misses, &fidelity).await {
                Ok(results) => {
                    self.breaker.mark_success("decode");
                    METRICS.deepseek_requests
                        .with_label_values(&["decode", "success"])
                        .inc();
                    break results;
                }
                Err(e) => {
                    self.breaker.mark_failure("decode");
                    METRICS.deepseek_requests
                        .with_label_values(&["decode", "error"])
                        .inc();

                    if attempt > self.config.retry_attempts {
                        error!("Decode failed after {} attempts: {}", attempt, e);
                        return Err(e);
                    }

                    let backoff = self.calculate_backoff(attempt);
                    warn!(
                        "Decode attempt {} failed: {}, retrying in {:?}",
                        attempt, e, backoff
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        };

        // Store in cache
        self.cache.store_batch(&decoded, &fidelity);

        // Combine hits and newly decoded
        let mut results = hits;
        results.extend(decoded);

        METRICS.deepseek_request_duration
            .with_label_values(&["decode"])
            .observe(start.elapsed().as_secs_f64());

        Ok(results)
    }

    /// Call the DeepSeek decode API
    async fn call_decode_api(
        &self,
        region_ids: &[String],
        fidelity: &FidelityLevel,
    ) -> Result<Vec<DecodeResult>, OcrError> {
        let url = format!("{}/v1/ocr/decode", self.config.service_url);

        let request_body = serde_json::json!({
            "region_ids": region_ids,
            "fidelity": fidelity.as_str()
        });

        debug!("Calling DeepSeek decode API: {} regions", region_ids.len());

        let mut req = self.http.post(&url).json(&request_body);

        // Add bearer auth if API key is configured
        if let Some(api_key) = &self.config.api_key {
            req = req.bearer_auth(api_key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    OcrError::Timeout(e.to_string())
                } else {
                    OcrError::RequestFailed(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(OcrError::UpstreamError(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        let decode_response: DecodeResponse = response
            .json()
            .await
            .map_err(|e| OcrError::InvalidResponse(e.to_string()))?;

        Ok(decode_response.results)
    }

    /// Index a document
    pub async fn index_document(
        &self,
        doc_url: String,
        metadata: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<IndexResponse, OcrError> {
        let start = Instant::now();

        if !self.config.enabled {
            METRICS.deepseek_requests
                .with_label_values(&["index", "disabled"])
                .inc();
            return Err(OcrError::Disabled);
        }

        if self.breaker.is_open("index") {
            METRICS.deepseek_circuit_open.with_label_values(&["index"]).inc();
            return Err(OcrError::CircuitOpen("index".to_string()));
        }

        let url = format!("{}/v1/ocr/index", self.config.service_url);

        let request_body = serde_json::json!({
            "doc_url": doc_url,
            "metadata": metadata.unwrap_or_default()
        });

        let mut req = self.http.post(&url).json(&request_body);

        if let Some(api_key) = &self.config.api_key {
            req = req.bearer_auth(api_key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| {
                self.breaker.mark_failure("index");
                METRICS.deepseek_requests
                    .with_label_values(&["index", "error"])
                    .inc();
                if e.is_timeout() {
                    OcrError::Timeout(e.to_string())
                } else {
                    OcrError::RequestFailed(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            self.breaker.mark_failure("index");
            METRICS.deepseek_requests
                .with_label_values(&["index", "error"])
                .inc();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(OcrError::UpstreamError(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        let index_response: IndexResponse = response
            .json()
            .await
            .map_err(|e| OcrError::InvalidResponse(e.to_string()))?;

        self.breaker.mark_success("index");
        METRICS.deepseek_requests
            .with_label_values(&["index", "success"])
            .inc();
        METRICS.deepseek_request_duration
            .with_label_values(&["index"])
            .observe(start.elapsed().as_secs_f64());

        Ok(index_response)
    }

    /// Get job status
    pub async fn get_job_status(&self, job_id: String) -> Result<JobStatusResponse, OcrError> {
        let start = Instant::now();

        if !self.config.enabled {
            METRICS.deepseek_requests
                .with_label_values(&["status", "disabled"])
                .inc();
            return Err(OcrError::Disabled);
        }

        let url = format!("{}/v1/ocr/jobs/{}", self.config.service_url, job_id);

        let mut req = self.http.get(&url);

        if let Some(api_key) = &self.config.api_key {
            req = req.bearer_auth(api_key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| {
                METRICS.deepseek_requests
                    .with_label_values(&["status", "error"])
                    .inc();
                if e.is_timeout() {
                    OcrError::Timeout(e.to_string())
                } else {
                    OcrError::RequestFailed(e.to_string())
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            METRICS.deepseek_requests
                .with_label_values(&["status", "error"])
                .inc();
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(OcrError::UpstreamError(format!(
                "Status {}: {}",
                status, error_text
            )));
        }

        let job_response: JobStatusResponse = response
            .json()
            .await
            .map_err(|e| OcrError::InvalidResponse(e.to_string()))?;

        METRICS.deepseek_requests
            .with_label_values(&["status", "success"])
            .inc();
        METRICS.deepseek_request_duration
            .with_label_values(&["status"])
            .observe(start.elapsed().as_secs_f64());

        Ok(job_response)
    }

    /// Calculate exponential backoff
    fn calculate_backoff(&self, attempt: usize) -> Duration {
        let base = self.config.retry_backoff();
        let multiplier = 2_u32.pow((attempt - 1) as u32);
        base.saturating_mul(multiplier)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> super::cache::CacheStats {
        self.cache.stats()
    }

    /// Get circuit breaker statistics
    pub fn breaker_stats(&self, operation: &str) -> super::circuit_breaker::BreakerStats {
        self.breaker.stats(operation)
    }
}

// Response types for DeepSeek API
#[derive(Debug, Serialize, Deserialize)]
struct DecodeResponse {
    results: Vec<DecodeResult>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff() {
        let config = DeepseekConfig::default();
        let client = DeepseekOcrClient::new(config).unwrap();

        assert_eq!(client.calculate_backoff(1), Duration::from_millis(200));
        assert_eq!(client.calculate_backoff(2), Duration::from_millis(400));
        assert_eq!(client.calculate_backoff(3), Duration::from_millis(800));
    }

    #[test]
    fn test_disabled_client() {
        let mut config = DeepseekConfig::default();
        config.enabled = false;

        let client = DeepseekOcrClient::new(config).unwrap();
        
        // Decode should return Disabled error
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on(client.decode_regions(
            vec!["region1".to_string()],
            FidelityLevel::Medium,
        ));

        assert!(matches!(result, Err(OcrError::Disabled)));
    }
}