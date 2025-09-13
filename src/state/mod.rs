pub mod document;
pub mod language_service;
pub mod parser_pool;

// Re-export layer-related types from layers module for backward compatibility
pub use crate::injection::{LanguageLayer, LayerManager};

pub use document::DocumentStore;
pub use language_service::LanguageService;
pub use parser_pool::{DocumentParserPool, ParserFactory};
