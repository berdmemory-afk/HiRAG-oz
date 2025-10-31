# Middleware Components

## Overview

Rust-HiRAG implements a comprehensive middleware layer that provides enterprise-grade security, rate limiting, input validation, and request processing capabilities. These middleware components work together to ensure the system is secure, performant, and reliable in production environments.

## Middleware Architecture

### Component Overview

The middleware layer consists of four main components:

1. **Authentication Middleware**: Token-based API security
2. **Rate Limiting Middleware**: Request throttling and abuse prevention
3. **Input Validation Middleware**: Comprehensive input sanitization
4. **Body Limiting Middleware**: Request size management

Each component is designed to be composable and can be applied independently or in combination based on configuration.

## Authentication Middleware

### Token-Based Authentication

The authentication middleware provides secure API access using bearer tokens with configurable validation rules.

#### Configuration

```rust
pub struct AuthConfig {
    pub enabled: bool,
    pub tokens: Vec<String>,
    pub header_name: String,
    pub token_prefix: String,
    pub strict_mode: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            tokens: vec![],
            header_name: "Authorization".to_string(),
            token_prefix: "Bearer".to_string(),
            strict_mode: true,
        }
    }
}
```

#### Implementation

```rust
pub struct AuthMiddleware {
    config: AuthConfig,
    valid_tokens: HashSet<String>,
}

impl AuthMiddleware {
    pub fn new(config: AuthConfig) -> Self {
        let valid_tokens = config.tokens.iter().cloned().collect();
        
        Self {
            config,
            valid_tokens,
        }
    }
    
    pub fn validate_token(&self, auth_header: Option<&str>) -> AuthResult {
        if !self.config.enabled {
            return AuthResult::Success;
        }
        
        let header = match auth_header {
            Some(h) => h,
            None => {
                if self.config.strict_mode {
                    return AuthResult::MissingToken;
                } else {
                    return AuthResult::Success;
                }
            }
        };
        
        // Extract token from "Bearer <token>" format
        let token = if let Some(prefix) = header.strip_prefix(&format!("{} ", self.config.token_prefix)) {
            prefix
        } else if self.config.strict_mode {
            return AuthResult::InvalidFormat;
        } else {
            header // Allow tokens without prefix in non-strict mode
        };
        
        if self.valid_tokens.contains(token) {
            AuthResult::Success
        } else {
            AuthResult::InvalidToken
        }
    }
    
    pub fn extract_client_id(&self, auth_header: Option<&str>) -> Option<String> {
        if let Some(header) = auth_header {
            if let Some(token) = header.strip_prefix(&format!("{} ", self.config.token_prefix)) {
                Some(self.hash_client_id(token))
            } else {
                Some(self.hash_client_id(header))
            }
        } else {
            None
        }
    }
    
    fn hash_client_id(&self, token: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        token.hash(&mut hasher);
        format!("client_{:x}", hasher.finish())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuthResult {
    Success,
    MissingToken,
    InvalidFormat,
    InvalidToken,
}
```

#### Axum Integration

```rust
impl AuthMiddleware {
    pub async fn axum_layer(
        config: AuthConfig,
    ) -> impl Layer<Route> + Clone {
        let auth = Arc::new(Self::new(config));
        
        move || {
            let auth = auth.clone();
            ServiceBuilder::new()
                .layer(move |req: Request<Body>, next: Next<Body>| {
                    let auth = auth.clone();
                    async move {
                        let auth_header = req.headers()
                            .get(&auth.config.header_name)
                            .and_then(|h| h.to_str().ok());
                        
                        match auth.validate_token(auth_header) {
                            AuthResult::Success => {
                                // Add client ID to request extensions
                                let client_id = auth.extract_client_id(auth_header);
                                let mut req = req;
                                if let Some(id) = client_id {
                                    req.extensions_mut().insert(ClientId(id));
                                }
                                
                                next.run(req).await
                            }
                            AuthResult::MissingToken => {
                                Response::builder()
                                    .status(StatusCode::UNAUTHORIZED)
                                    .body(Body::from("Missing authentication token"))
                                    .unwrap()
                            }
                            AuthResult::InvalidFormat => {
                                Response::builder()
                                    .status(StatusCode::UNAUTHORIZED)
                                    .body(Body::from("Invalid authentication format"))
                                    .unwrap()
                            }
                            AuthResult::InvalidToken => {
                                Response::builder()
                                    .status(StatusCode::UNAUTHORIZED)
                                    .body(Body::from("Invalid authentication token"))
                                    .unwrap()
                            }
                        }
                    }
                })
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientId(pub String);
```

