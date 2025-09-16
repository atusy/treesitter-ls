use crate::config::TreeSitterSettings;
use crate::language::{
    ConfigStore, FiletypeResolver, LanguageLoadResult, LanguageRegistry, LogMessage,
    ParserFactory, ParserLoader, QueryStore,
};
use std::sync::{Arc, RwLock};
use tower_lsp::lsp_types::Url;

/// Coordinates between language-related modules without holding state
pub struct LanguageCoordinator {
    pub query_store: Arc<QueryStore>,
    pub config_store: Arc<ConfigStore>,
    pub filetype_resolver: Arc<FiletypeResolver>,
    pub language_registry: Arc<LanguageRegistry>,
    pub parser_loader: Arc<RwLock<ParserLoader>>,
}

impl LanguageCoordinator {
    pub fn new() -> Self {
        Self {
            query_store: Arc::new(QueryStore::new()),
            config_store: Arc::new(ConfigStore::new()),
            filetype_resolver: Arc::new(FiletypeResolver::new()),
            language_registry: Arc::new(LanguageRegistry::new()),
            parser_loader: Arc::new(RwLock::new(ParserLoader::new())),
        }
    }

    /// Initialize from TreeSitter settings
    pub fn load_settings(&self, settings: TreeSitterSettings) -> LanguageLoadResult {
        // Update configuration stores
        self.config_store.update_from_settings(&settings);
        self.filetype_resolver.build_from_settings(&settings);

        // For now, just return success
        // The actual language loading would need to be reimplemented based on the new config structure
        LanguageLoadResult::new()
            .with_log(LogMessage::info("Settings loaded successfully".to_string()))
    }

    /// Try to load a language dynamically by ID
    pub fn try_load_language_by_id(&self, language_id: &str) -> LanguageLoadResult {
        let result = LanguageLoadResult::new();

        // Check if already loaded
        if self.language_registry.get(language_id).is_some() {
            return result.with_log(LogMessage::warning(format!(
                "Language {language_id} is already loaded"
            )));
        }

        // For now, return a failure since we can't load dynamically without proper config
        result
            .with_log(LogMessage::error(format!(
                "Dynamic loading not yet implemented for language {language_id}"
            )))
            .failed()
    }

    /// Get language for a document
    pub fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        self.filetype_resolver.get_language_for_document(uri)
    }

    /// Create a parser factory that uses this coordinator's registry
    pub fn create_parser_factory(&self) -> Arc<dyn ParserFactory> {
        Arc::new(SimpleParserFactory {
            registry: self.language_registry.clone(),
        })
    }

    /// Check if a language is loaded
    pub fn is_language_loaded(&self, language_id: &str) -> bool {
        self.language_registry.get(language_id).is_some()
    }

    /// Get the filetype map (language -> extensions)
    pub fn get_filetype_map(&self) -> std::collections::HashMap<String, Vec<String>> {
        self.filetype_resolver.get_language_extensions_map()
    }


    /// Get highlights query for a language
    pub fn get_highlight_query(&self, language_id: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_highlight_query(language_id)
    }

    /// Get locals query for a language
    pub fn get_locals_query(&self, language_id: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_locals_query(language_id)
    }

    /// Check if a language has queries loaded
    pub fn has_queries(&self, language_id: &str) -> bool {
        self.query_store.has_highlight_query(language_id)
    }

    /// Get capture mappings
    pub fn get_capture_mappings(&self) -> crate::config::CaptureMappings {
        self.config_store.get_capture_mappings()
    }
}

/// Simple implementation of ParserFactory using LanguageRegistry
struct SimpleParserFactory {
    registry: Arc<LanguageRegistry>,
}

impl ParserFactory for SimpleParserFactory {
    fn create_parser(&self, language_id: &str) -> Option<tree_sitter::Parser> {
        let language = self.registry.get(language_id)?;
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language).ok()?;
        Some(parser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_coordinator() {
        let coordinator = LanguageCoordinator::new();
        assert!(!coordinator.is_language_loaded("rust"));
    }

    #[test]
    fn test_get_filetype_map() {
        let coordinator = LanguageCoordinator::new();
        // Just test that get_filetype_map returns an empty map initially
        let filetype_map = coordinator.get_filetype_map();
        assert!(filetype_map.is_empty());
    }
}