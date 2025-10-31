# Configuration Management and Deployment

## Overview

Rust-HiRAG implements a comprehensive configuration management system with support for multiple configuration sources, environment variable overrides, and validation. The deployment strategies cover various environments from development to production with Docker, Kubernetes, and cloud-native deployments.

## Configuration Management

### Configuration Architecture

The configuration system uses a hierarchical approach with multiple sources and validation layers.

#### Configuration Structure

```rust
use serde::{Deserialize, Serialize};
use secrecy::{Secret, ExposeSecret};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub embedding: EmbeddingConfig,
    pub vector_db: VectorDbConfig,
    pub hirag: HiRAGConfig,
    pub protocol: ProtocolConfig,
    pub logging: LoggingConfig,
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub rate_limit: RateLimitConfig,
    pub metrics: MetricsConfig,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = loader::load_config(path)?;
        validation::validate_config(&config)?;
        Ok(config)
    }
    
    pub fn from_file_with_env<P: AsRef<Path>>(path: P) -> Result<Self> {
        let config = loader::load_config_with_env(path)?;
        validation::validate_config(&config)?;
        Ok(config)
    }
    
    pub fn from_env() -> Result<Self> {
        let config = loader::load_from_env()?;
        validation::validate_config(&config)?;
        Ok(config)
    }
    
    pub fn validate(&self) -> Result<()> {
        validation::validate_config(self)
    }
    
    pub fn merge_with_env(mut self) -> Result<Self> {
        // Override with environment variables
        if let Ok(api_url) = std::env::var("CHUTES_API_URL") {
            self.embedding.api_url = api_url;
        }
        
        if let Ok(api_token) = std::env::var("CHUTES_API_TOKEN") {
            self.embedding.api_token = Secret::new(api_token);
        }
        
        if let Ok(qdrant_url) = std::env::var("QDRANT_URL") {
            self.vector_db.url = qdrant_url;
        }
        
        if let Ok(log_level) = std::env::var("LOG_LEVEL") {
            self.logging.level = log_level;
        }
        
        if let Ok(port) = std::env::var("PORT") {
            self.server.port = port.parse()
                .map_err(|_| ConfigError::InvalidPort(port))?;
        }
        
        Ok(self)
    }
}
```

#### Configuration Loader

```rust
pub mod loader {
    use super::*;
    use std::path::Path;
    use config::{Config, File, Environment};
    
    pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
        let mut settings = Config::builder();
        
        // Load base configuration file
        settings = settings.add_source(File::from(path.as_ref()));
        
        // Build configuration
        let config = settings.build()
            .map_err(|e| ConfigError::LoadError(e.to_string()))?;
        
        config.try_deserialize()
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }
    
    pub fn load_config_with_env<P: AsRef<Path>>(path: P) -> Result<Config> {
        let mut settings = Config::builder();
        
        // Load base configuration file
        settings = settings.add_source(File::from(path.as_ref()));
        
        // Override with environment variables
        settings = settings.add_source(Environment::with_prefix("HIRAG"));
        
        // Build configuration
        let config = settings.build()
            .map_err(|e| ConfigError::LoadError(e.to_string()))?;
        
        config.try_deserialize()
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }
    
    pub fn load_from_env() -> Result<Config> {
        let mut settings = Config::builder();
        
        // Load from environment variables only
        settings = settings.add_source(Environment::with_prefix("HIRAG"));
        
        // Build configuration
        let config = settings.build()
            .map_err(|e| ConfigError::LoadError(e.to_string()))?;
        
        config.try_deserialize()
            .map_err(|e| ConfigError::ParseError(e.to_string()))
    }
    
    pub fn load_with_defaults() -> Config {
        Config::default_config()
    }
}
```

#### Configuration Validation

