# Observability Features

## Overview

Rust-HiRAG implements a comprehensive observability stack with metrics collection, health monitoring, and structured logging to provide deep insights into system performance, reliability, and operational status. The observability features are designed for production environments with Prometheus integration and real-time monitoring capabilities.

## Metrics Collection System

### Metrics Architecture

The metrics system provides detailed performance and operational metrics for all major components of the HiRAG system.

#### Core Metrics Collector

```rust
pub struct MetricsCollector {
    request_metrics: Arc<RequestMetrics>,
    cache_metrics: Arc<CacheMetrics>,
    vector_db_metrics: Arc<VectorDbMetrics>,
    embedding_metrics: Arc<EmbeddingMetrics>,
    system_metrics: Arc<SystemMetrics>,
    circuit_breaker_metrics: Arc<CircuitBreakerMetrics>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            request_metrics: Arc::new(RequestMetrics::new()),
            cache_metrics: Arc::new(CacheMetrics::new()),
            vector_db_metrics: Arc::new(VectorDbMetrics::new()),
            embedding_metrics: Arc::new(EmbeddingMetrics::new()),
            system_metrics: Arc::new(SystemMetrics::new()),
            circuit_breaker_metrics: Arc::new(CircuitBreakerMetrics::new()),
        }
    }
    
    pub fn record_request(&self, duration: Duration) {
        self.request_metrics.record_request(duration);
    }
    
    pub fn record_cache_hit(&self) {
        self.cache_metrics.record_hit();
    }
    
    pub fn record_cache_miss(&self) {
        self.cache_metrics.record_miss();
    }
    
    pub fn record_error(&self) {
        self.request_metrics.record_error();
    }
    
    pub fn get_metrics(&self) -> SystemMetrics {
        SystemMetrics {
            requests: self.request_metrics.get_stats(),
            cache: self.cache_metrics.get_stats(),
            vector_db: self.vector_db_metrics.get_stats(),
            embedding: self.embedding_metrics.get_stats(),
            circuit_breakers: self.circuit_breaker_metrics.get_stats(),
        }
    }
}
```

#### Request Metrics

```rust
pub struct RequestMetrics {
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    request_durations: Arc<RwLock<VecDeque<Duration>>>,
    active_requests: AtomicUsize,
    start_time: Instant,
}

impl RequestMetrics {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            request_durations: Arc::new(RwLock::new(VecDeque::with_capacity(10000))),
            active_requests: AtomicUsize::new(0),
            start_time: Instant::now(),
        }
    }
    
    pub fn record_request(&self, duration: Duration) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        
        // Store duration for percentile calculations
        let mut durations = self.request_durations.write().unwrap();
        durations.push_back(duration);
        
        // Keep only last 10,000 measurements
        if durations.len() > 10000 {
            durations.pop_front();
        }
    }
    
    pub fn record_success(&self) {
        self.successful_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_error(&self) {
        self.failed_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn increment_active(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn get_stats(&self) -> RequestStats {
        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_requests.load(Ordering::Relaxed);
        let failed = self.failed_requests.load(Ordering::Relaxed);
        let active = self.active_requests.load(Ordering::Relaxed);
        
        let durations = self.request_durations.read().unwrap();
        let (p50, p95, p99) = if !durations.is_empty() {
            let mut sorted: Vec<_> = durations.iter().map(|d| d.as_millis() as u64).collect();
            sorted.sort_unstable();
            
            let len = sorted.len();
            let p50 = sorted[len * 50 / 100];
            let p95 = sorted[len * 95 / 100];
            let p99 = sorted[len * 99 / 100];
            (p50, p95, p99)
        } else {
            (0, 0, 0)
        };
        
        RequestStats {
            total_requests: total,
            successful_requests: successful,
            failed_requests: failed,
            active_requests: active,
            success_rate: if total > 0 { successful as f64 / total as f64 } else { 0.0 },
            error_rate: if total > 0 { failed as f64 / total as f64 } else { 0.0 },
            avg_response_time_ms: if total > 0 {
                let total_ms: u64 = durations.iter().map(|d| d.as_millis() as u64).sum();
                total_ms / total as u64
            } else { 0 },
            p50_response_time_ms: p50,
            p95_response_time_ms: p95,
            p99_response_time_ms: p99,
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestStats {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub active_requests: usize,
    pub success_rate: f64,
    pub error_rate: f64,
    pub avg_response_time_ms: u64,
    pub p50_response_time_ms: u64,
    pub p95_response_time_ms: u64,
    pub p99_response_time_ms: u64,
    pub uptime_seconds: u64,
}
```

