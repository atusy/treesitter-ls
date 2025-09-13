// Re-export layer-related types from layers module for backward compatibility
pub use crate::injection::{LanguageLayer, LayerManager};

// Re-export from new modules
pub use crate::syntax::parser_pool::{DocumentParserPool, ParserFactory};
pub use crate::workspace::documents::{Document, DocumentStore};
pub use crate::workspace::languages::LanguageService;
