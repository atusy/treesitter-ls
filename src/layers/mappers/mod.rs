pub mod injection_mapper;
pub mod range_mapper;
pub mod semantic_token_mapper;

// Re-export position_mapper from text module
pub use crate::text::position as position_mapper;