#### Cache Metrics

```rust
pub struct CacheMetrics {
    l1_hits: AtomicU64,
    l1_misses: AtomicU64,
    embedding_hits: AtomicU64,
    embedding_misses: AtomicU64,
    l1_size: AtomicUsize,
    l1_max_size: AtomicUsize,
    embedding_cache_size: AtomicUsize,
    embedding_cache_max_size: AtomicUsize,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self {
            l1_hits: AtomicU64::new(0),
            l1_misses: AtomicU64::new(0),
            embedding_hits: AtomicU64::new(0),
            embedding_misses: AtomicU64::new(0),
            l1_size: AtomicUsize::new(0),
            l1_max_size: AtomicUsize::new(0),
            embedding_cache_size: AtomicUsize::new(0),
            embedding_cache_max_size: AtomicUsize::new(0),
        }
    }
    
    pub fn record_l1_hit(&self) {
        self.l1_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_l1_miss(&self) {
        self.l1_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_embedding_hit(&self) {
        self.embedding_hits.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_embedding_miss(&self) {
        self.embedding_misses.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn update_l1_size(&self, size: usize) {
        self.l1_size.store(size, Ordering::Relaxed);
    }
    
    pub fn set_l1_max_size(&self, max_size: usize) {
        self.l1_max_size.store(max_size, Ordering::Relaxed);
    }
    
    pub fn get_stats(&self) -> CacheStats {
        let l1_hits = self.l1_hits.load(Ordering::Relaxed);
        let l1_misses = self.l1_misses.load(Ordering::Relaxed);
        let embedding_hits = self.embedding_hits.load(Ordering::Relaxed);
        let embedding_misses = self.embedding_misses.load(Ordering::Relaxed);
        
        CacheStats {
            l1_hit_rate: if l1_hits + l1_misses > 0 {
                l1_hits as f64 / (l1_hits + l1_misses) as f64
            } else { 0.0 },
            embedding_hit_rate: if embedding_hits + embedding_misses > 0 {
                embedding_hits as f64 / (embedding_hits + embedding_misses) as f64
            } else { 0.0 },
            l1_size: self.l1_size.load(Ordering::Relaxed),
            l1_max_size: self.l1_max_size.load(Ordering::Relaxed),
            embedding_cache_size: self.embedding_cache_size.load(Ordering::Relaxed),
            embedding_cache_max_size: self.embedding_cache_max_size.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub l1_hit_rate: f64,
    pub embedding_hit_rate: f64,
    pub l1_size: usize,
    pub l1_max_size: usize,
    pub embedding_cache_size: usize,
    pub embedding_cache_max_size: usize,
}
```

### Prometheus Integration

#### Metrics Exporter

