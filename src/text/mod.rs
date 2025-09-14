// Re-export from document module for backward compatibility
pub use crate::document::{
    ParsedDocument, PositionMapper, SimplePositionMapper, StatefulDocument, TextDocument,
    adjust_ranges_for_edit, compute_line_starts, edit_affects_ranges, transform_edit_for_injection,
};

// Create edit module re-export for backward compatibility
pub mod edit {
    pub use crate::document::edits::*;
}
