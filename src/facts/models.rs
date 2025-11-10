//! Data models for facts store

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// Source anchor for fact provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAnchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vt_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_line: Option<String>,
}

impl SourceAnchor {
    pub fn new() -> Self {
        Self {
            doc_id: None,
            page: None,
            region_id: None,
            vt_ref: None,
            path_line: None,
        }
    }

    pub fn with_doc(mut self, doc_id: String, page: Option<u32>) -> Self {
        self.doc_id = Some(doc_id);
        self.page = page;
        self
    }

    pub fn with_region(mut self, region_id: String, vt_ref: Option<String>) -> Self {
        self.region_id = Some(region_id);
        self.vt_ref = vt_ref;
        self
    }

    pub fn with_code(mut self, path_line: String) -> Self {
        self.path_line = Some(path_line);
        self
    }
}

impl Default for SourceAnchor {
    fn default() -> Self {
        Self::new()
    }
}

/// Fact (RDF-style triple)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datatype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doc: Option<String>,
    pub source_anchor: SourceAnchor,
    pub confidence: f32,
    pub observed_at: DateTime<Utc>,
    pub hash: String,
}

impl Fact {
    /// Create a new fact
    pub fn new(
        subject: String,
        predicate: String,
        object: String,
        source_anchor: SourceAnchor,
        confidence: f32,
    ) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let hash = Self::compute_hash(&subject, &predicate, &object, &source_anchor);
        
        Self {
            id,
            subject,
            predicate,
            object,
            datatype: None,
            source_doc: source_anchor.doc_id.clone(),
            source_anchor,
            confidence: confidence.clamp(0.0, 1.0),
            observed_at: Utc::now(),
            hash,
        }
    }

    /// Compute hash for deduplication
    pub fn compute_hash(
        subject: &str,
        predicate: &str,
        object: &str,
        source_anchor: &SourceAnchor,
    ) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(subject.as_bytes());
        hasher.update(b"|");
        hasher.update(predicate.as_bytes());
        hasher.update(b"|");
        hasher.update(object.as_bytes());
        hasher.update(b"|");
        
        // Include source anchor in hash
        if let Some(doc_id) = &source_anchor.doc_id {
            hasher.update(doc_id.as_bytes());
        }
        if let Some(page) = source_anchor.page {
            hasher.update(page.to_string().as_bytes());
        }
        if let Some(region_id) = &source_anchor.region_id {
            hasher.update(region_id.as_bytes());
        }
        
        format!("{:x}", hasher.finalize())
    }

    /// Check if fact meets confidence threshold
    pub fn meets_threshold(&self, threshold: f32) -> bool {
        self.confidence >= threshold
    }
}

/// Fact insert request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactInsertRequest {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    #[serde(default)]
    pub datatype: Option<String>,
    #[serde(default)]
    pub source_doc: Option<String>,
    pub source_anchor: SourceAnchor,
    pub confidence: f32,
}

/// Fact insert response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactInsertResponse {
    pub fact_id: String,
    pub hash: String,
    pub duplicate: bool,
}

/// Query criteria for facts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub predicate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_confidence: Option<f32>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

impl Default for FactQuery {
    fn default() -> Self {
        Self {
            subject: None,
            predicate: None,
            object: None,
            source_doc: None,
            min_confidence: None,
            limit: default_limit(),
        }
    }
}

/// Fact query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactQueryRequest {
    pub query: FactQuery,
}

/// Fact query response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactQueryResponse {
    pub facts: Vec<Fact>,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fact_creation() {
        let anchor = SourceAnchor::new().with_doc("doc_1".to_string(), Some(1));
        let fact = Fact::new(
            "Rust".to_string(),
            "is_a".to_string(),
            "programming_language".to_string(),
            anchor,
            0.95,
        );

        assert!(!fact.id.is_empty());
        assert!(!fact.hash.is_empty());
        assert_eq!(fact.confidence, 0.95);
    }

    #[test]
    fn test_hash_computation() {
        let anchor1 = SourceAnchor::new().with_doc("doc_1".to_string(), Some(1));
        let anchor2 = SourceAnchor::new().with_doc("doc_1".to_string(), Some(1));

        let hash1 = Fact::compute_hash("A", "B", "C", &anchor1);
        let hash2 = Fact::compute_hash("A", "B", "C", &anchor2);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_confidence_clamping() {
        let anchor = SourceAnchor::new();
        let fact = Fact::new(
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            anchor,
            1.5, // Over 1.0
        );

        assert_eq!(fact.confidence, 1.0);
    }

    #[test]
    fn test_meets_threshold() {
        let anchor = SourceAnchor::new();
        let fact = Fact::new(
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            anchor,
            0.85,
        );

        assert!(fact.meets_threshold(0.8));
        assert!(!fact.meets_threshold(0.9));
    }
}