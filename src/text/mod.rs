pub mod document;
pub mod edit;
pub mod with_layers;
pub mod with_state;

// Re-export commonly used types
pub use document::TextDocument;
// Re-export from document module for backward compatibility
pub use crate::document::coordinates::{PositionMapper, SimplePositionMapper, compute_line_starts};
pub use with_layers::ParsedDocument;
pub use with_state::StatefulDocument;
