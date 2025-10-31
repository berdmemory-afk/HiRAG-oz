# API Layer and Communication Protocols

## Overview

Rust-HiRAG implements a comprehensive RESTful API layer with support for multiple communication protocols, including JSON and MessagePack serialization. The API layer provides clean, well-documented endpoints for all core functionality while maintaining high performance and security.

## API Architecture

### HTTP Server Implementation

The system uses Axum as the HTTP framework, providing high-performance async request handling with comprehensive middleware support.

#### Server Configuration

```rust
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_body_size_mb: usize,
    pub enable_compression: bool,
    pub request_timeout_secs: u64,
    pub shutdown_timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            max_body_size_mb: 10,
            enable_compression: true,
            request_timeout_secs: 30,
            shutdown_timeout_secs: 10,
        }
    }
}
```

#### Server Implementation

```rust
pub struct ContextManagerServer {
    config: ServerConfig,
    app: Router,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl ContextManagerServer {
    pub fn new(
        config: ServerConfig,
        hirag_manager: Arc<HiRAGManagerV2>,
        metrics: Arc<MetricsCollector>,
        health_checker: Arc<HealthChecker>,
    ) -> Self {
        let app = Self::create_router(hirag_manager, metrics, health_checker);
        
        Self {
            config,
            app,
            shutdown_tx: None,
        }
    }
    
    fn create_router(
        hirag_manager: Arc<HiRAGManagerV2>,
        metrics: Arc<MetricsCollector>,
        health_checker: Arc<HealthChecker>,
    ) -> Router {
        Router::new()
            // Context management routes
            .route("/contexts", post(store_context))
            .route("/contexts/:id", get(get_context))
            .route("/contexts/:id", put(update_context))
            .route("/contexts/:id", delete(delete_context))
            .route("/contexts/search", post(search_contexts))
            
            // Health check routes
            .route("/health", get(health_check))
            .route("/health/ready", get(health_ready))
            .route("/health/live", get(health_live))
            
            // Metrics routes
            .route("/metrics", get(prometheus_metrics))
            .route("/stats", get(system_stats))
            
            // Admin routes
            .route("/admin/cache/clear", post(clear_cache))
            .route("/admin/collections", list_collections)
            .route("/admin/collections/:name", delete(delete_collection))
            
            .with_state(AppState {
                hirag_manager,
                metrics,
                health_checker,
            })
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(CompressionLayer::new())
                    .layer(CorsLayer::permissive())
            )
    }
    
    pub async fn run(mut self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        
        info!("Starting server on {}", addr);
        
        let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        let graceful = axum::Server::from_tcp(listener)?
            .serve(self.app.into_make_service())
            .with_graceful_shutdown(async move {
                shutdown_rx.recv().await.ok();
                info!("Received shutdown signal");
            });
        
        graceful.await?;
        info!("Server shutdown complete");
        
        Ok(())
    }
    
    pub async fn shutdown(&self) -> Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }
        Ok(())
    }
}

#[derive(Clone)]
struct AppState {
    hirag_manager: Arc<HiRAGManagerV2>,
    metrics: Arc<MetricsCollector>,
    health_checker: Arc<HealthChecker>,
}
```

## REST API Endpoints

### Context Management Endpoints

#### Store Context

```rust
#[derive(Deserialize)]
pub struct StoreContextRequest {
    pub text: String,
    pub level: ContextLevel,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
}

#[derive(Serialize)]
pub struct StoreContextResponse {
    pub id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub token_count: usize,
}

pub async fn store_context(
    State(state): State<AppState>,
    Json(request): Json<StoreContextRequest>,
) -> Result<Json<StoreContextResponse>, ApiError> {
    let start_time = Instant::now();
    state.metrics.increment_active();
    
    let result = state.hirag_manager.store_context(
        &request.text,
        request.level,
        request.metadata,
    ).await;
    
    state.metrics.record_request(start_time.elapsed());
    
    match result {
        Ok(id) => {
            state.metrics.record_success();
            Ok(Json(StoreContextResponse {
                id,
                timestamp: Utc::now(),
                token_count: estimate_token_count(&request.text),
            }))
        }
        Err(e) => {
            state.metrics.record_error();
            Err(ApiError::from(e))
        }
    }
}
```

#### Search Contexts

