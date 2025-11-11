//! Vision API endpoints for DeepSeek OCR integration
//!
//! Implements the vision API specification from brainstorming.md:
//! - POST /api/v1/vision/search - Search regions by query
//! - POST /api/v1/vision/decode - Decode regions to text
//! - POST /api/v1/vision/index - Index documents
//! - GET /api/v1/vision/index/jobs/{job_id} - Job status

pub mod handlers;
pub mod models;
pub mod client;
pub mod cache;
pub mod circuit_breaker;
pub mod deepseek_config;
pub mod deepseek_client;

pub use handlers::{search_regions, decode_regions, index_document, get_job_status, VisionState};
pub use models::{
    VisionSearchRequest, VisionSearchResponse, DecodeRequest, DecodeResponse,
    IndexRequest, IndexResponse, JobStatus, Region, BoundingBox, FidelityLevel,
};
pub use client::VisionServiceClient;
pub use deepseek_client::DeepseekOcrClient;
pub use deepseek_config::DeepseekConfig;