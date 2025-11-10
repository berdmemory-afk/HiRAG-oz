//! Data models for vision API

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Bounding box coordinates (pixels, origin top-left)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Fidelity level for decoding
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FidelityLevel {
    /// Coarse skim (fast), ~60% accuracy - navigation only
    #[serde(rename = "20x")]
    Fast,
    /// Balanced (default), ~97% precision - most uses
    #[serde(rename = "10x")]
    Balanced,
    /// High fidelity - tables/code blocks
    #[serde(rename = "5x")]
    High,
    /// Exact decode - final verification only
    #[serde(rename = "1x")]
    Exact,
}

impl Default for FidelityLevel {
    fn default() -> Self {
        Self::Balanced
    }
}

impl FidelityLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fast => "20x",
            Self::Balanced => "10x",
            Self::High => "5x",
            Self::Exact => "1x",
        }
    }
}

/// Region type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionType {
    Table,
    Figure,
    Code,
    Text,
}

/// Vision region
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub region_id: String,
    pub doc_id: String,
    pub page: u32,
    pub bbox: BoundingBox,
    #[serde(rename = "type")]
    pub region_type: RegionType,
    pub score: f32,
    pub why_relevant: String,
    pub has_vt: bool,
    pub token_estimate: usize,
}

/// Vision search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionSearchRequest {
    pub query: String,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    #[serde(default)]
    pub filters: HashMap<String, String>,
}

fn default_top_k() -> usize {
    12
}

/// Vision search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionSearchResponse {
    pub regions: Vec<Region>,
}

/// Decode request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeRequest {
    pub region_ids: Vec<String>,
    #[serde(default)]
    pub fidelity: FidelityLevel,
}

/// Decoded region result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedRegion {
    pub region_id: String,
    pub text: String,
    pub fidelity: String,
    pub confidence: f32,
}

/// Decode response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodeResponse {
    pub results: Vec<DecodedRegion>,
}

/// Index request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRequest {
    pub doc_url: String,
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub force_reindex: bool,
}

/// Job status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

/// Index response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexResponse {
    pub job_id: String,
    pub status: JobStatus,
}

/// Job status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusResponse {
    pub job_id: String,
    pub status: JobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
}

/// API error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Standard error codes from brainstorming.md
pub mod error_codes {
    pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
    pub const RATE_LIMIT: &str = "RATE_LIMIT";
    pub const UNAUTHORIZED: &str = "UNAUTHORIZED";
    pub const FORBIDDEN: &str = "FORBIDDEN";
    pub const NOT_FOUND: &str = "NOT_FOUND";
    pub const CONFLICT: &str = "CONFLICT";
    pub const TIMEOUT: &str = "TIMEOUT";
    pub const UPSTREAM_ERROR: &str = "UPSTREAM_ERROR";
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
}