```rust
pub mod validation {
    use super::*;
    
    pub fn validate_config(config: &Config) -> Result<()> {
        validate_embedding_config(&config.embedding)?;
        validate_vector_db_config(&config.vector_db)?;
        validate_hirag_config(&config.hirag)?;
        validate_server_config(&config.server)?;
        validate_auth_config(&config.auth)?;
        validate_rate_limit_config(&config.rate_limit)?;
        
        Ok(())
    }
    
    fn validate_embedding_config(config: &EmbeddingConfig) -> Result<()> {
        if config.api_url.is_empty() {
            return Err(ConfigError::ValidationError("Embedding API URL cannot be empty".to_string()).into());
        }
        
        if !config.api_url.starts_with("http://") && !config.api_url.starts_with("https://") {
            return Err(ConfigError::ValidationError("Embedding API URL must start with http:// or https://".to_string()).into());
        }
        
        if config.api_token.expose_secret().is_empty() {
            return Err(ConfigError::ValidationError("Embedding API token cannot be empty".to_string()).into());
        }
        
        if config.batch_size == 0 {
            return Err(ConfigError::ValidationError("Embedding batch size must be greater than 0".to_string()).into());
        }
        
        if config.timeout_secs == 0 {
            return Err(ConfigError::ValidationError("Embedding timeout must be greater than 0".to_string()).into());
        }
        
        Ok(())
    }
    
    fn validate_vector_db_config(config: &VectorDbConfig) -> Result<()> {
        if config.url.is_empty() {
            return Err(ConfigError::ValidationError("Vector DB URL cannot be empty".to_string()).into());
        }
        
        if !config.url.starts_with("http://") && !config.url.starts_with("https://") {
            return Err(ConfigError::ValidationError("Vector DB URL must start with http:// or https://".to_string()).into());
        }
        
        if config.vector_size == 0 {
            return Err(ConfigError::ValidationError("Vector size must be greater than 0".to_string()).into());
        }
        
        if config.collection_prefix.is_empty() {
            return Err(ConfigError::ValidationError("Collection prefix cannot be empty".to_string()).into());
        }
        
        Ok(())
    }
    
    fn validate_hirag_config(config: &HiRAGConfig) -> Result<()> {
        if config.l1_size == 0 {
            return Err(ConfigError::ValidationError("L1 cache size must be greater than 0".to_string()).into());
        }
        
        if config.l2_size == 0 {
            return Err(ConfigError::ValidationError("L2 cache size must be greater than 0".to_string()).into());
        }
        
        if config.max_context_tokens == 0 {
            return Err(ConfigError::ValidationError("Max context tokens must be greater than 0".to_string()).into());
        }
        
        if config.relevance_threshold < 0.0 || config.relevance_threshold > 1.0 {
            return Err(ConfigError::ValidationError("Relevance threshold must be between 0.0 and 1.0".to_string()).into());
        }
        
        // Validate ranking weights sum to 1.0
        let weight_sum = config.ranking_weights.similarity_weight +
                        config.ranking_weights.recency_weight +
                        config.ranking_weights.level_weight +
                        config.ranking_weights.frequency_weight;
        
        if (weight_sum - 1.0).abs() > 0.01 {
            return Err(ConfigError::ValidationError("Ranking weights must sum to 1.0".to_string()).into());
        }
        
        Ok(())
    }
    
    fn validate_server_config(config: &ServerConfig) -> Result<()> {
        if config.port == 0 {
            return Err(ConfigError::ValidationError("Server port must be greater than 0".to_string()).into());
        }
        
        if config.port > 65535 {
            return Err(ConfigError::ValidationError("Server port must be less than 65536".to_string()).into());
        }
        
        if config.host.is_empty() {
            return Err(ConfigError::ValidationError("Server host cannot be empty".to_string()).into());
        }
        
        Ok(())
    }
    
    fn validate_auth_config(config: &AuthConfig) -> Result<()> {
        if config.enabled && config.tokens.is_empty() {
            return Err(ConfigError::ValidationError("Authentication tokens cannot be empty when auth is enabled".to_string()).into());
        }
        
        Ok(())
    }
    
    fn validate_rate_limit_config(config: &RateLimitConfig) -> Result<()> {
        if config.enabled && config.requests_per_window == 0 {
            return Err(ConfigError::ValidationError("Requests per window must be greater than 0 when rate limiting is enabled".to_string()).into());
        }
        
        if config.window_secs == 0 {
            return Err(ConfigError::ValidationError("Rate limit window must be greater than 0".to_string()).into());
        }
        
        Ok(())
    }
}
```

### Configuration Files

#### Default Configuration (config.toml)

