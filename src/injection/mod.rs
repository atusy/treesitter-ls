pub mod language_layer;
pub mod layer_manager;
pub mod mappers;

// Re-export commonly used types
pub use language_layer::LanguageLayer;
pub use layer_manager::LayerManager;
pub use mappers::{
    injection_mapper::InjectionPositionMapper,
    range_mapper::{LayerInfo, RangeMapper},
    semantic_token_mapper::SemanticTokenMapper,
};

// Re-export from text module
pub use crate::text::edit as edit_transform;
pub use crate::text::position::{PositionMapper, compute_line_starts};
