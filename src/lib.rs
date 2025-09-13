pub mod config;
pub mod document;
pub mod features;
pub mod handlers;
pub mod injection; // Facade over layers module
pub mod layers;
pub mod server;
pub mod state;
pub mod syntax;
pub mod text;
pub mod treesitter;
pub mod workspace;

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
