pub mod config;
pub mod features;
pub mod injection;
pub mod server;
pub mod syntax;
pub mod text;
pub mod workspace;

// Internal modules (not part of public API)
#[doc(hidden)]
pub mod document;
#[doc(hidden)]
pub mod layers;
#[doc(hidden)]
pub mod treesitter;

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
pub use server::TreeSitterLs;