```toml
# Rust-HiRAG Configuration File

[embedding]
api_url = "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings"
api_token = "your_api_token_here"
batch_size = 32
timeout_secs = 30
max_retries = 3
cache_enabled = true
cache_ttl_secs = 3600
cache_size = 1000
tls_enabled = true
tls_verify = true

[vector_db]
url = "http://localhost:6333"
api_key = ""
collection_prefix = "contexts"
vector_size = 1024
distance = "Cosine"
timeout_secs = 30
tls_enabled = false
tls_verify = true

[hirag]
l1_size = 10
l2_size = 100
l3_enabled = true
max_context_tokens = 4000
relevance_threshold = 0.7

[hirag.token_estimator]
type = "CharacterBased"
chars_per_token = 4.0

[hirag.retrieval_strategy]
l1_allocation = 0.3
l2_allocation = 0.4
l3_allocation = 0.3
min_contexts_per_level = 1

[hirag.ranking_weights]
similarity_weight = 0.5
recency_weight = 0.2
level_weight = 0.2
frequency_weight = 0.1

[hirag.background]
gc_enabled = false
gc_interval_secs = 300
l2_ttl_secs = 3600
l3_ttl_secs = 86400

[protocol]
version = "1.0.0"
codec = "json"
max_message_size_mb = 10

[logging]
level = "info"
format = "json"

[server]
port = 8080
host = "0.0.0.0"
max_body_size_mb = 10
enable_compression = true
request_timeout_secs = 30
shutdown_timeout_secs = 10

[auth]
enabled = false
tokens = []
header_name = "Authorization"
token_prefix = "Bearer"
strict_mode = true

[rate_limit]
enabled = true
requests_per_window = 100
window_secs = 60
burst_size = 10
cleanup_interval_secs = 300
max_clients = 10000

[metrics]
enabled = true
endpoint = "/metrics"
collection_interval_secs = 30
```

#### Environment-Specific Configurations

**Development (config.dev.toml)**:
```toml
[logging]
level = "debug"
format = "pretty"

[server]
port = 8081

[auth]
enabled = false

[rate_limit]
enabled = false

[vector_db]
url = "http://localhost:6333"
```

**Production (config.prod.toml)**:
```toml
[logging]
level = "info"
format = "json"

[server]
port = 8080
host = "0.0.0.0"

[auth]
enabled = true
tokens = ["${AUTH_TOKEN}"]

[rate_limit]
enabled = true
requests_per_window = 1000

[vector_db]
url = "${QDRANT_URL}"
api_key = "${QDRANT_API_KEY}"
tls_enabled = true
```

## Deployment Strategies

### Docker Deployment

#### Dockerfile

```dockerfile
# Multi-stage build for optimized production image
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy source code
COPY . .

# Build the application
RUN cargo build --release

# Production stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false hirag

# Copy binary
COPY --from=builder /app/target/release/context-manager /usr/local/bin/

# Copy configuration file
COPY config.toml /etc/context-manager/config.toml

# Create directories
RUN mkdir -p /var/lib/context-manager /var/log/context-manager && \
    chown -R hirag:hirag /var/lib/context-manager /var/log/context-manager

# Switch to non-root user
USER hirag

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Run the application
CMD ["context-manager", "--config", "/etc/context-manager/config.toml"]
```

#### Docker Compose

```yaml
version: '3.8'

services:
  context-manager:
    build:
      context: .
      dockerfile: Dockerfile
    ports:
      - "8080:8080"
    environment:
      - CHUTES_API_TOKEN=${CHUTES_API_TOKEN}
      - QDRANT_URL=http://qdrant:6333
      - LOG_LEVEL=info
    volumes:
      - ./config.toml:/etc/context-manager/config.toml:ro
      - ./logs:/var/log/context-manager
    depends_on:
      - qdrant
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s

  qdrant:
    image: qdrant/qdrant:latest
    ports:
      - "6333:6333"
      - "6334:6334"
    volumes:
      - qdrant_storage:/qdrant/storage
      - ./qdrant_config.yaml:/qdrant/config/production.yaml:ro
    environment:
      - QDRANT__SERVICE__HTTP_PORT=6333
      - QDRANT__SERVICE__GRPC_PORT=6334
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:6333/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml:ro
      - prometheus_data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
      - '--storage.tsdb.path=/prometheus'
      - '--web.console.libraries=/etc/prometheus/console_libraries'
      - '--web.console.templates=/etc/prometheus/consoles'
      - '--storage.tsdb.retention.time=200h'
      - '--web.enable-lifecycle'
    restart: unless-stopped

  grafana:
    image: grafana/grafana:latest
    ports:
      - "3000:3000"
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    volumes:
      - grafana_data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards:ro
      - ./grafana/datasources:/etc/grafana/provisioning/datasources:ro
    restart: unless-stopped

volumes:
  qdrant_storage:
  prometheus_data:
  grafana_data:
```

