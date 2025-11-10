//! Metrics collection for observability

use prometheus::{
    Counter, CounterVec, Histogram, HistogramVec, Opts, Registry,
    register_counter_vec_with_registry, register_histogram_vec_with_registry,
    register_counter_with_registry, register_histogram_with_registry,
};
use std::sync::Arc;
use once_cell::sync::Lazy;

/// Global metrics registry
pub static METRICS: Lazy<Arc<Metrics>> = Lazy::new(|| {
    Arc::new(Metrics::new().expect("Failed to initialize metrics"))
});

/// Metrics collector
pub struct Metrics {
    registry: Registry,
    
    // Vision API metrics
    pub vision_search_requests: CounterVec,
    pub vision_decode_requests: CounterVec,
    pub vision_index_requests: CounterVec,
    pub vision_request_duration: HistogramVec,
    
    // Facts API metrics
    pub facts_insert_requests: CounterVec,
    pub facts_query_requests: CounterVec,
    pub facts_duplicates: Counter,
    pub facts_request_duration: HistogramVec,
    
    // Token budget metrics
    pub token_budget_used: Histogram,
    pub token_budget_remaining: Histogram,
    pub token_budget_overflows: Counter,
    pub token_budget_summarizations: Counter,
    
    // Rate limiting metrics
    pub rate_limit_hits: CounterVec,
    pub rate_limit_allowed: CounterVec,
    
    // Context management metrics
    pub context_retrievals: Counter,
    pub context_storage: Counter,
}

impl Metrics {
    /// Create a new metrics collector
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let registry = Registry::new();
        
        // Vision API metrics
        let vision_search_requests = register_counter_vec_with_registry!(
            Opts::new("vision_search_requests_total", "Total vision search requests"),
            &["status"],
            registry
        )?;
        
        let vision_decode_requests = register_counter_vec_with_registry!(
            Opts::new("vision_decode_requests_total", "Total vision decode requests"),
            &["status"],
            registry
        )?;
        
        let vision_index_requests = register_counter_vec_with_registry!(
            Opts::new("vision_index_requests_total", "Total vision index requests"),
            &["status"],
            registry
        )?;
        
        let vision_request_duration = register_histogram_vec_with_registry!(
            "vision_request_duration_seconds",
            "Vision API request duration in seconds",
            &["endpoint"],
            registry
        )?;
        
        // Facts API metrics
        let facts_insert_requests = register_counter_vec_with_registry!(
            Opts::new("facts_insert_requests_total", "Total facts insert requests"),
            &["status"],
            registry
        )?;
        
        let facts_query_requests = register_counter_vec_with_registry!(
            Opts::new("facts_query_requests_total", "Total facts query requests"),
            &["status"],
            registry
        )?;
        
        let facts_duplicates = register_counter_with_registry!(
            Opts::new("facts_duplicates_total", "Total duplicate facts detected"),
            registry
        )?;
        
        let facts_request_duration = register_histogram_vec_with_registry!(
            "facts_request_duration_seconds",
            "Facts API request duration in seconds",
            &["endpoint"],
            registry
        )?;
        
        // Token budget metrics
        let token_budget_used = register_histogram_with_registry!(
            "token_budget_used",
            "Tokens used per request",
            registry
        )?;
        
        let token_budget_remaining = register_histogram_with_registry!(
            "token_budget_remaining",
            "Tokens remaining per request",
            registry
        )?;
        
        let token_budget_overflows = register_counter_with_registry!(
            Opts::new("token_budget_overflows_total", "Total token budget overflows"),
            registry
        )?;
        
        let token_budget_summarizations = register_counter_with_registry!(
            Opts::new("token_budget_summarizations_total", "Total summarizations performed"),
            registry
        )?;
        
        // Rate limiting metrics
        let rate_limit_hits = register_counter_vec_with_registry!(
            Opts::new("rate_limit_hits_total", "Total rate limit hits"),
            &["client_id"],
            registry
        )?;
        