```rust
impl MetricsCollector {
    pub fn export_prometheus(&self) -> String {
        let mut output = String::new();
        
        // Request metrics
        let request_stats = self.request_metrics.get_stats();
        output.push_str(&format!(
            "# HELP hirag_requests_total Total number of requests\n\
             # TYPE hirag_requests_total counter\n\
             hirag_requests_total {}\n\
             \n\
             # HELP hirag_requests_success_total Total number of successful requests\n\
             # TYPE hirag_requests_success_total counter\n\
             hirag_requests_success_total {}\n\
             \n\
             # HELP hirag_requests_error_total Total number of failed requests\n\
             # TYPE hirag_requests_error_total counter\n\
             hirag_requests_error_total {}\n\
             \n\
             # HELP hirag_request_duration_seconds Request duration in seconds\n\
             # TYPE hirag_request_duration_seconds histogram\n\
             hirag_request_duration_seconds_sum {}\n\
             hirag_request_duration_seconds_count {}\n\
             \n\
             # HELP hirag_active_requests Current number of active requests\n\
             # TYPE hirag_active_requests gauge\n\
             hirag_active_requests {}\n\
             \n",
            request_stats.total_requests,
            request_stats.successful_requests,
            request_stats.failed_requests,
            request_stats.avg_response_time_ms as f64 / 1000.0,
            request_stats.total_requests,
            request_stats.active_requests,
        ));
        
        // Cache metrics
        let cache_stats = self.cache_metrics.get_stats();
        output.push_str(&format!(
            "# HELP hirag_cache_hit_rate Cache hit rate (0-1)\n\
             # TYPE hirag_cache_hit_rate gauge\n\
             hirag_cache_hit_rate {}\n\
             \n\
             # HELP hirag_l1_cache_size Current L1 cache size\n\
             # TYPE hirag_l1_cache_size gauge\n\
             hirag_l1_cache_size {}\n\
             \n\
             # HELP hirag_l1_cache_max_size Maximum L1 cache size\n\
             # TYPE hirag_l1_cache_max_size gauge\n\
             hirag_l1_cache_max_size {}\n\
             \n",
            cache_stats.l1_hit_rate,
            cache_stats.l1_size,
            cache_stats.l1_max_size,
        ));
        
        // Vector DB metrics
        let vector_db_stats = self.vector_db_metrics.get_stats();
        output.push_str(&format!(
            "# HELP hirag_vector_db_operations_total Total vector database operations\n\
             # TYPE hirag_vector_db_operations_total counter\n\
             hirag_vector_db_operations_total {}\n\
             \n\
             # HELP hirag_vector_db_errors_total Total vector database errors\n\
             # TYPE hirag_vector_db_errors_total counter\n\
             hirag_vector_db_errors_total {}\n\
             \n",
            vector_db_stats.total_operations,
            vector_db_stats.total_errors,
        ));
        
        // Circuit breaker metrics
        let circuit_breaker_stats = self.circuit_breaker_metrics.get_stats();
        for (name, stats) in circuit_breaker_stats {
            output.push_str(&format!(
                "# HELP {}_state Circuit breaker state (0=closed, 1=half-open, 2=open)\n\
                 # TYPE {}_state gauge\n\
                 {}_state {}\n\
                 \n\
                 # HELP {}_calls_total Total calls through circuit breaker\n\
                 # TYPE {}_calls_total counter\n\
                 {}_calls_total {}\n\
                 \n",
                name, name, name, stats.state as u8,
                name, name, name, stats.total_calls,
            ));
        }
        
        output
    }
}
```

## Health Check System

### Health Monitoring Architecture

The health check system provides comprehensive monitoring of all system components with configurable thresholds and automatic status aggregation.

#### Health Checker Implementation

```rust
pub struct HealthChecker {
    components: Vec<Box<dyn HealthCheckable>>,
    config: HealthCheckConfig,
}

pub trait HealthCheckable: Send + Sync {
    async fn check_health(&self) -> ComponentHealth;
    fn name(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
    pub timeout_secs: u64,
    pub failure_threshold: usize,
    pub recovery_threshold: usize,
    pub check_interval_secs: u64,
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            failure_threshold: 3,
            recovery_threshold: 2,
            check_interval_secs: 60,
        }
    }
}

impl HealthChecker {
    pub fn new(config: HealthCheckConfig) -> Self {
        Self {
            components: Vec::new(),
            config,
        }
    }
    
    pub fn add_component(&mut self, component: Box<dyn HealthCheckable>) {
        self.components.push(component);
    }
    
    pub async fn check_health(&self) -> SystemHealth {
        let timeout = Duration::from_secs(self.config.timeout_secs);
        let mut component_healths = Vec::new();
        
        for component in &self.components {
            let health = tokio::time::timeout(timeout, component.check_health()).await;
            
            let health = match health {
                Ok(h) => h,
                Err(_) => ComponentHealth {
                    name: component.name().to_string(),
                    status: HealthStatus::Unhealthy,
                    message: Some("Health check timeout".to_string()),
                    last_check: Utc::now(),
                    response_time_ms: timeout.as_millis() as u64,
                },
            };
            
            component_healths.push(health);
        }
        
        let overall_status = self.aggregate_health(&component_healths);
        
        SystemHealth {
            status: overall_status,
            components: component_healths,
            timestamp: Utc::now(),
        }
    }
    
    fn aggregate_health(&self, components: &[ComponentHealth]) -> HealthStatus {
        if components.is_empty() {
            return HealthStatus::Healthy;
        }
        
        let unhealthy_count = components.iter()
            .filter(|c| matches!(c.status, HealthStatus::Unhealthy))
            .count();
        
        let degraded_count = components.iter()
            .filter(|c| matches!(c.status, HealthStatus::Degraded))
            .count();
        
        if unhealthy_count > 0 {
            HealthStatus::Unhealthy
        } else if degraded_count > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemHealth {
    pub status: HealthStatus,
    pub components: Vec<ComponentHealth>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ComponentHealth {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub response_time_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}
```