## Rate Limiting Middleware

### Sliding Window Rate Limiting

The rate limiting middleware implements a sophisticated sliding window algorithm to prevent abuse while allowing legitimate traffic.

#### Configuration

```rust
pub struct RateLimitConfig {
    pub enabled: bool,
    pub requests_per_window: usize,
    pub window_secs: u64,
    pub burst_size: usize,
    pub cleanup_interval_secs: u64,
    pub max_clients: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_window: 100,
            window_secs: 60,
            burst_size: 10,
            cleanup_interval_secs: 300,
            max_clients: 10000,
        }
    }
}
```

#### Implementation

```rust
pub struct RateLimiter {
    config: RateLimitConfig,
    client_data: Arc<DashMap<String, ClientRateData>>,
    cleanup_task: JoinHandle<()>,
}

struct ClientRateData {
    requests: VecDeque<Instant>,
    burst_tokens: usize,
    last_reset: Instant,
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        let client_data = Arc::new(DashMap::new());
        let cleanup_data = client_data.clone();
        let cleanup_interval = config.cleanup_interval_secs;
        
        // Start cleanup task
        let cleanup_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(cleanup_interval));
            loop {
                interval.tick().await;
                Self::cleanup_expired_clients(&cleanup_data).await;
            }
        });
        
        Self {
            config,
            client_data,
            cleanup_task,
        }
    }
    
    pub async fn check_rate_limit(&self, client_id: &str) -> RateLimitResult {
        if !self.config.enabled {
            return RateLimitResult::Allowed;
        }
        
        let now = Instant::now();
        let window_start = now - Duration::from_secs(self.config.window_secs);
        
        let mut client_data = self.client_data.entry(client_id.to_string()).or_insert_with(|| {
            ClientRateData {
                requests: VecDeque::new(),
                burst_tokens: self.config.burst_size,
                last_reset: now,
            }
        });
        
        // Clean old requests outside the window
        while let Some(&front_time) = client_data.requests.front() {
            if front_time < window_start {
                client_data.requests.pop_front();
            } else {
                break;
            }
        }
        
        // Check rate limit
        if client_data.requests.len() >= self.config.requests_per_window {
            // Try to use burst tokens
            if client_data.burst_tokens > 0 {
                client_data.burst_tokens -= 1;
                client_data.requests.push_back(now);
                RateLimitResult::AllowedBurst
            } else {
                RateLimitResult::Limited {
                    retry_after: self.calculate_retry_after(&client_data, now),
                }
            }
        } else {
            // Normal allowance
            client_data.requests.push_back(now);
            
            // Refill burst tokens slowly
            if client_data.burst_tokens < self.config.burst_size {
                let time_since_reset = now.duration_since(client_data.last_reset);
                let refill_interval = Duration::from_secs(self.config.window_secs) / self.config.burst_size as u32;
                
                if time_since_reset >= refill_interval {
                    client_data.burst_tokens = (client_data.burst_tokens + 1).min(self.config.burst_size);
                    client_data.last_reset = now;
                }
            }
            
            RateLimitResult::Allowed
        }
    }
    
    fn calculate_retry_after(&self, client_data: &ClientRateData, now: Instant) -> u64 {
        if let Some(&oldest_request) = client_data.requests.front() {
            let retry_time = oldest_request + Duration::from_secs(self.config.window_secs);
            if retry_time > now {
                retry_time.duration_since(now).as_secs()
            } else {
                1
            }
        } else {
            1
        }
    }
    
    async fn cleanup_expired_clients(client_data: &DashMap<String, ClientRateData>) {
        let now = Instant::now();
        let expiry_duration = Duration::from_secs(3600); // 1 hour
        
        client_data.retain(|_, data| {
            now.duration_since(data.last_reset) < expiry_duration
        });
    }
    
    pub fn get_stats(&self) -> RateLimitStats {
        let total_clients = self.client_data.len();
        let active_clients = self.client_data.iter()
            .filter(|entry| {
                let now = Instant::now();
                let window_start = now - Duration::from_secs(self.config.window_secs);
                entry.value().requests.back().map_or(false, |&time| time >= window_start)
            })
            .count();
        
        RateLimitStats {
            total_clients,
            active_clients,
            max_clients: self.config.max_clients,
        }
    }
}

#[derive(Debug, Clone)]
pub enum RateLimitResult {
    Allowed,
    AllowedBurst,
    Limited { retry_after: u64 },
}

#[derive(Debug, Clone)]
pub struct RateLimitStats {
    pub total_clients: usize,
    pub active_clients: usize,
    pub max_clients: usize,
}
```

