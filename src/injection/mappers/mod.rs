pub mod injection_mapper;
pub mod range_mapper;
pub mod semantic_token_mapper;

// Re-export position_mapper from text module
pub use crate::document::coordinates as position_mapper;
