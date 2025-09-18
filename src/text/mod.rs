pub mod position;

// Re-export main types and functions
pub use position::{
    SimplePositionMapper, compute_line_starts, convert_byte_to_utf16_in_line,
    convert_utf16_to_byte_in_line, extract_text_from_ranges,
};
