pub mod config;
pub mod handlers;
// pub mod injection; // TODO: Fix tree-sitter API compatibility
pub mod layers;
pub mod server;
pub mod state;
pub mod treesitter;

// Re-export config types for backward compatibility
pub use config::{
    CaptureMapping, CaptureMappings, HighlightItem, HighlightSource, LanguageConfig,
    TreeSitterSettings,
};

// Re-export for tests
pub use handlers::{
    ContextType, DefinitionCandidate, DefinitionResolver, LEGEND_TYPES, ReferenceContext,
};

// Re-export the main server implementation
pub use server::TreeSitterLs;
