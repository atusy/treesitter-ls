use super::config_store::ConfigStore;
use super::events::{LanguageEvent, LanguageLoadResult, LanguageLoadSummary, LanguageLogLevel};
use super::filetypes::FiletypeResolver;
use super::loader::ParserLoader;
use super::parser_pool::{DocumentParserPool, ParserFactory};
use super::query_loader::QueryLoader;
use super::query_store::QueryStore;
use super::registry::LanguageRegistry;
use crate::config::settings::LanguageConfig;
use crate::config::{CaptureMappings, TreeSitterSettings, WorkspaceSettings};
use std::sync::{Arc, RwLock};
use tree_sitter::Language;

/// Coordinates language runtime components (registry, queries, configs).
pub struct LanguageCoordinator {
    query_store: QueryStore,
    config_store: ConfigStore,
    filetype_resolver: FiletypeResolver,
    language_registry: LanguageRegistry,
    parser_loader: RwLock<ParserLoader>,
}

impl Default for LanguageCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageCoordinator {
    pub fn new() -> Self {
        Self {
            query_store: QueryStore::new(),
            config_store: ConfigStore::new(),
            filetype_resolver: FiletypeResolver::new(),
            language_registry: LanguageRegistry::new(),
            parser_loader: RwLock::new(ParserLoader::new()),
        }
    }

    pub fn ensure_language_loaded(&self, language_id: &str) -> LanguageLoadResult {
        if self.language_registry.contains(language_id) {
            LanguageLoadResult::success_with(Vec::new())
        } else {
            self.try_load_language_by_id(language_id)
        }
    }

    /// Initialize from workspace-level settings and return coordination events
    pub fn load_settings(&self, settings: WorkspaceSettings) -> LanguageLoadSummary {
        let config_settings: TreeSitterSettings = settings.into();
        self.load_settings_from_config(&config_settings)
    }

    fn load_settings_from_config(&self, settings: &TreeSitterSettings) -> LanguageLoadSummary {
        self.config_store.update_from_settings(settings);
        self.filetype_resolver.build_from_settings(settings);

        let mut summary = LanguageLoadSummary::default();
        for (lang_name, config) in &settings.languages {
            let result = self.load_single_language(lang_name, config, &settings.search_paths);
            summary.record(lang_name, result);
        }
        summary
    }

    /// Try to dynamically load a language by ID from configured search paths
    pub fn try_load_language_by_id(&self, language_id: &str) -> LanguageLoadResult {
        if self.language_registry.contains(language_id) {
            return LanguageLoadResult::success_with(Vec::new());
        }

        let search_paths = self.config_store.get_search_paths();
        let Some(paths) = &search_paths else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Warning,
                format!("No search paths configured, cannot load language '{language_id}'"),
            ));
        };

        let library_path = QueryLoader::resolve_library_path(None, language_id, &search_paths);
        let Some(lib_path) = library_path else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Warning,
                format!("Could not find parser for language '{language_id}'"),
            ));
        };

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

        self.language_registry
            .register_unchecked(language_id.to_string(), language.clone());

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
                .insert_injection_query(language_id.to_string(), Arc::new(query));
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

    /// Get configured search paths (primarily for testing and diagnostics).
    pub fn get_search_paths(&self) -> Option<Vec<String>> {
        self.config_store.get_search_paths()
    }

    /// Check if a language is already loaded in the registry
    pub fn is_language_loaded(&self, language_id: &str) -> bool {
        self.language_registry.contains(language_id)
    }

    /// Create a document parser pool
    pub fn create_document_parser_pool(&self) -> DocumentParserPool {
        let parser_factory = ParserFactory::new(self.language_registry.clone());
        DocumentParserPool::new(parser_factory)
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

    pub fn get_injection_query(&self, lang_name: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_injection_query(lang_name)
    }

    /// Get capture mappings
    pub fn get_capture_mappings(&self) -> CaptureMappings {
        let config_mappings = self.config_store.get_capture_mappings();
        config_mappings
            .iter()
            .map(|(lang, mappings)| (lang.clone(), mappings.clone()))
            .collect::<CaptureMappings>()
    }

    fn load_single_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
    ) -> LanguageLoadResult {
        let library_path =
            QueryLoader::resolve_library_path(config.library.as_ref(), lang_name, search_paths);
        let Some(lib_path) = library_path else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Error,
                format!("No library path found for language {lang_name}"),
            ));
        };

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

        self.language_registry
            .register_unchecked(lang_name.to_string(), language.clone());

        let mut events = self.load_queries_for_language(lang_name, config, search_paths, &language);
        events.push(LanguageEvent::log(
            LanguageLogLevel::Info,
            format!("Language {lang_name} loaded."),
        ));
        LanguageLoadResult::success_with(events)
    }

    fn load_queries_for_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        language: &Language,
    ) -> Vec<LanguageEvent> {
        let mut events = Vec::new();

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

        // Load injection queries from search paths if available
        if let Some(paths) = search_paths
            && let Ok(query) = QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "injections.scm",
            )
        {
            self.query_store
                .insert_injection_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Injection query loaded from search paths for {lang_name}"),
            ));
        }

        events
    }

    /// Register a language directly for testing purposes.
    ///
    /// This bypasses the normal loading process and directly registers
    /// a tree-sitter Language in the registry. Useful for unit tests
    /// that need to test with specific language parsers.
    #[cfg(test)]
    pub fn register_language_for_test(&self, language_id: &str, language: tree_sitter::Language) {
        self.language_registry
            .register_unchecked(language_id.to_string(), language);
    }
}