#### Axum Integration

```rust
impl RateLimiter {
    pub fn axum_layer(config: RateLimitConfig) -> impl Layer<Route> + Clone {
        let rate_limiter = Arc::new(Self::new(config));
        
        move || {
            let rate_limiter = rate_limiter.clone();
            ServiceBuilder::new()
                .layer(move |req: Request<Body>, next: Next<Body>| {
                    let rate_limiter = rate_limiter.clone();
                    async move {
                        // Extract client ID from request extensions
                        let client_id = req.extensions().get::<ClientId>()
                            .map(|id| id.0.clone())
                            .unwrap_or_else(|| "anonymous".to_string());
                        
                        match rate_limiter.check_rate_limit(&client_id).await {
                            RateLimitResult::Allowed | RateLimitResult::AllowedBurst => {
                                next.run(req).await
                            }
                            RateLimitResult::Limited { retry_after } => {
                                let response = Response::builder()
                                    .status(StatusCode::TOO_MANY_REQUESTS)
                                    .header("Retry-After", retry_after.to_string())
                                    .header("X-RateLimit-Limit", rate_limiter.config.requests_per_window.to_string())
                                    .header("X-RateLimit-Window", rate_limiter.config.window_secs.to_string())
                                    .body(Body::from("Rate limit exceeded"))
                                    .unwrap();
                                
                                response
                            }
                        }
                    }
                })
        }
    }
}
```

## Input Validation Middleware

### Comprehensive Input Sanitization

The input validation middleware provides comprehensive validation and sanitization for all incoming requests to prevent injection attacks and ensure data integrity.

#### Validation Rules