### Kubernetes Deployment

#### Deployment Manifest

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rust-hirag
  labels:
    app: rust-hirag
    version: v1
spec:
  replicas: 3
  selector:
    matchLabels:
      app: rust-hirag
  template:
    metadata:
      labels:
        app: rust-hirag
        version: v1
    spec:
      containers:
      - name: rust-hirag
        image: rust-hirag:latest
        ports:
        - containerPort: 8080
          name: http
        env:
        - name: CHUTES_API_TOKEN
          valueFrom:
            secretKeyRef:
              name: hirag-secrets
              key: chutes-api-token
        - name: QDRANT_URL
          value: "http://qdrant-service:6333"
        - name: LOG_LEVEL
          value: "info"
        - name: RUST_LOG
          value: "info"
        volumeMounts:
        - name: config
          mountPath: /etc/context-manager
          readOnly: true
        - name: logs
          mountPath: /var/log/context-manager
        resources:
          requests:
            memory: "512Mi"
            cpu: "250m"
          limits:
            memory: "1Gi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
          timeoutSeconds: 3
          failureThreshold: 3
        startupProbe:
          httpGet:
            path: /health/live
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 10
          timeoutSeconds: 5
          failureThreshold: 30
      volumes:
      - name: config
        configMap:
          name: hirag-config
      - name: logs
        emptyDir: {}
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
---
apiVersion: v1
kind: Service
metadata:
  name: rust-hirag-service
spec:
  selector:
    app: rust-hirag
  ports:
  - protocol: TCP
    port: 80
    targetPort: 8080
    name: http
  type: ClusterIP
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: hirag-config
data:
  config.toml: |
    [embedding]
    api_url = "https://chutes-intfloat-multilingual-e5-large.chutes.ai/v1/embeddings"
    batch_size = 32
    timeout_secs = 30
    cache_enabled = true
    cache_ttl_secs = 3600
    cache_size = 1000
    
    [vector_db]
    url = "http://qdrant-service:6333"
    collection_prefix = "contexts"
    vector_size = 1024
    distance = "Cosine"
    timeout_secs = 30
    
    [hirag]
    l1_size = 100
    l2_size = 1000
    max_context_tokens = 8000
    relevance_threshold = 0.7
    
    [logging]
    level = "info"
    format = "json"
    
    [server]
    port = 8080
    host = "0.0.0.0"
    
    [auth]
    enabled = true
    
    [rate_limit]
    enabled = true
    requests_per_window = 1000
---
apiVersion: v1
kind: Secret
metadata:
  name: hirag-secrets
type: Opaque
data:
  chutes-api-token: <base64-encoded-token>
---
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: rust-hirag-pdb
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: rust-hirag
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: rust-hirag-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: rust-hirag
  minReplicas: 3
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

#### Ingress Configuration

```yaml
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: rust-hirag-ingress
  annotations:
    kubernetes.io/ingress.class: nginx
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/rate-limit: "100"
    nginx.ingress.kubernetes.io/rate-limit-window: "1m"
spec:
  tls:
  - hosts:
    - hirag.example.com
    secretName: hirag-tls
  rules:
  - host: hirag.example.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: rust-hirag-service
            port:
              number: 80
```

### Cloud Deployment

#### AWS ECS Deployment

