use crate::config::{LanguageConfig, TreeSitterSettings};
use crate::language::{
    ConfigStore, FiletypeResolver, LanguageRegistry, ParserFactory, ParserLoader, QueryLoader,
    QueryStore,
};
use std::sync::{Arc, RwLock};
use tower_lsp::Client;
use tower_lsp::lsp_types::{MessageType, Url};
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

    /// Initialize from TreeSitter settings
    pub async fn load_settings(&self, settings: TreeSitterSettings, client: &Client) {
        // Update configuration stores
        self.config_store.update_from_settings(&settings);
        self.filetype_resolver.build_from_settings(&settings);

        // Load each language
        for (lang_name, config) in &settings.languages {
            self.load_single_language(lang_name, config, &settings.search_paths, client)
                .await;
        }
    }

    /// Load a single language with its queries
    async fn load_single_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        client: &Client,
    ) {
        // Resolve library path
        let library_path =
            QueryLoader::resolve_library_path(config.library.as_ref(), lang_name, search_paths);

        let Some(lib_path) = library_path else {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("No library path found for language {lang_name}"),
                )
                .await;
            return;
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
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load language {lang_name}: {err}"),
                        )
                        .await;
                    return;
                }
            }
        };

        // Register the language
        self.language_registry
            .register(lang_name.to_string(), language.clone());

        // Load queries
        self.load_queries_for_language(lang_name, config, search_paths, &language, client)
            .await;

        client
            .log_message(MessageType::INFO, format!("Language {lang_name} loaded."))
            .await;
    }

    /// Load queries for a language
    async fn load_queries_for_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        language: &Language,
        client: &Client,
    ) {
        // Load highlight queries
        if !config.highlight.is_empty() {
            match QueryLoader::load_highlight_query(language, &config.highlight) {
                Ok(query) => {
                    self.query_store
                        .insert_highlight_query(lang_name.to_string(), Arc::new(query));
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Highlight query loaded for {lang_name}"),
                        )
                        .await;
                }
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load highlight query for {lang_name}: {err}"),
                        )
                        .await;
                }
            }
        } else if let Some(paths) = search_paths {
            // Try to load from search paths
            match QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "highlights.scm",
            ) {
                Ok(query) => {
                    self.query_store
                        .insert_highlight_query(lang_name.to_string(), Arc::new(query));
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Highlight query loaded from search paths for {lang_name}"),
                        )
                        .await;
                }
                Err(_) => {
                    // Highlight queries are optional
                }
            }
        }

        // Load locals queries
        if let Some(locals_items) = &config.locals {
            match QueryLoader::load_highlight_query(language, locals_items) {
                Ok(query) => {
                    self.query_store
                        .insert_locals_query(lang_name.to_string(), Arc::new(query));
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Locals query loaded for {lang_name}"),
                        )
                        .await;
                }
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load locals query for {lang_name}: {err}"),
                        )
                        .await;
                }
            }
        } else if let Some(paths) = search_paths {
            // Try to load from search paths
            match QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "locals.scm",
            ) {
                Ok(query) => {
                    self.query_store
                        .insert_locals_query(lang_name.to_string(), Arc::new(query));
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Locals query loaded from search paths for {lang_name}"),
                        )
                        .await;
                }
                Err(_) => {
                    // Locals queries are optional
                }
            }
        }

        // Load injections queries from search paths
        if let Some(paths) = search_paths {
            match QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "injections.scm",
            ) {
                Ok(query) => {
                    self.query_store
                        .insert_injections_query(lang_name.to_string(), Arc::new(query));
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Injections query loaded for {lang_name}"),
                        )
                        .await;
                }
                Err(_) => {
                    // Injections queries are optional
                }
            }
        }
    }

    /// Try to dynamically load a language by ID
    pub async fn try_load_language_by_id(&self, language_id: &str, client: &Client) -> bool {
        // Check if already loaded
        if self.language_registry.contains(language_id) {
            return true;
        }

        // Try to load from search paths
        let search_paths = self.config_store.get_search_paths();
        let Some(paths) = &search_paths else {
            client
                .log_message(
                    MessageType::WARNING,
                    format!("No search paths configured, cannot load language '{language_id}'"),
                )
                .await;
            return false;
        };

        // Try to find the parser library
        let library_path = QueryLoader::resolve_library_path(None, language_id, &search_paths);

        let Some(lib_path) = library_path else {
            client
                .log_message(
                    MessageType::WARNING,
                    format!("Could not find parser for language '{language_id}'"),
                )
                .await;
            return false;
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
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load language {language_id} from {lib_path}: {err}"),
                        )
                        .await;
                    return false;
                }
            }
        };

        // Register the language
        self.language_registry
            .register(language_id.to_string(), language.clone());

        // Try to load queries from search paths
        if let Ok(query) = QueryLoader::load_query_from_search_paths(
            &language,
            paths,
            language_id,
            "highlights.scm",
        ) {
            self.query_store
                .insert_highlight_query(language_id.to_string(), Arc::new(query));
            client
                .log_message(
                    MessageType::INFO,
                    format!("Dynamically loaded highlights for {language_id}"),
                )
                .await;
        }

        if let Ok(query) =
            QueryLoader::load_query_from_search_paths(&language, paths, language_id, "locals.scm")
        {
            self.query_store
                .insert_locals_query(language_id.to_string(), Arc::new(query));
            client
                .log_message(
                    MessageType::INFO,
                    format!("Dynamically loaded locals for {language_id}"),
                )
                .await;
        }

        if let Ok(query) = QueryLoader::load_query_from_search_paths(
            &language,
            paths,
            language_id,
            "injections.scm",
        ) {
            self.query_store
                .insert_injections_query(language_id.to_string(), Arc::new(query));
            client
                .log_message(
                    MessageType::INFO,
                    format!("Dynamically loaded injections for {language_id}"),
                )
                .await;
        }

        client
            .log_message(
                MessageType::INFO,
                format!("Dynamically loaded language {language_id} from {lib_path}"),
            )
            .await;

        // Request semantic tokens refresh after successful loading
        if client.semantic_tokens_refresh().await.is_ok() {
            client
                .log_message(
                    MessageType::INFO,
                    format!("Requested semantic tokens refresh for {language_id}"),
                )
                .await;
        }

        true
    }

    /// Get language for a document
    pub fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        self.filetype_resolver.get_language_for_document(uri)
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

    /// Get filetype map
    pub fn get_filetype_map(&self) -> std::collections::HashMap<String, String> {
        self.filetype_resolver.get_filetype_map()
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
