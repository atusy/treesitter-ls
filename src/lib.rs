mod analysis;
pub mod config;
pub mod handlers;
pub mod server;
pub mod state;
pub mod utils;

// Re-export config types for backward compatibility
pub use config::{HighlightItem, HighlightSource, LanguageConfig, TreeSitterSettings};

// Re-export for tests
pub use handlers::{
    ContextType, DefinitionCandidate, DefinitionResolver, LEGEND_TYPES, ReferenceContext,
};

// Re-export the main server implementation
pub use server::TreeSitterLs;

