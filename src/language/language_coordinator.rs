use crate::config::{LanguageConfig, TreeSitterSettings};
use crate::language::{
    ConfigStore, FiletypeResolver, LanguageEvent, LanguageLoadResult, LanguageLoadSummary,
    LanguageLogLevel, LanguageRegistry, ParserFactory, ParserLoader, QueryLoader, QueryStore,
};
use std::sync::{Arc, RwLock};
use tree_sitter::Language;

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

    /// Initialize from TreeSitter settings and return coordination events
    pub fn load_settings(&self, settings: TreeSitterSettings) -> LanguageLoadSummary {
        // Update configuration stores
        self.config_store.update_from_settings(&settings);
        self.filetype_resolver.build_from_settings(&settings);

        // Load each language and accumulate events
        let mut summary = LanguageLoadSummary::default();

        for (lang_name, config) in &settings.languages {
            let result = self.load_single_language(lang_name, config, &settings.search_paths);
            summary.record(lang_name, result);
        }

        summary
    }

    /// Load a single language with its queries, returning emitted events
    fn load_single_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
    ) -> LanguageLoadResult {
        // Resolve library path
        let library_path =
            QueryLoader::resolve_library_path(config.library.as_ref(), lang_name, search_paths);

        let Some(lib_path) = library_path else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Error,
                format!("No library path found for language {lang_name}"),
            ));
        };

        // Load the language
        let language = {
            let result = self
                .parser_loader
                .write()
                .unwrap()
                .load_language(&lib_path, lang_name);

            match result {
                Ok(lang) => lang,
                Err(err) => {
                    return LanguageLoadResult::failure_with(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load language {lang_name}: {err}"),
                    ));
                }
            }
        };

        // Register the language
        self.language_registry
            .register_unchecked(lang_name.to_string(), language.clone());

        // Load queries and collect informational events
        let mut events = self.load_queries_for_language(lang_name, config, search_paths, &language);
        events.push(LanguageEvent::log(
            LanguageLogLevel::Info,
            format!("Language {lang_name} loaded."),
        ));

        LanguageLoadResult::success_with(events)
    }

    /// Load queries for a language and return diagnostic events
    fn load_queries_for_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        language: &Language,
    ) -> Vec<LanguageEvent> {
        let mut events = Vec::new();

        // Load highlight queries
        if !config.highlight.is_empty() {
            match QueryLoader::load_highlight_query(language, &config.highlight) {
                Ok(query) => {
                    self.query_store
                        .insert_highlight_query(lang_name.to_string(), Arc::new(query));
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Info,
                        format!("Highlight query loaded for {lang_name}"),
                    ));
                }
                Err(err) => {
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load highlight query for {lang_name}: {err}"),
                    ));
                }
            }
        } else if let Some(paths) = search_paths
            && let Ok(query) = QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "highlights.scm",
            )
        {
            self.query_store
                .insert_highlight_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Highlight query loaded from search paths for {lang_name}"),
            ));
        }

        // Load locals queries
        if let Some(locals_items) = &config.locals {
            match QueryLoader::load_highlight_query(language, locals_items) {
                Ok(query) => {
                    self.query_store
                        .insert_locals_query(lang_name.to_string(), Arc::new(query));
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Info,
                        format!("Locals query loaded for {lang_name}"),
                    ));
                }
                Err(err) => {
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load locals query for {lang_name}: {err}"),
                    ));
                }
            }
        } else if let Some(paths) = search_paths
            && let Ok(query) =
                QueryLoader::load_query_from_search_paths(language, paths, lang_name, "locals.scm")
        {
            self.query_store
                .insert_locals_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Locals query loaded from search paths for {lang_name}"),
            ));
        }

        // Load injections queries from search paths only (no inline config today)
        if let Some(paths) = search_paths
            && let Ok(query) = QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "injections.scm",
            )
        {
            self.query_store
                .insert_injections_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Injections query loaded for {lang_name}"),
            ));
        }

        events
    }

    /// Try to dynamically load a language by ID from configured search paths
    pub fn try_load_language_by_id(&self, language_id: &str) -> LanguageLoadResult {
        // Check if already loaded
        if self.language_registry.contains(language_id) {
            return LanguageLoadResult::success_with(Vec::new());
        }

        // Try to load from search paths
        let search_paths = self.config_store.get_search_paths();
        let Some(paths) = &search_paths else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Warning,
                format!("No search paths configured, cannot load language '{language_id}'"),
            ));
        };

        // Try to find the parser library
        let library_path = QueryLoader::resolve_library_path(None, language_id, &search_paths);

        let Some(lib_path) = library_path else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Warning,
                format!("Could not find parser for language '{language_id}'"),
            ));
        };

        // Load the language
        let language = {
            let result = self
                .parser_loader
                .write()
                .unwrap()
                .load_language(&lib_path, language_id);

            match result {
                Ok(lang) => lang,
                Err(err) => {
                    return LanguageLoadResult::failure_with(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load language {language_id} from {lib_path}: {err}"),
                    ));
                }
            }
        };

        // Register the language
        self.language_registry
            .register_unchecked(language_id.to_string(), language.clone());

        // Try to load queries from search paths
        let mut events = Vec::new();
        if let Ok(query) = QueryLoader::load_query_from_search_paths(
            &language,
            paths,
            language_id,
            "highlights.scm",
        ) {
            self.query_store
                .insert_highlight_query(language_id.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Dynamically loaded highlights for {language_id}"),
            ));
        }

        if let Ok(query) =
            QueryLoader::load_query_from_search_paths(&language, paths, language_id, "locals.scm")
        {
            self.query_store
                .insert_locals_query(language_id.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Dynamically loaded locals for {language_id}"),
            ));
        }

        if let Ok(query) = QueryLoader::load_query_from_search_paths(
            &language,
            paths,
            language_id,
            "injections.scm",
        ) {
            self.query_store
                .insert_injections_query(language_id.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Dynamically loaded injections for {language_id}"),
            ));
        }

        events.push(LanguageEvent::log(
            LanguageLogLevel::Info,
            format!("Dynamically loaded language {language_id} from {lib_path}"),
        ));
        if self.has_queries(language_id) {
            events.push(LanguageEvent::semantic_tokens_refresh(
                language_id.to_string(),
            ));
        }

        LanguageLoadResult::success_with(events)
    }

    /// Get language for a document path
    pub fn get_language_for_path(&self, path: &str) -> Option<String> {
        self.filetype_resolver.get_language_for_path(path)
    }

    /// Get language for a file extension
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        self.filetype_resolver.get_language_for_extension(extension)
    }

    /// Create a parser factory
    pub fn create_parser_factory(&self) -> Arc<ParserFactory> {
        Arc::new(ParserFactory::new(self.language_registry.clone()))
    }

    /// Create a parser for a specific language
    pub fn create_parser(&self, language_name: &str) -> Option<tree_sitter::Parser> {
        self.create_parser_factory().create_parser(language_name)
    }

    /// Create a document parser pool
    pub fn create_document_parser_pool(&self) -> crate::language::DocumentParserPool {
        let parser_factory = self.create_parser_factory();
        crate::language::DocumentParserPool::new(parser_factory)
    }

    /// Check if queries exist for a language
    pub fn has_queries(&self, lang_name: &str) -> bool {
        self.query_store.has_highlight_query(lang_name)
    }

    /// Get highlight query for a language
    pub fn get_highlight_query(&self, lang_name: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_highlight_query(lang_name)
    }

    /// Get locals query for a language
    pub fn get_locals_query(&self, lang_name: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_locals_query(lang_name)
    }

    /// Get capture mappings
    pub fn get_capture_mappings(&self) -> crate::config::CaptureMappings {
        self.config_store.get_capture_mappings()
    }
}

impl Default for LanguageCoordinator {
    fn default() -> Self {
        Self::new()
    }
}
