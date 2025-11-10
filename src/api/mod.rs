//! API handlers for context management

pub mod handlers;
pub mod routes;
pub mod routes_vision;
pub mod router_complete;
pub mod vision;
pub mod integration;

pub use handlers::*;
pub use routes::build_router;
pub use routes_vision::build_vision_routes;
pub use router_complete::build_complete_router;
pub use vision::VisionState;
pub use integration::{init_vision_service, init_facts_store, build_facts_routes};