```rust
pub struct InputValidator;

impl InputValidator {
    pub fn validate_text(text: &str) -> Result<()> {
        if text.is_empty() {
            return Err(ValidationError::EmptyText.into());
        }
        
        if text.len() > 100000 {
            return Err(ValidationError::TextTooLong(text.len()).into());
        }
        
        // Check for potentially dangerous content
        if Self::contains_sql_injection_patterns(text) {
            return Err(ValidationError::SuspiciousContent("SQL injection pattern".to_string()).into());
        }
        
        if Self::contains_xss_patterns(text) {
            return Err(ValidationError::SuspiciousContent("XSS pattern".to_string()).into());
        }
        
        Ok(())
    }
    
    pub fn validate_metadata_key(key: &str) -> Result<()> {
        if key.is_empty() {
            return Err(ValidationError::EmptyMetadataKey.into());
        }
        
        if key.len() > 100 {
            return Err(ValidationError::MetadataKeyTooLong(key.len()).into());
        }
        
        // Only allow alphanumeric, underscore, and hyphen
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(ValidationError::InvalidMetadataKey(key.to_string()).into());
        }
        
        // Reserved keys
        let reserved_keys = ["id", "timestamp", "level", "text"];
        if reserved_keys.contains(&key) {
            return Err(ValidationError::ReservedMetadataKey(key.to_string()).into());
        }
        
        Ok(())
    }
    
    pub fn validate_token_count(count: usize, max_allowed: usize) -> Result<()> {
        if count == 0 {
            return Err(ValidationError::InvalidTokenCount(0).into());
        }
        
        if count > max_allowed {
            return Err(ValidationError::TokenCountExceeded { count, max_allowed }.into());
        }
        
        Ok(())
    }
    
    pub fn validate_vector_dimension(dimension: usize, expected: usize) -> Result<()> {
        if dimension != expected {
            return Err(ValidationError::InvalidVectorDimension { dimension, expected }.into());
        }
        
        Ok(())
    }
    
    pub fn validate_uuid(uuid_str: &str) -> Result<()> {
        uuid::Uuid::parse_str(uuid_str)
            .map_err(|_| ValidationError::InvalidUuid(uuid_str.to_string()))?;
        
        Ok(())
    }
    
    fn contains_sql_injection_patterns(text: &str) -> bool {
        let patterns = [
            "union select",
            "drop table",
            "insert into",
            "delete from",
            "update set",
            "'--",
            "/*",
            "*/",
            "xp_",
            "sp_",
        ];
        
        let lower_text = text.to_lowercase();
        patterns.iter().any(|pattern| lower_text.contains(pattern))
    }
    
    fn contains_xss_patterns(text: &str) -> bool {
        let patterns = [
            "<script",
            "</script>",
            "javascript:",
            "onload=",
            "onerror=",
            "onclick=",
            "onmouseover=",
            "eval(",
            "alert(",
        ];
        
        let lower_text = text.to_lowercase();
        patterns.iter().any(|pattern| lower_text.contains(pattern))
    }
    
    pub fn sanitize_text(text: &str) -> String {
        text.chars()
            .filter(|c| c.is_ascii() && !c.is_control())
            .collect::<String>()
            .trim()
            .to_string()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Text cannot be empty")]
    EmptyText,
    
    #[error("Text too long: {0} characters")]
    TextTooLong(usize),
    
    #[error("Suspicious content detected: {0}")]
    SuspiciousContent(String),
    
    #[error("Metadata key cannot be empty")]
    EmptyMetadataKey,
    
    #[error("Metadata key too long: {0} characters")]
    MetadataKeyTooLong(usize),
    
    #[error("Invalid metadata key: {0}")]
    InvalidMetadataKey(String),
    
    #[error("Reserved metadata key: {0}")]
    ReservedMetadataKey(String),
    
    #[error("Invalid token count: {0}")]
    InvalidTokenCount(usize),
    
    #[error("Token count exceeded: {count} > {max_allowed}")]
    TokenCountExceeded { count: usize, max_allowed: usize },
    
    #[error("Invalid vector dimension: {dimension}, expected: {expected}")]
    InvalidVectorDimension { dimension: usize, expected: usize },
    
    #[error("Invalid UUID: {0}")]
    InvalidUuid(String),
}
```

#### Axum Integration

