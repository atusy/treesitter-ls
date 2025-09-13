pub mod config;
pub mod document;
pub mod features;
pub mod injection;
pub mod lsp;
pub mod syntax;
pub mod text;
pub mod workspace;

// Re-export config types for backward compatibility
pub use config::{
    CaptureMapping, CaptureMappings, HighlightItem, HighlightSource, LanguageConfig,
    TreeSitterSettings,
};

// Re-export for tests
pub use features::{
    ContextType, DefinitionCandidate, DefinitionResolver, LEGEND_TYPES, ReferenceContext,
};

// Re-export the main server implementation
pub use lsp::TreeSitterLs;