        let rate_limit_allowed = register_counter_vec_with_registry!(
            Opts::new("rate_limit_allowed_total", "Total rate limit allowed requests"),
            &["client_id"],
            registry
        )?;
        
        // Context management metrics
        let context_retrievals = register_counter_with_registry!(
            Opts::new("context_retrievals_total", "Total context retrievals"),
            registry
        )?;
        
        let context_storage = register_counter_with_registry!(
            Opts::new("context_storage_total", "Total context storage operations"),
            registry
        )?;
        
        Ok(Self {
            registry,
            vision_search_requests,
            vision_decode_requests,
            vision_index_requests,
            vision_request_duration,
            facts_insert_requests,
            facts_query_requests,
            facts_duplicates,
            facts_request_duration,
            token_budget_used,
            token_budget_remaining,
            token_budget_overflows,
            token_budget_summarizations,
            rate_limit_hits,
            rate_limit_allowed,
            context_retrievals,
            context_storage,
        })
    }
    
    /// Get the metrics registry for exporting
    pub fn registry(&self) -> &Registry {
        &self.registry
    }
    
    /// Record a vision search request
    pub fn record_vision_search(&self, success: bool) {
        let status = if success { "success" } else { "error" };
        self.vision_search_requests.with_label_values(&[status]).inc();
    }
    
    /// Record a vision decode request
    pub fn record_vision_decode(&self, success: bool) {
        let status = if success { "success" } else { "error" };
        self.vision_decode_requests.with_label_values(&[status]).inc();
    }
    
    /// Record a facts insert request
    pub fn record_facts_insert(&self, success: bool, duplicate: bool) {
        let status = if success { "success" } else { "error" };
        self.facts_insert_requests.with_label_values(&[status]).inc();
        if duplicate {
            self.facts_duplicates.inc();
        }
    }
    
    /// Record a facts query request
    pub fn record_facts_query(&self, success: bool) {
        let status = if success { "success" } else { "error" };
        self.facts_query_requests.with_label_values(&[status]).inc();
    }
    
    /// Record token budget usage
    pub fn record_token_budget(&self, used: usize, remaining: usize, overflow: bool) {
        self.token_budget_used.observe(used as f64);
        self.token_budget_remaining.observe(remaining as f64);
        if overflow {
            self.token_budget_overflows.inc();
        }
    }
    
    /// Record a summarization event
    pub fn record_summarization(&self) {
        self.token_budget_summarizations.inc();
    }
    
    /// Record rate limit event
    pub fn record_rate_limit(&self, client_id: &str, allowed: bool) {
        if allowed {
            self.rate_limit_allowed.with_label_values(&[client_id]).inc();
        } else {
            self.rate_limit_hits.with_label_values(&[client_id]).inc();
        }
    }
    
    /// Export metrics in Prometheus text format
    pub fn export_prometheus(&self) -> String {
        use prometheus::Encoder;
        
        let encoder = prometheus::TextEncoder::new();
        let metric_families = prometheus::gather();
        
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap_or_default();
        
        String::from_utf8(buffer).unwrap_or_default()
    }
}

/// Helper macro to time operations
#[macro_export]
macro_rules! time_operation {
    ($histogram:expr, $label:expr, $operation:expr) => {{
        let timer = $histogram.with_label_values(&[$label]).start_timer();
        let result = $operation;
        timer.observe_duration();
        result
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        let metrics = Metrics::new();
        assert!(metrics.is_ok());
    }

    #[test]
    fn test_record_vision_search() {
        let metrics = Metrics::new().unwrap();
        metrics.record_vision_search(true);
        metrics.record_vision_search(false);
        // Metrics should be recorded without panicking
    }

    #[test]
    fn test_record_token_budget() {
        let metrics = Metrics::new().unwrap();
        metrics.record_token_budget(5000, 3000, false);
        metrics.record_token_budget(8100, 0, true);
        // Metrics should be recorded without panicking
    }
}