```json
{
  "family": "rust-hirag",
  "networkMode": "awsvpc",
  "requiresCompatibilities": ["FARGATE"],
  "cpu": "512",
  "memory": "1024",
  "executionRoleArn": "arn:aws:iam::account:role/ecsTaskExecutionRole",
  "taskRoleArn": "arn:aws:iam::account:role/ecsTaskRole",
  "containerDefinitions": [
    {
      "name": "rust-hirag",
      "image": "your-account.dkr.ecr.region.amazonaws.com/rust-hirag:latest",
      "portMappings": [
        {
          "containerPort": 8080,
          "protocol": "tcp"
        }
      ],
      "environment": [
        {
          "name": "QDRANT_URL",
          "value": "http://qdrant-service:6333"
        },
        {
          "name": "LOG_LEVEL",
          "value": "info"
        }
      ],
      "secrets": [
        {
          "name": "CHUTES_API_TOKEN",
          "valueFrom": "arn:aws:secretsmanager:region:account:secret:hirag/chutes-token"
        }
      ],
      "logConfiguration": {
        "logDriver": "awslogs",
        "options": {
          "awslogs-group": "/ecs/rust-hirag",
          "awslogs-region": "us-west-2",
          "awslogs-stream-prefix": "ecs"
        }
      },
      "healthCheck": {
        "command": ["CMD-SHELL", "curl -f http://localhost:8080/health || exit 1"],
        "interval": 30,
        "timeout": 5,
        "retries": 3,
        "startPeriod": 60
      }
    }
  ]
}
```

#### Google Cloud Run Deployment

```yaml
apiVersion: serving.knative.dev/v1
kind: Service
metadata:
  name: rust-hirag
  annotations:
    run.googleapis.com/ingress: all
    run.googleapis.com/execution-environment: gen2
spec:
  template:
    metadata:
      annotations:
        autoscaling.knative.dev/maxScale: "100"
        autoscaling.knative.dev/minScale: "1"
        run.googleapis.com/cpu-throttling: "false"
    spec:
      containerConcurrency: 100
      timeoutSeconds: 300
      containers:
      - image: gcr.io/project-id/rust-hirag:latest
        ports:
        - containerPort: 8080
        env:
        - name: QDRANT_URL
          value: "https://qdrant-service.example.com"
        - name: LOG_LEVEL
          value: "info"
        - name: CHUTES_API_TOKEN
          valueFrom:
            secretKeyRef:
              name: hirag-secrets
              key: chutes-api-token
        resources:
          limits:
            cpu: "1000m"
            memory: "1Gi"
          requests:
            cpu: "250m"
            memory: "512Mi"
        startupProbe:
          httpGet:
            path: /health/live
            port: 8080
          failureThreshold: 30
          periodSeconds: 10
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8080
          failureThreshold: 3
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8080
          failureThreshold: 3
          periodSeconds: 5
```

### Monitoring and Logging

#### Prometheus Configuration

```yaml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

rule_files:
  - "hirag_rules.yml"

scrape_configs:
  - job_name: 'rust-hirag'
    static_configs:
      - targets: ['rust-hirag-service:8080']
    metrics_path: '/metrics'
    scrape_interval: 30s

  - job_name: 'qdrant'
    static_configs:
      - targets: ['qdrant-service:6333']
    metrics_path: '/metrics'
    scrape_interval: 30s

alerting:
  alertmanagers:
    - static_configs:
        - targets:
          - alertmanager:9093
```

#### Alert Rules

```yaml
groups:
- name: hirag_alerts
  rules:
  - alert: HighErrorRate
    expr: rate(hirag_requests_error_total[5m]) > 0.05
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: "High error rate detected"
      description: "Error rate is {{ $value }} errors per second"

  - alert: HighResponseTime
    expr: histogram_quantile(0.95, rate(hirag_request_duration_seconds_bucket[5m])) > 1
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High response time detected"
      description: "95th percentile response time is {{ $value }} seconds"

  - alert: CircuitBreakerOpen
    expr: hirag_circuit_breaker_state == 2
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "Circuit breaker is open"
      description: "Circuit breaker for {{ $labels.name }} is open"

  - alert: LowCacheHitRate
    expr: hirag_cache_hit_rate < 0.5
    for: 10m
    labels:
      severity: info
    annotations:
      summary: "Low cache hit rate"
      description: "Cache hit rate is {{ $value }}"
```

This comprehensive configuration management and deployment guide provides the necessary tools and patterns to deploy Rust-HiRAG in various environments from development to production with proper monitoring, scaling, and reliability features.