#### Component Health Checks

```rust
// L1 Cache Health Check
pub struct L1CacheHealthCheck {
    cache: Arc<DashMap<Uuid, Context>>,
    max_size: usize,
}

impl HealthCheckable for L1CacheHealthCheck {
    async fn check_health(&self) -> ComponentHealth {
        let start_time = Instant::now();
        
        let current_size = self.cache.len();
        let status = if current_size <= self.max_size {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };
        
        ComponentHealth {
            name: "L1 Cache".to_string(),
            status,
            message: Some(format!("Size: {}/{}", current_size, self.max_size)),
            last_check: Utc::now(),
            response_time_ms: start_time.elapsed().as_millis() as u64,
        }
    }
    
    fn name(&self) -> &str {
        "L1 Cache"
    }
}

// Vector Database Health Check
pub struct VectorDbHealthCheck {
    client: Arc<dyn VectorStore>,
}

impl HealthCheckable for VectorDbHealthCheck {
    async fn check_health(&self) -> ComponentHealth {
        let start_time = Instant::now();
        
        // Test basic connectivity with a simple operation
        match self.client.health_check().await {
            Ok(_) => ComponentHealth {
                name: "Vector Database".to_string(),
                status: HealthStatus::Healthy,
                message: Some("Connected and responsive".to_string()),
                last_check: Utc::now(),
                response_time_ms: start_time.elapsed().as_millis() as u64,
            },
            Err(e) => ComponentHealth {
                name: "Vector Database".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some(format!("Connection failed: {}", e)),
                last_check: Utc::now(),
                response_time_ms: start_time.elapsed().as_millis() as u64,
            },
        }
    }
    
    fn name(&self) -> &str {
        "Vector Database"
    }
}

// Embedding Service Health Check
pub struct EmbeddingServiceHealthCheck {
    client: Arc<dyn EmbeddingProvider>,
}

impl HealthCheckable for EmbeddingServiceHealthCheck {
    async fn check_health(&self) -> ComponentHealth {
        let start_time = Instant::now();
        
        // Test with a simple embedding request
        match self.client.embed_single("health check").await {
            Ok(embedding) => {
                let status = if embedding.len() == 1024 { // Expected dimension
                    HealthStatus::Healthy
                } else {
                    HealthStatus::Degraded
                };
                
                ComponentHealth {
                    name: "Embedding Service".to_string(),
                    status,
                    message: Some(format!("Embedding dimension: {}", embedding.len())),
                    last_check: Utc::now(),
                    response_time_ms: start_time.elapsed().as_millis() as u64,
                }
            }
            Err(e) => ComponentHealth {
                name: "Embedding Service".to_string(),
                status: HealthStatus::Unhealthy,
                message: Some(format!("API error: {}", e)),
                last_check: Utc::now(),
                response_time_ms: start_time.elapsed().as_millis() as u64,
            },
        }
    }
    
    fn name(&self) -> &str {
        "Embedding Service"
    }
}
```

### Health Check Endpoints

#### HTTP Health Endpoints

```rust
pub async fn health_check_handler(
    State(health_checker): State<Arc<HealthChecker>>,
) -> impl IntoResponse {
    let health = health_checker.check_health().await;
    
    let status_code = match health.status {
        HealthStatus::Healthy => StatusCode::OK,
        HealthStatus::Degraded => StatusCode::OK, // Still serving traffic
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };
    
    (status_code, Json(health))
}

pub async fn health_check_ready_handler(
    State(health_checker): State<Arc<HealthChecker>>,
) -> impl IntoResponse {
    let health = health_checker.check_health().await;
    
    let status_code = match health.status {
        HealthStatus::Healthy | HealthStatus::Degraded => StatusCode::OK,
        HealthStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
    };
    
    (status_code, Json(health))
}

pub async fn health_check_live_handler(
    State(health_checker): State<Arc<HealthChecker>>,
) -> impl IntoResponse {
    // Liveness probe - just check if the service is running
    let health = SystemHealth {
        status: HealthStatus::Healthy,
        components: vec![],
        timestamp: Utc::now(),
    };
    
    (StatusCode::OK, Json(health))
}
```

