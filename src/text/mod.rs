pub mod edits;
pub mod position;
pub mod query;

// Re-export main types and functions
pub use edits::{adjust_ranges_for_edit, edit_affects_ranges, transform_edit_for_injection};
pub use position::{
    PositionMapper, SimplePositionMapper, compute_line_starts, convert_byte_to_utf16_in_line,
    convert_utf16_to_byte_in_line, extract_text_from_ranges,
};
pub use query::filter_captures;
