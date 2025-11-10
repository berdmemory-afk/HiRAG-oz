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

        // Create payload
        let mut payload = HashMap::new();
        payload.insert("subject".to_string(), fact.subject.clone().into());
        payload.insert("predicate".to_string(), fact.predicate.clone().into());
        payload.insert("object".to_string(), fact.object.clone().into());
        payload.insert("confidence".to_string(), (fact.confidence as f64).into());
        payload.insert("hash".to_string(), fact.hash.clone().into());
        payload.insert("observed_at".to_string(), fact.observed_at.to_rfc3339().into());

        if let Some(source_doc) = &fact.source_doc {
            payload.insert("source_doc".to_string(), source_doc.clone().into());
        }

        // Create dummy vector (in production, this would be an embedding)
        let vector = vec![0.0; self.config.vector_size];

        // Insert into Qdrant
        let point = PointStruct::new(
            fact.id.clone(),
            vector,
            payload,
        );

        self.client
            .upsert_points_blocking(&self.config.collection_name, vec![point])
            .await
            .map_err(|e| ContextError::Internal(format!("Failed to insert fact: {}", e)))?;

        info!("Fact inserted successfully: id={}", fact.id);

        Ok(FactInsertResponse {
            fact_id: fact.id,
            hash: fact.hash,
            duplicate: false,
        })
    }

    /// Check for duplicate fact by hash
    async fn check_duplicate(&self, hash: &str) -> Result<Option<String>> {
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

        let search_result = self.client
            .search_points(&SearchPoints {
                collection_name: self.config.collection_name.clone(),
                vector: vec![0.0; self.config.vector_size],
                filter: Some(filter),
                limit: 1,
                with_payload: Some(true.into()),
                ..Default::default()
            })
            .await
            .map_err(|e| ContextError::Internal(format!("Failed to search for duplicate: {}", e)))?;

        if let Some(point) = search_result.result.first() {
            Ok(Some(point.id.clone().unwrap().to_string()))
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