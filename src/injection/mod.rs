// Re-export from new locations for backward compatibility
pub use crate::document::{
    InjectionPositionMapper, LayerInfo, PositionMapper, RangeMapper, SemanticTokenMapper,
    compute_line_starts,
};
pub use crate::language::{LanguageLayer, LayerManager};

// Re-export from text module
pub use crate::text::edit as edit_transform;
