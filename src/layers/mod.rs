pub mod language_layer;
pub mod layer_manager;
pub mod mappers;

// Re-export commonly used types
pub use crate::text::position::{PositionMapper, compute_line_starts};
pub use language_layer::LanguageLayer;
pub use layer_manager::LayerManager;

// Re-export edit_transform from text module
pub use crate::text::edit as edit_transform;
