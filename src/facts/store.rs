//! Facts store implementation using Qdrant

use super::models::*;
use crate::error::{Result, ContextError};
use qdrant_client::{
    client::QdrantClient,
    qdrant::{
        CreateCollection, Distance, VectorParams, VectorsConfig,
        PointStruct, SearchPoints, Filter, Condition, FieldCondition, Match,
    },
};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Facts store configuration
#[derive(Debug, Clone)]
pub struct FactStoreConfig {
    pub collection_name: String,
    pub dedup_enabled: bool,
    pub confidence_threshold: f32,
    pub max_facts_per_query: usize,
    pub vector_size: usize,
}

impl Default for FactStoreConfig {
    fn default() -> Self {
        Self {
            collection_name: "facts".to_string(),
            dedup_enabled: true,
            confidence_threshold: 0.8,
            max_facts_per_query: 100,
            vector_size: 1024,
        }
    }
}

/// Facts store
pub struct FactStore {
    client: QdrantClient,
    config: FactStoreConfig,
}

impl FactStore {
    /// Create a new facts store
    pub async fn new(client: QdrantClient, config: FactStoreConfig) -> Result<Self> {
        let store = Self { client, config };
        store.ensure_collection().await?;
        Ok(store)
    }

    /// Ensure the facts collection exists
    async fn ensure_collection(&self) -> Result<()> {
        // Check if collection exists
        let collections = self.client
            .list_collections()
            .await
            .map_err(|e| ContextError::Internal(format!("Failed to list collections: {}", e)))?;

        let exists = collections
            .collections
            .iter()
            .any(|c| c.name == self.config.collection_name);

        if !exists {
            info!("Creating facts collection: {}", self.config.collection_name);
            
            self.client
                .create_collection(&CreateCollection {
                    collection_name: self.config.collection_name.clone(),
                    vectors_config: Some(VectorsConfig {
                        config: Some(qdrant_client::qdrant::vectors_config::Config::Params(
                            VectorParams {
                                size: self.config.vector_size as u64,
                                distance: Distance::Cosine.into(),
                                ..Default::default()
                            },
                        )),
                    }),
                    ..Default::default()
                })
                .await
                .map_err(|e| ContextError::Internal(format!("Failed to create collection: {}", e)))?;
        }

        Ok(())
    }

    /// Insert a fact
    pub async fn insert_fact(&self, request: FactInsertRequest) -> Result<FactInsertResponse> {
        let fact = Fact::new(
            request.subject,
            request.predicate,
            request.object,
            request.source_anchor,
            request.confidence,
        );

        debug!("Inserting fact: id={}, hash={}", fact.id, fact.hash);

        // Check for duplicates if enabled
        if self.config.dedup_enabled {
            if let Some(existing) = self.check_duplicate(&fact.hash).await? {
                warn!("Duplicate fact detected: hash={}", fact.hash);
                return Ok(FactInsertResponse {
                    fact_id: existing,
                    hash: fact.hash,
                    duplicate: true,
                });
            }
        }

        // Check confidence threshold
        if !fact.meets_threshold(self.config.confidence_threshold) {
            return Err(ContextError::Internal(format!(
                "Fact confidence {} below threshold {}",
                fact.confidence, self.config.confidence_threshold
            )));
        }

        // Create payload using serde_json for safety
        let payload_json = serde_json::json!({
            "subject": fact.subject,
            "predicate": fact.predicate,
            "object": fact.object,
            "confidence": fact.confidence,
            "hash": fact.hash,
            "observed_at": fact.observed_at.to_rfc3339(),
            "source_doc": fact.source_doc,
        });

        // Convert to HashMap for Qdrant
        // Note: PointStruct::new accepts serde_json::Value in recent qdrant-client versions
        // If compilation fails, uncomment the QValue mapping below
        let payload: HashMap<String, serde_json::Value> = payload_json
            .as_object()
            .ok_or_else(|| ContextError::Internal("Failed to create payload object".to_string()))?
            .clone()
            .into_iter()
            .collect();

        // Alternative: Map to qdrant::Value if needed (uncomment if compile fails)
        // use qdrant_client::qdrant::value::Value as QValue;
        // let payload: HashMap<String, QValue> = payload
        //     .into_iter()
        //     .map(|(k, v)| (k, QValue::from(v)))
        //     .collect();

        // Create dummy vector (in production, this would be an embedding)
        let vector = vec![0.0; self.config.vector_size];

        // Insert into Qdrant
        let point = PointStruct::new(
            fact.id.clone(),
            vector,
            payload,
        );

        self.client
            .upsert_points(&self.config.collection_name, None, vec![point], None)
            .await
            .map_err(|e| ContextError::Internal(format!("Failed to insert fact: {}", e)))?;

        info!("Fact inserted successfully: id={}", fact.id);

        Ok(FactInsertResponse {
            fact_id: fact.id,
            hash: fact.hash,
            duplicate: false,
        })
    }

