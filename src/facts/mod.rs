//! Facts store for neuro-symbolic reasoning
//!
//! Implements a fact storage and retrieval system using Qdrant for:
//! - RDF-style triple storage (subject, predicate, object)
//! - Provenance tracking (source documents, pages, regions)
//! - Confidence scoring
//! - Hash-based deduplication

pub mod store;
pub mod models;
pub mod handlers;

pub use store::{FactStore, FactStoreConfig};
pub use models::{Fact, FactQuery, FactInsertRequest, FactQueryRequest, SourceAnchor};
pub use handlers::{insert_fact, query_facts, FactsState};