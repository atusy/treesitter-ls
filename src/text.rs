pub mod position;

// Re-export main types and functions
pub use position::{PositionMapper, convert_byte_to_utf16_in_line, convert_utf16_to_byte_in_line};