    /// Check for duplicate fact by hash using filter-only scroll
    async fn check_duplicate(&self, hash: &str) -> Result<Option<String>> {
        use qdrant_client::qdrant::{ScrollPoints, WithPayloadSelector, with_payload_selector::SelectorOptions};
        
        let filter = Filter {
            must: vec![Condition {
                condition_one_of: Some(qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    FieldCondition {
                        key: "hash".to_string(),
                        r#match: Some(Match {
                            match_value: Some(qdrant_client::qdrant::r#match::MatchValue::Keyword(
                                hash.to_string(),
                            )),
                        }),
                        ..Default::default()
                    },
                )),
            }],
            ..Default::default()
        };

        let with_payload = WithPayloadSelector {
            selector_options: Some(SelectorOptions::Enable(true))
        };

        let scroll_result = self.client
            .scroll(&ScrollPoints {
                collection_name: self.config.collection_name.clone(),
                filter: Some(filter),
                limit: Some(1u32),
                with_payload: Some(with_payload),
                ..Default::default()
            })
            .await
            .map_err(|e| ContextError::Internal(format!("Failed to check for duplicate: {}", e)))?;

        if let Some(point) = scroll_result.points.first() {
            Ok(point.id.as_ref().map(|id| id.to_string()))
        } else {
            Ok(None)
        }
    }

    /// Query facts
    pub async fn query_facts(&self, query: FactQuery) -> Result<FactQueryResponse> {
        debug!("Querying facts: {:?}", query);

        // Build filter conditions
        let mut conditions = Vec::new();

        if let Some(subject) = &query.subject {
            conditions.push(Condition {
                condition_one_of: Some(qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    FieldCondition {
                        key: "subject".to_string(),
                        r#match: Some(Match {
                            match_value: Some(qdrant_client::qdrant::r#match::MatchValue::Keyword(
                                subject.clone(),
                            )),
                        }),
                        ..Default::default()
                    },
                )),
            });
        }

        if let Some(predicate) = &query.predicate {
            conditions.push(Condition {
                condition_one_of: Some(qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    FieldCondition {
                        key: "predicate".to_string(),
                        r#match: Some(Match {
                            match_value: Some(qdrant_client::qdrant::r#match::MatchValue::Keyword(
                                predicate.clone(),
                            )),
                        }),
                        ..Default::default()
                    },
                )),
            });
        }

        if let Some(object) = &query.object {
            conditions.push(Condition {
                condition_one_of: Some(qdrant_client::qdrant::condition::ConditionOneOf::Field(
                    FieldCondition {
                        key: "object".to_string(),
                        r#match: Some(Match {
                            match_value: Some(qdrant_client::qdrant::r#match::MatchValue::Keyword(
                                object.clone(),
                            )),
                        }),
                        ..Default::default()
                    },
                )),
            });
        }

        let filter = if conditions.is_empty() {
            None
        } else {
            Some(Filter {
                must: conditions,
                ..Default::default()
            })
        };

        // Search with limit
        let limit = query.limit.min(self.config.max_facts_per_query);
        
        let search_result = self.client
            .search_points(&SearchPoints {
                collection_name: self.config.collection_name.clone(),
                vector: vec![0.0; self.config.vector_size],
                filter,
                limit: limit as u64,
                with_payload: Some(true.into()),
                ..Default::default()
            })
            .await
            .map_err(|e| ContextError::Internal(format!("Failed to query facts: {}", e)))?;

        // Convert results to facts
        let facts: Vec<Fact> = search_result
            .result
            .iter()
            .filter_map(|point| {
                let payload = point.payload.as_ref()?;
                
                Some(Fact {
                    id: point.id.clone()?.to_string(),
                    subject: payload.get("subject")?.as_str()?.to_string(),
                    predicate: payload.get("predicate")?.as_str()?.to_string(),
                    object: payload.get("object")?.as_str()?.to_string(),
                    datatype: None,
                    source_doc: payload.get("source_doc").and_then(|v| v.as_str()).map(String::from),
                    source_anchor: SourceAnchor::default(),
                    confidence: payload.get("confidence")?.as_f64()? as f32,
                    observed_at: chrono::DateTime::parse_from_rfc3339(
                        payload.get("observed_at")?.as_str()?
                    ).ok()?.with_timezone(&chrono::Utc),
                    hash: payload.get("hash")?.as_str()?.to_string(),
                })
            })
            .collect();

        let total = facts.len();

        info!("Query returned {} facts", total);

        Ok(FactQueryResponse { facts, total })
    }

    /// Get configuration
    pub fn config(&self) -> &FactStoreConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests require a running Qdrant instance
    // They are marked as ignored by default

    #[tokio::test]
    #[ignore]
    async fn test_fact_store_creation() {
        let client = QdrantClient::from_url("http://localhost:6334").build().unwrap();
        let config = FactStoreConfig::default();
        let store = FactStore::new(client, config).await;
        assert!(store.is_ok());
    }
}