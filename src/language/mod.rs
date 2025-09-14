pub mod injection;
pub mod loader;
pub mod parser;
pub mod query;
pub mod registry;

// Re-export key types
pub use injection::{LanguageLayer, LayerManager};
pub use loader::ParserLoader;
pub use parser::{DocumentParserPool, ParserConfig, ParserFactory};
pub use query::filter_captures;
pub use registry::LanguageRegistry;
