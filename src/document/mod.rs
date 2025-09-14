pub mod coordinates;
pub mod edits;
pub mod store;
pub mod text;
pub mod with_layers;
pub mod with_state;

// Re-export main types
pub use coordinates::{PositionMapper, SimplePositionMapper, compute_line_starts};
pub use edits::{adjust_ranges_for_edit, edit_affects_ranges, transform_edit_for_injection};
pub use store::DocumentStore;
pub use text::TextDocument;
pub use with_layers::ParsedDocument;
pub use with_state::StatefulDocument;