```rust
#[derive(Deserialize)]
pub struct SearchContextsRequest {
    pub query: String,
    pub max_tokens: usize,
    #[serde(default)]
    pub levels: Vec<ContextLevel>,
    #[serde(default)]
    pub filters: Option<SearchFilter>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub priority: Priority,
}

#[derive(Deserialize)]
pub struct SearchFilter {
    pub level: Option<ContextLevel>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub time_range: Option<TimeRange>,
}

#[derive(Deserialize)]
pub struct TimeRange {
    pub start: Option<chrono::DateTime<chrono::Utc>>,
    pub end: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn search_contexts(
    State(state): State<AppState>,
    Json(request): Json<SearchContextsRequest>,
) -> Result<Json<ContextResponse>, ApiError> {
    let start_time = Instant::now();
    state.metrics.increment_active();
    
    let context_request = ContextRequest {
        query: request.query,
        max_tokens: request.max_tokens,
        levels: request.levels,
        filters: request.filters.map(|f| Filter {
            level: f.level,
            session_id: f.session_id,
            agent_id: f.agent_id,
            metadata_conditions: f.metadata.into_iter()
                .map(|(k, v)| MetadataCondition::Equals { key: k, value: v })
                .collect(),
            time_range: f.time_range,
        }),
        priority: request.priority,
        session_id: request.session_id,
    };
    
    let result = state.hirag_manager.retrieve_context(context_request).await;
    
    state.metrics.record_request(start_time.elapsed());
    
    match result {
        Ok(response) => {
            state.metrics.record_success();
            Ok(Json(response))
        }
        Err(e) => {
            state.metrics.record_error();
            Err(ApiError::from(e))
        }
    }
}
```

#### Get Context by ID

```rust
pub async fn get_context(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Context>, ApiError> {
    let start_time = Instant::now();
    state.metrics.increment_active();
    
    // Search across all levels for the context
    let levels = vec![
        ContextLevel::Immediate,
        ContextLevel::ShortTerm,
        ContextLevel::LongTerm,
    ];
    
    for level in levels {
        let collection_name = format!("contexts_{}", level.as_str().to_lowercase());
        if let Ok(Some(point)) = state.hirag_manager.vector_db.get_point(&collection_name, id).await {
            let context = Context {
                id: point.id,
                text: point.payload.text,
                level: point.payload.level,
                relevance_score: 1.0,
                token_count: estimate_token_count(&point.payload.text),
                timestamp: point.payload.timestamp,
                metadata: point.payload.metadata,
            };
            
            state.metrics.record_request(start_time.elapsed());
            state.metrics.record_success();
            return Ok(Json(context));
        }
    }
    
    state.metrics.record_request(start_time.elapsed());
    state.metrics.record_error();
    Err(ApiError::NotFound(format!("Context {} not found", id)))
}
```

#### Update Context

```rust
#[derive(Deserialize)]
pub struct UpdateContextRequest {
    pub metadata: HashMap<String, serde_json::Value>,
}

pub async fn update_context(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateContextRequest>,
) -> Result<StatusCode, ApiError> {
    let start_time = Instant::now();
    state.metrics.increment_active();
    
    let result = state.hirag_manager.update_context(id, request.metadata).await;
    
    state.metrics.record_request(start_time.elapsed());
    
    match result {
        Ok(()) => {
            state.metrics.record_success();
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            state.metrics.record_error();
            Err(ApiError::from(e))
        }
    }
}
```

#### Delete Context

```rust
pub async fn delete_context(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let start_time = Instant::now();
    state.metrics.increment_active();
    
    let result = state.hirag_manager.delete_context(id).await;
    
    state.metrics.record_request(start_time.elapsed());
    
    match result {
        Ok(()) => {
            state.metrics.record_success();
            Ok(StatusCode::NO_CONTENT)
        }
        Err(e) => {
            state.metrics.record_error();
            Err(ApiError::from(e))
        }
    }
}
```

### Health Check Endpoints

```rust
pub async fn health_check(
    State(state): State<AppState>,
) -> Json<SystemHealth> {
    state.health_checker.check_health().await
}

pub async fn health_ready(
    State(state): State<AppState>,
) -> Result<Json<SystemHealth>, StatusCode> {
    let health = state.health_checker.check_health().await;
    
    match health.status {
        HealthStatus::Healthy | HealthStatus::Degraded => Ok(Json(health)),
        HealthStatus::Unhealthy => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

pub async fn health_live(
    State(_state): State<AppState>,
) -> Json<SystemHealth> {
    Json(SystemHealth {
        status: HealthStatus::Healthy,
        components: vec![],
        timestamp: Utc::now(),
    })
}
```