```rust
pub struct ValidationMiddleware;

impl ValidationMiddleware {
    pub fn axum_layer() -> impl Layer<Route> + Clone {
        ServiceBuilder::new()
            .layer(|req: Request<Body>, next: Next<Body>| async move {
                // Validate request body size first
                if let Some(content_length) = req.headers().get("content-length") {
                    if let Ok(length_str) = content_length.to_str() {
                        if let Ok(length) = length_str.parse::<usize>() {
                            if length > 10 * 1024 * 1024 { // 10MB limit
                                return Response::builder()
                                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                                    .body(Body::from("Request body too large"))
                                    .unwrap();
                            }
                        }
                    }
                }
                
                // For POST/PUT requests, validate the body
                if matches!(req.method(), &Method::POST | &Method::PUT | &Method::PATCH) {
                    let content_type = req.headers()
                        .get("content-type")
                        .and_then(|h| h.to_str().ok())
                        .unwrap_or("");
                    
                    if content_type.contains("application/json") {
                        let (req, body_bytes) = match extract_body_bytes(req).await {
                            Ok(result) => result,
                            Err(_) => {
                                return Response::builder()
                                    .status(StatusCode::BAD_REQUEST)
                                    .body(Body::from("Invalid request body"))
                                    .unwrap();
                            }
                        };
                        
                        // Validate JSON structure
                        if let Err(e) = validate_json_body(&body_bytes) {
                            return Response::builder()
                                .status(StatusCode::BAD_REQUEST)
                                .body(Body::from(format!("Validation error: {}", e)))
                                .unwrap();
                        }
                        
                        // Reconstruct request with validated body
                        let req = reconstruct_request(req, body_bytes).await;
                        next.run(req).await
                    } else {
                        next.run(req).await
                    }
                } else {
                    next.run(req).await
                }
            })
    }
}

async fn extract_body_bytes(req: Request<Body>) -> Result<(Request<Body>, Bytes), Box<dyn std::error::Error>> {
    let (parts, body) = req.into_parts();
    let bytes = hyper::body::to_bytes(body).await?;
    let body = Body::from(bytes.clone());
    let req = Request::from_parts(parts, body);
    Ok((req, bytes))
}

fn validate_json_body(bytes: &Bytes) -> Result<(), Box<dyn std::error::Error>> {
    let json: serde_json::Value = serde_json::from_slice(bytes)?;
    
    // Validate based on expected structure
    if let Some(obj) = json.as_object() {
        if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
            InputValidator::validate_text(text)?;
        }
        
        if let Some(query) = obj.get("query").and_then(|v| v.as_str()) {
            InputValidator::validate_text(query)?;
        }
        
        if let Some(max_tokens) = obj.get("max_tokens").and_then(|v| v.as_u64()) {
            InputValidator::validate_token_count(max_tokens as usize, 100000)?;
        }
        
        if let Some(metadata) = obj.get("metadata").and_then(|v| v.as_object()) {
            for key in metadata.keys() {
                InputValidator::validate_metadata_key(key)?;
            }
        }
    }
    
    Ok(())
}

async fn reconstruct_request(req: Request<Body>, bytes: Bytes) -> Request<Body> {
    let (parts, _) = req.into_parts();
    let body = Body::from(bytes);
    Request::from_parts(parts, body)
}
```

## Body Limiting Middleware

### Request Size Management

The body limiting middleware prevents oversized requests that could consume excessive resources or cause denial of service.

#### Configuration

```rust
pub struct BodyLimitConfig {
    pub max_body_size_mb: usize,
    pub enable_compression: bool,
    pub compressed_size_multiplier: f64,
}

impl Default for BodyLimitConfig {
    fn default() -> Self {
        Self {
            max_body_size_mb: 10,
            enable_compression: true,
            compressed_size_multiplier: 0.3, // Allow 30% of original size for compressed content
        }
    }
}
```

#### Implementation

```rust
pub struct BodyLimiter {
    config: BodyLimitConfig,
}

impl BodyLimiter {
    pub fn new(config: BodyLimitConfig) -> Self {
        Self { config }
    }
    
    pub fn axum_layer(config: BodyLimitConfig) -> impl Layer<Route> + Clone {
        let limiter = Arc::new(Self::new(config));
        
        move || {
            let limiter = limiter.clone();
            ServiceBuilder::new()
                .layer(move |req: Request<Body>, next: Next<Body>| {
                    let limiter = limiter.clone();
                    async move {
                        // Check content-length header first
                        if let Some(content_length) = req.headers().get("content-length") {
                            if let Ok(length_str) = content_length.to_str() {
                                if let Ok(length) = length_str.parse::<usize>() {
                                    let max_size = limiter.calculate_max_size(&req);
                                    if length > max_size {
                                        return Response::builder()
                                            .status(StatusCode::PAYLOAD_TOO_LARGE)
                                            .header("X-Max-Size", max_size.to_string())
                                            .body(Body::from("Request body too large"))
                                            .unwrap();
                                    }
                                }
                            }
                        }
                        
                        // For streaming requests, we need to check the actual body
                        let (parts, body) = req.into_parts();
                        let limited_body = LimitedBody::new(
                            body,
                            limiter.calculate_max_size_from_parts(&parts),
                        );
                        
                        let req = Request::from_parts(parts, Body::from_stream(limited_body));
                        
                        next.run(req).await
                    }
                })
        }
    }
    
    fn calculate_max_size(&self, req: &Request<Body>) -> usize {
        let base_size = self.config.max_body_size_mb * 1024 * 1024;
        
        // Check for compression
        if let Some(encoding) = req.headers().get("content-encoding") {
            if let Ok(encoding_str) = encoding.to_str() {
                if encoding_str.contains("gzip") || encoding_str.contains("deflate") {
                    return (base_size as f64 * self.config.compressed_size_multiplier) as usize;
                }
            }
        }
        
        base_size
    }
    
    fn calculate_max_size_from_parts(&self, parts: &Parts) -> usize {
        let base_size = self.config.max_body_size_mb * 1024 * 1024;
        
        // Check for compression
        if let Some(encoding) = parts.headers.get("content-encoding") {
            if let Ok(encoding_str) = encoding.to_str() {
                if encoding_str.contains("gzip") || encoding_str.contains("deflate") {
                    return (base_size as f64 * self.config.compressed_size_multiplier) as usize;
                }
            }
        }
        
        base_size
    }
}

struct LimitedBody {
    inner: Body,
    remaining: usize,
}

impl LimitedBody {
    fn new(inner: Body, max_size: usize) -> Self {
        Self {
            inner,
            remaining: max_size,
        }
    }
}

impl Stream for LimitedBody {
    type Item = Result<bytes::Bytes, Box<dyn std::error::Error + Send + Sync>>;
    
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match std::pin::Pin::new(&mut self.inner).poll_next(cx) {
            std::task::Poll::Ready(Some(Ok(chunk))) => {
                let chunk_len = chunk.len();
                if chunk_len > self.remaining {
                    std::task::Poll::Ready(Some(Err("Request body too large".into())))
                } else {
                    self.remaining -= chunk_len;
                    std::task::Poll::Ready(Some(Ok(chunk)))
                }
            }
            std::task::Poll::Ready(Some(Err(e))) => {
                std::task::Poll::Ready(Some(Err(e.into())))
            }
            std::task::Poll::Ready(None) => {
                std::task::Poll::Ready(None)
            }
            std::task::Poll::Pending => {
                std::task::Poll::Pending
            }
        }
    }
}
```

