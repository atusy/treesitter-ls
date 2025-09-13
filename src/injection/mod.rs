// Facade module for language injection functionality
// Re-exports from layers module while maintaining API stability

pub use crate::layers::{
    // Core types
    LanguageLayer,
    LayerManager,

    // Mappers
    mappers::{
        injection_mapper::InjectionPositionMapper,
        range_mapper::{LayerInfo, RangeMapper},
        semantic_token_mapper::SemanticTokenMapper,
    },
};