### Metrics Endpoints

```rust
pub async fn prometheus_metrics(
    State(state): State<AppState>,
) -> String {
    state.metrics.export_prometheus()
}

pub async fn system_stats(
    State(state): State<AppState>,
) -> Json<SystemMetrics> {
    Json(state.metrics.get_metrics())
}
```

### Admin Endpoints

```rust
pub async fn clear_cache(
    State(state): State<AppState>,
) -> Result<Json<CacheClearResponse>, ApiError> {
    // Clear L1 cache
    state.hirag_manager.clear_l1_cache().await?;
    
    // Clear embedding cache
    state.hirag_manager.clear_embedding_cache().await?;
    
    Ok(Json(CacheClearResponse {
        message: "All caches cleared successfully".to_string(),
        timestamp: Utc::now(),
    }))
}

#[derive(Serialize)]
pub struct CacheClearResponse {
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

pub async fn list_collections(
    State(state): State<AppState>,
) -> Json<Vec<String>> {
    let collections = vec![
        "contexts_immediate".to_string(),
        "contexts_shortterm".to_string(),
        "contexts_longterm".to_string(),
    ];
    Json(collections)
}

pub async fn delete_collection(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let level = match name.as_str() {
        "contexts_immediate" => ContextLevel::Immediate,
        "contexts_shortterm" => ContextLevel::ShortTerm,
        "contexts_longterm" => ContextLevel::LongTerm,
        _ => return Err(ApiError::BadRequest("Invalid collection name".to_string())),
    };
    
    state.hirag_manager.clear_level(level).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

## Communication Protocols

### Message Protocol

The system implements a flexible message protocol supporting multiple serialization formats.

#### Message Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Message {
    #[serde(rename = "context.store")]
    StoreContext {
        id: String,
        text: String,
        level: ContextLevel,
        metadata: HashMap<String, serde_json::Value>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    
    #[serde(rename = "context.retrieve")]
    RetrieveContext {
        id: String,
        query: String,
        max_tokens: usize,
        levels: Vec<ContextLevel>,
        filters: Option<Filter>,
    },
    
    #[serde(rename = "context.response")]
    ContextResponse {
        id: String,
        contexts: Vec<Context>,
        total_tokens: usize,
        retrieval_time_ms: u64,
        metadata: ResponseMetadata,
    },
    
    #[serde(rename = "health.check")]
    HealthCheck {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    
    #[serde(rename = "health.response")]
    HealthResponse {
        status: HealthStatus,
        components: Vec<ComponentHealth>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    
    #[serde(rename = "error")]
    Error {
        code: String,
        message: String,
        details: Option<serde_json::Value>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}
```

#### Codec Implementation

```rust
#[async_trait]
pub trait Codec: Send + Sync {
    async fn encode(&self, message: &Message) -> Result<Vec<u8>>;
    async fn decode(&self, data: &[u8]) -> Result<Message>;
}

pub struct JsonCodec;

#[async_trait]
impl Codec for JsonCodec {
    async fn encode(&self, message: &Message) -> Result<Vec<u8>> {
        serde_json::to_vec(message).map_err(|e| CodecError::EncodeError(e.to_string()).into())
    }
    
    async fn decode(&self, data: &[u8]) -> Result<Message> {
        serde_json::from_slice(data).map_err(|e| CodecError::DecodeError(e.to_string()).into())
    }
}

pub struct MessagePackCodec;

#[async_trait]
impl Codec for MessagePackCodec {
    async fn encode(&self, message: &Message) -> Result<Vec<u8>> {
        rmp_serde::to_vec(message).map_err(|e| CodecError::EncodeError(e.to_string()).into())
    }
    
    async fn decode(&self, data: &[u8]) -> Result<Message> {
        rmp_serde::from_slice(data).map_err(|e| CodecError::DecodeError(e.to_string()).into())
    }
}
```

#### Message Handler