## Middleware Composition

### Combined Middleware Stack

The middleware components are designed to work together in a specific order for optimal security and performance:

```rust
pub fn create_middleware_stack(
    auth_config: AuthConfig,
    rate_limit_config: RateLimitConfig,
    body_limit_config: BodyLimitConfig,
) -> Stack<Router> {
    Router::new()
        .layer(
            ServiceBuilder::new()
                // Apply in order: body limit -> auth -> rate limit -> validation
                .layer(BodyLimiter::axum_layer(body_limit_config))
                .layer(AuthMiddleware::axum_layer(auth_config))
                .layer(RateLimiter::axum_layer(rate_limit_config))
                .layer(ValidationMiddleware::axum_layer())
                .layer(TraceLayer::new_for_http())
        )
}
```

### Middleware Ordering

1. **Body Limiting**: First to prevent oversized requests
2. **Authentication**: Validate identity before processing
3. **Rate Limiting**: Prevent abuse per client
4. **Input Validation**: Sanitize and validate all inputs
5. **Tracing**: Add observability (last in the stack)

### Configuration Integration

```rust
impl From<&Config> for AuthConfig {
    fn from(config: &Config) -> Self {
        Self {
            enabled: config.auth.enabled,
            tokens: config.auth.tokens.clone(),
            header_name: config.auth.header_name.clone(),
            token_prefix: config.auth.token_prefix.clone(),
            strict_mode: config.auth.strict_mode,
        }
    }
}

impl From<&Config> for RateLimitConfig {
    fn from(config: &Config) -> Self {
        Self {
            enabled: config.rate_limit.enabled,
            requests_per_window: config.rate_limit.requests_per_window,
            window_secs: config.rate_limit.window_secs,
            burst_size: config.rate_limit.burst_size,
            cleanup_interval_secs: config.rate_limit.cleanup_interval_secs,
            max_clients: config.rate_limit.max_clients,
        }
    }
}

impl From<&Config> for BodyLimitConfig {
    fn from(config: &Config) -> Self {
        Self {
            max_body_size_mb: config.server.max_body_size_mb,
            enable_compression: config.server.enable_compression,
            compressed_size_multiplier: config.server.compressed_size_multiplier,
        }
    }
}
```

This comprehensive middleware stack provides enterprise-grade security, performance, and reliability for the Rust-HiRAG system, ensuring safe operation in production environments while maintaining high performance and usability.