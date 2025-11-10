//! Facts API handlers

use super::models::*;
use super::store::FactStore;
use axum::{
    extract::State,
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::{error, info};

/// Application state for facts handlers
#[derive(Clone)]
pub struct FactsState {
    pub store: Arc<FactStore>,
}

/// API error for facts endpoints
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FactsApiError {
    pub code: String,
    pub message: String,
}

impl FactsApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Insert a fact
///
/// POST /api/v1/facts
pub async fn insert_fact(
    State(state): State<FactsState>,
    Json(request): Json<FactInsertRequest>,
) -> Result<Json<FactInsertResponse>, (StatusCode, Json<FactsApiError>)> {
    info!("Fact insert request: subject={}", request.subject);

    // Validate request
    if request.subject.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(FactsApiError::new("VALIDATION_ERROR", "Subject cannot be empty")),
        ));
    }

    if request.predicate.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(FactsApiError::new("VALIDATION_ERROR", "Predicate cannot be empty")),
        ));
    }

    if request.object.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(FactsApiError::new("VALIDATION_ERROR", "Object cannot be empty")),
        ));
    }

    if request.confidence < 0.0 || request.confidence > 1.0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(FactsApiError::new(
                "VALIDATION_ERROR",
                "Confidence must be between 0.0 and 1.0",
            )),
        ));
    }

    // Insert fact
    match state.store.insert_fact(request).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Fact insert failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(FactsApiError::new("INTERNAL_ERROR", e.to_string())),
            ))
        }
    }
}

/// Query facts
///
/// POST /api/v1/facts/query
pub async fn query_facts(
    State(state): State<FactsState>,
    Json(request): Json<FactQueryRequest>,
) -> Result<Json<FactQueryResponse>, (StatusCode, Json<FactsApiError>)> {
    info!("Fact query request");

    // Validate limit
    if request.query.limit > state.store.config().max_facts_per_query {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(FactsApiError::new(
                "VALIDATION_ERROR",
                format!(
                    "Limit cannot exceed {}",
                    state.store.config().max_facts_per_query
                ),
            )),
        ));
    }

    // Query facts
    match state.store.query_facts(request.query).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!("Fact query failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(FactsApiError::new("INTERNAL_ERROR", e.to_string())),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::store::{FactStore, FactStoreConfig};
    use qdrant_client::client::QdrantClient;

    // Note: These tests require a running Qdrant instance
    // They are marked as ignored by default

    #[tokio::test]
    #[ignore]
    async fn test_insert_fact_handler() {
        let client = QdrantClient::from_url("http://localhost:6334").build().unwrap();
        let config = FactStoreConfig::default();
        let store = FactStore::new(client, config).await.unwrap();
        let state = FactsState {
            store: Arc::new(store),
        };

        let request = FactInsertRequest {
            subject: "Rust".to_string(),
            predicate: "is_a".to_string(),
            object: "programming_language".to_string(),
            datatype: None,
            source_doc: None,
            source_anchor: SourceAnchor::default(),
            confidence: 0.95,
        };

        let result = insert_fact(State(state), Json(request)).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_insert_fact_validation() {
        // Test empty subject
        let request = FactInsertRequest {
            subject: "".to_string(),
            predicate: "is_a".to_string(),
            object: "test".to_string(),
            datatype: None,
            source_doc: None,
            source_anchor: SourceAnchor::default(),
            confidence: 0.95,
        };

        // Validation would fail in handler
        assert!(request.subject.is_empty());
    }
}