```rust
pub struct MessageHandler {
    hirag_manager: Arc<HiRAGManagerV2>,
    codec: Box<dyn Codec>,
}

impl MessageHandler {
    pub fn new(hirag_manager: Arc<HiRAGManagerV2>, codec: Box<dyn Codec>) -> Self {
        Self {
            hirag_manager,
            codec,
        }
    }
    
    pub async fn handle_message(&self, data: &[u8]) -> Result<Vec<u8>> {
        let message = self.codec.decode(data).await?;
        
        let response = match message {
            Message::StoreContext { text, level, metadata, .. } => {
                match self.hirag_manager.store_context(&text, level, metadata).await {
                    Ok(id) => Message::StoreContext {
                        id: id.to_string(),
                        text,
                        level,
                        metadata: HashMap::new(),
                        timestamp: Utc::now(),
                    },
                    Err(e) => Message::Error {
                        code: "STORE_ERROR".to_string(),
                        message: e.to_string(),
                        details: None,
                        timestamp: Utc::now(),
                    },
                }
            }
            
            Message::RetrieveContext { query, max_tokens, levels, filters, .. } => {
                let request = ContextRequest {
                    query,
                    max_tokens,
                    levels,
                    filters,
                    priority: Priority::Normal,
                    session_id: None,
                };
                
                match self.hirag_manager.retrieve_context(request).await {
                    Ok(response) => Message::ContextResponse {
                        id: uuid::Uuid::new_v4().to_string(),
                        contexts: response.contexts,
                        total_tokens: response.total_tokens,
                        retrieval_time_ms: response.retrieval_time_ms,
                        metadata: response.metadata,
                    },
                    Err(e) => Message::Error {
                        code: "RETRIEVE_ERROR".to_string(),
                        message: e.to_string(),
                        details: None,
                        timestamp: Utc::now(),
                    },
                }
            }
            
            Message::HealthCheck { .. } => {
                Message::HealthResponse {
                    status: HealthStatus::Healthy,
                    components: vec![],
                    timestamp: Utc::now(),
                }
            }
            
            _ => Message::Error {
                code: "UNKNOWN_MESSAGE".to_string(),
                message: "Unknown message type".to_string(),
                details: None,
                timestamp: Utc::now(),
            },
        };
        
        self.codec.encode(&response).await
    }
}
```

## Error Handling

### API Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
    
    #[error("Forbidden: {0}")]
    Forbidden(String),
    
    #[error("Conflict: {0}")]
    Conflict(String),
    
    #[error("Too many requests: {0}")]
    TooManyRequests(String),
    
    #[error("Internal server error: {0}")]
    InternalServerError(String),
    
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Circuit breaker open")]
    CircuitBreakerOpen,
}

impl ApiError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden(_) => StatusCode::FORBIDDEN,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            ApiError::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            ApiError::ValidationError(_) => StatusCode::BAD_REQUEST,
            ApiError::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            ApiError::CircuitBreakerOpen => StatusCode::SERVICE_UNAVAILABLE,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_response = ErrorResponse {
            error: self.to_string(),
            code: status.as_u16(),
            timestamp: Utc::now(),
        };
        
        (status, Json(error_response)).into_response()
    }
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
```

## API Documentation

### OpenAPI Specification

The system automatically generates OpenAPI documentation for all endpoints.

```rust
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    paths(
        store_context,
        get_context,
        update_context,
        delete_context,
        search_contexts,
        health_check,
        health_ready,
        health_live,
        prometheus_metrics,
        system_stats,
    ),
    components(
        schemas(
            StoreContextRequest,
            StoreContextResponse,
            SearchContextsRequest,
            ContextResponse,
            Context,
            ErrorResponse,
            SystemHealth,
            ComponentHealth,
        )
    ),
    tags(
        (name = "contexts", description = "Context management operations"),
        (name = "health", description = "Health check endpoints"),
        (name = "metrics", description = "Metrics and monitoring"),
    )
)]
pub struct ApiDoc;

pub fn create_api_docs() -> utoipa::swagger::Swagger {
    ApiDoc::openapi()
}
```

### Documentation Endpoint

```rust
pub async fn api_docs() -> Json<utoipa::swagger::Swagger> {
    Json(create_api_docs())
}

pub async fn api_docs_ui() -> impl IntoResponse {
    Html(include_str!("../static/swagger.html").to_string())
}
```

This comprehensive API layer provides a clean, well-documented interface to the Rust-HiRAG system with support for multiple communication protocols, comprehensive error handling, and production-ready features.