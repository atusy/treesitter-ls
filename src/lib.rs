pub mod analysis;
pub mod config;
pub mod document;
pub mod language;
pub mod lsp;
pub mod syntax;
pub mod workspace;

// Legacy module re-exports for backward compatibility
// These can be removed in a future version
pub mod features {
    pub use crate::analysis::*;
}
pub mod injection {
    pub use crate::document::edits as edit_transform;
    pub use crate::document::{
        InjectionPositionMapper, LayerInfo, PositionMapper, RangeMapper, SemanticTokenMapper,
        compute_line_starts,
    };
    pub use crate::language::{LanguageLayer, LayerManager};
}
pub mod text {
    pub use crate::document::*;
}

// Re-export config types for backward compatibility
pub use config::{
    CaptureMapping, CaptureMappings, HighlightItem, HighlightSource, LanguageConfig,
    TreeSitterSettings,
};

// Re-export for tests
pub use analysis::{
    ContextType, DefinitionCandidate, DefinitionResolver, LEGEND_TYPES, ReferenceContext,
};

// Re-export the main server implementation
pub use lsp::TreeSitterLs;