## Structured Logging

### Logging Configuration

```rust
pub fn init_observability(log_level: &str, format: &str) {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));
    
    match format {
        "json" => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true))
                .init();
        }
        _ => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_line_number(true))
                .init();
        }
    }
}
```

### Structured Logging Implementation

```rust
use tracing::{info, warn, error, debug, instrument};

impl HiRAGManagerV2 {
    #[instrument(skip(self), fields(context_id = %id, level = ?level))]
    pub async fn store_context(
        &self,
        text: &str,
        level: ContextLevel,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<Uuid> {
        let start_time = Instant::now();
        
        info!(
            text_length = text.len(),
            metadata_count = metadata.len(),
            "Storing context"
        );
        
        // ... implementation ...
        
        let duration = start_time.elapsed();
        info!(
            context_id = %id,
            duration_ms = duration.as_millis(),
            "Context stored successfully"
        );
        
        Ok(id)
    }
    
    #[instrument(skip(self), fields(query = %request.query, max_tokens = request.max_tokens))]
    pub async fn retrieve_context(&self, request: ContextRequest) -> Result<ContextResponse> {
        let start_time = Instant::now();
        
        info!(
            levels_count = request.levels.len(),
            has_filters = request.filters.is_some(),
            "Retrieving context"
        );
        
        // ... implementation ...
        
        let duration = start_time.elapsed();
        info!(
            contexts_found = response.contexts.len(),
            total_tokens = response.total_tokens,
            cache_hits = response.metadata.cache_hits,
            duration_ms = duration.as_millis(),
            "Context retrieved successfully"
        );
        
        Ok(response)
    }
}
```

## Performance Monitoring

### Performance Metrics Dashboard

```rust
pub struct PerformanceMonitor {
    metrics_collector: Arc<MetricsCollector>,
    health_checker: Arc<HealthChecker>,
}

impl PerformanceMonitor {
    pub async fn get_dashboard_data(&self) -> DashboardData {
        let metrics = self.metrics_collector.get_metrics();
        let health = self.health_checker.check_health().await;
        
        DashboardData {
            timestamp: Utc::now(),
            health,
            metrics,
            alerts: self.generate_alerts(&metrics, &health),
        }
    }
    
    fn generate_alerts(&self, metrics: &SystemMetrics, health: &SystemHealth) -> Vec<Alert> {
        let mut alerts = Vec::new();
        
        // Error rate alert
        if metrics.requests.error_rate > 0.05 { // 5% error rate
            alerts.push(Alert {
                level: AlertLevel::Warning,
                message: format!("High error rate: {:.2}%", metrics.requests.error_rate * 100.0),
                timestamp: Utc::now(),
            });
        }
        
        // Response time alert
        if metrics.requests.p95_response_time_ms > 1000 { // 1 second
            alerts.push(Alert {
                level: AlertLevel::Warning,
                message: format!("High P95 response time: {}ms", metrics.requests.p95_response_time_ms),
                timestamp: Utc::now(),
            });
        }
        
        // Cache hit rate alert
        if metrics.cache.l1_hit_rate < 0.5 { // 50% hit rate
            alerts.push(Alert {
                level: AlertLevel::Info,
                message: format!("Low L1 cache hit rate: {:.2}%", metrics.cache.l1_hit_rate * 100.0),
                timestamp: Utc::now(),
            });
        }
        
        // Health status alerts
        for component in &health.components {
            if matches!(component.status, HealthStatus::Unhealthy) {
                alerts.push(Alert {
                    level: AlertLevel::Critical,
                    message: format!("Component unhealthy: {}", component.name),
                    timestamp: Utc::now(),
                });
            }
        }
        
        alerts
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub health: SystemHealth,
    pub metrics: SystemMetrics,
    pub alerts: Vec<Alert>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Alert {
    pub level: AlertLevel,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertLevel {
    Info,
    Warning,
    Critical,
}
```

This comprehensive observability system provides deep insights into the Rust-HiRAG system's performance, health, and operational status, enabling effective monitoring and troubleshooting in production environments.