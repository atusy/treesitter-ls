use crate::config::{
    CaptureMappings, HighlightItem, HighlightSource, LanguageConfig, TreeSitterSettings,
};
use crate::language::{LanguageRegistry, ParserFactory, ParserLoader};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tower_lsp::Client;
use tower_lsp::lsp_types::{MessageType, Url};
use tree_sitter::{Language, Query};

pub struct LanguageService {
    pub language_registry: Arc<LanguageRegistry>,
    pub queries: Mutex<HashMap<String, Query>>,
    pub locals_queries: Mutex<HashMap<String, Query>>,
    pub injections_queries: Mutex<HashMap<String, Query>>,
    pub language_configs: Mutex<HashMap<String, LanguageConfig>>,
    pub filetype_map: Mutex<HashMap<String, String>>,
    pub library_loader: Mutex<ParserLoader>,
    pub capture_mappings: Mutex<CaptureMappings>,
    pub search_paths: Mutex<Option<Vec<String>>>,
    // NOTE: Parser management moved to DocumentParserPool (Step 3-2)
}

impl Default for LanguageService {
    fn default() -> Self {
        Self {
            language_registry: Arc::new(LanguageRegistry::new()),
            queries: Mutex::new(HashMap::new()),
            locals_queries: Mutex::new(HashMap::new()),
            injections_queries: Mutex::new(HashMap::new()),
            language_configs: Mutex::new(HashMap::new()),
            filetype_map: Mutex::new(HashMap::new()),
            library_loader: Mutex::new(ParserLoader::new()),
            capture_mappings: Mutex::new(CaptureMappings::default()),
            search_paths: Mutex::new(None),
        }
    }
}

impl LanguageService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a ParserFactory that can create parsers for loaded languages
    pub fn create_parser_factory(&self) -> Arc<ParserFactory> {
        Arc::new(ParserFactory::new(self.language_registry.clone()))
    }

    pub fn load_language(
        &self,
        path: &str,
        lang_name: &str,
    ) -> std::result::Result<Language, String> {
        self.library_loader
            .lock()
            .unwrap()
            .load_language(path, lang_name)
            .map_err(|e| e.to_string())
    }

    pub fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        let extension = uri.path().split('.').next_back().unwrap_or("");
        let filetype_map = self.filetype_map.lock().unwrap();
        filetype_map.get(extension).cloned()
    }

    pub fn resolve_library_path(
        &self,
        config: &LanguageConfig,
        language: &str,
        search_paths: &Option<Vec<String>>,
    ) -> Option<String> {
        // If explicit library path is provided, use it
        if let Some(library) = &config.library {
            return Some(library.clone());
        }

        // Otherwise, search in searchPaths: <base>/parser/
        if let Some(paths) = search_paths {
            for path in paths {
                // Try .so extension first (Linux)
                let so_path = format!("{path}/parser/{language}.so");
                if std::path::Path::new(&so_path).exists() {
                    return Some(so_path);
                }

                // Try .dylib extension (macOS)
                let dylib_path = format!("{path}/parser/{language}.dylib");
                if std::path::Path::new(&dylib_path).exists() {
                    return Some(dylib_path);
                }
            }
        }

        None
    }

    pub fn load_query_from_highlight(
        &self,
        highlight_items: &[HighlightItem],
    ) -> std::result::Result<String, String> {
        let mut combined_query = String::new();

        for item in highlight_items {
            match &item.source {
                HighlightSource::Path { path } => match std::fs::read_to_string(path) {
                    Ok(content) => {
                        combined_query.push_str(&content);
                        combined_query.push('\n');
                    }
                    Err(e) => {
                        return Err(format!("Failed to read query file {path}: {e}"));
                    }
                },
                HighlightSource::Query { query } => {
                    combined_query.push_str(query);
                    combined_query.push('\n');
                }
            }
        }

        Ok(combined_query)
    }

    pub async fn load_settings(&self, settings: TreeSitterSettings, client: &Client) {
        // Store language configs
        self.update_language_configs(&settings);

        // Build filetype map
        self.build_filetype_map(&settings);

        // Store capture mappings
        *self.capture_mappings.lock().unwrap() = settings.capture_mappings.clone();

        // Store search paths for dynamic loading
        self.store_search_paths(settings.search_paths.clone());

        // Load languages and queries
        for (lang_name, config) in &settings.languages {
            self.load_single_language(lang_name, config, &settings.search_paths, client)
                .await;
        }
    }

    fn update_language_configs(&self, settings: &TreeSitterSettings) {
        *self.language_configs.lock().unwrap() = settings.languages.clone();
    }

    fn build_filetype_map(&self, settings: &TreeSitterSettings) {
        let mut filetype_map = self.filetype_map.lock().unwrap();
        for (language, config) in &settings.languages {
            for ext in &config.filetypes {
                filetype_map.insert(ext.clone(), language.clone());
            }
        }
    }

    fn store_search_paths(&self, search_paths: Option<Vec<String>>) {
        *self.search_paths.lock().unwrap() = search_paths;
    }

    async fn load_single_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        client: &Client,
    ) {
        let library_path = self.resolve_library_path(config, lang_name, search_paths);

        match library_path {
            Some(lib_path) => match self.load_language(&lib_path, lang_name) {
                Ok(language) => {
                    // Load highlight queries (explicit or via searchPaths)
                    self.load_highlight_queries(lang_name, config, search_paths, &language, client)
                        .await;

                    // Load locals queries (explicit or via searchPaths)
                    self.load_locals_queries_auto(
                        lang_name,
                        config,
                        search_paths,
                        &language,
                        client,
                    )
                    .await;

                    // Store the language
                    self.language_registry
                        .register(lang_name.to_string(), language);

                    client
                        .log_message(MessageType::INFO, format!("Language {lang_name} loaded."))
                        .await;
                }
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load language {lang_name}: {err}"),
                        )
                        .await;
                }
            },
            None => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("No library path found for language {lang_name}: neither explicit library path nor any valid entry in 'searchPaths'"),
                    )
                    .await;
            }
        }
    }

    async fn load_highlight_queries(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        language: &Language,
        client: &Client,
    ) {
        // Prefer explicit per-language highlights if provided
        if !config.highlight.is_empty() {
            match self.load_query_from_highlight(&config.highlight) {
                Ok(combined_query) => match Query::new(language, &combined_query) {
                    Ok(query) => {
                        self.queries
                            .lock()
                            .unwrap()
                            .insert(lang_name.to_string(), query);
                        client
                            .log_message(
                                MessageType::INFO,
                                format!("Query loaded for {lang_name}."),
                            )
                            .await;
                    }
                    Err(err) => {
                        client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to parse query for {lang_name}: {err}"),
                            )
                            .await;
                    }
                },
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to load highlight queries for {lang_name}: {err}"),
                        )
                        .await;
                }
            }
            return;
        }

        // Otherwise, try to load from searchPaths: <base>/queries/<lang_name>/highlights.scm
        let mut loaded_from_search = false;
        if let Some(runtime_bases) = search_paths
            && let Some(path) = self.find_query_file(runtime_bases, lang_name, "highlights.scm")
        {
            match fs::read_to_string(&path) {
                Ok(content) => match Query::new(language, &content) {
                    Ok(query) => {
                        self.queries
                            .lock()
                            .unwrap()
                            .insert(lang_name.to_string(), query);
                        client
                            .log_message(
                                MessageType::INFO,
                                format!("Query loaded from {path:?} for {lang_name}."),
                            )
                            .await;
                        loaded_from_search = true;
                    }
                    Err(err) => {
                        client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to parse highlight query for {lang_name} from {path:?}: {err}"),
                            )
                            .await;
                    }
                },
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to read highlight query {path:?}: {err}"),
                        )
                        .await;
                }
            }
        }

        if !loaded_from_search {
            client
                .log_message(
                    MessageType::ERROR,
                    format!("No highlight queries provided for {lang_name}: neither per-language 'highlight' nor any file under 'searchPaths' yielded a file"),
                )
                .await;
        }
    }

    async fn load_locals_queries(
        &self,
        lang_name: &str,
        locals_sources: &[HighlightItem],
        language: &Language,
        client: &Client,
    ) {
        match self.load_query_from_highlight(locals_sources) {
            Ok(combined_locals_query) => match Query::new(language, &combined_locals_query) {
                Ok(locals_query) => {
                    self.locals_queries
                        .lock()
                        .unwrap()
                        .insert(lang_name.to_string(), locals_query);
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Locals query loaded for {lang_name}."),
                        )
                        .await;
                }
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to parse locals query for {lang_name}: {err}"),
                        )
                        .await;
                }
            },
            Err(err) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to load locals queries for {lang_name}: {err}"),
                    )
                    .await;
            }
        }
    }

    async fn load_locals_queries_auto(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        language: &Language,
        client: &Client,
    ) {
        if let Some(locals_sources) = &config.locals {
            self.load_locals_queries(lang_name, locals_sources, language, client)
                .await;
            return;
        }

        if let Some(runtime_bases) = search_paths
            && let Some(path) = self.find_query_file(runtime_bases, lang_name, "locals.scm")
        {
            match fs::read_to_string(&path) {
                Ok(content) => match Query::new(language, &content) {
                    Ok(query) => {
                        self.locals_queries
                            .lock()
                            .unwrap()
                            .insert(lang_name.to_string(), query);
                        client
                            .log_message(
                                MessageType::INFO,
                                format!("Locals query loaded from {path:?} for {lang_name}."),
                            )
                            .await;
                    }
                    Err(err) => {
                        client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to parse locals query for {lang_name} from {path:?}: {err}"),
                            )
                            .await;
                    }
                },
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to read locals query {path:?}: {err}"),
                        )
                        .await;
                }
            }
        }
        // No locals is acceptable; silently skip

        // Load injections query from searchPaths
        if let Some(runtime_bases) = search_paths
            && let Some(path) = self.find_query_file(runtime_bases, lang_name, "injections.scm")
        {
            match fs::read_to_string(&path) {
                Ok(content) => match Query::new(language, &content) {
                    Ok(query) => {
                        self.injections_queries
                            .lock()
                            .unwrap()
                            .insert(lang_name.to_string(), query);
                        client
                            .log_message(
                                MessageType::INFO,
                                format!("Injections query loaded from {path:?} for {lang_name}."),
                            )
                            .await;
                    }
                    Err(err) => {
                        client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to parse injections query for {lang_name} from {path:?}: {err}"),
                            )
                            .await;
                    }
                },
                Err(err) => {
                    client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to read injections query {path:?}: {err}"),
                        )
                        .await;
                }
            }
        }
        // No injections is acceptable; silently skip
    }

    fn find_query_file(
        &self,
        runtime_bases: &[String],
        lang_name: &str,
        file_name: &str,
    ) -> Option<PathBuf> {
        for base in runtime_bases {
            let candidate = Path::new(base)
                .join("queries")
                .join(lang_name)
                .join(file_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    pub async fn try_load_language_by_id(&self, language_id: &str, client: &Client) -> bool {
        // Check if already loaded
        if self.language_registry.contains(language_id) {
            return true;
        }

        // Try to load from search paths
        let search_paths = self.search_paths.lock().unwrap().clone();
        if let Some(paths) = &search_paths {
            // Try to find the parser library
            for path in paths {
                // Try .so extension first (Linux)
                let so_path = format!("{path}/parser/{language_id}.so");
                if std::path::Path::new(&so_path).exists() {
                    return self
                        .load_language_from_path(language_id, &so_path, &search_paths, client)
                        .await;
                }

                // Try .dylib extension (macOS)
                let dylib_path = format!("{path}/parser/{language_id}.dylib");
                if std::path::Path::new(&dylib_path).exists() {
                    return self
                        .load_language_from_path(language_id, &dylib_path, &search_paths, client)
                        .await;
                }
            }
        }

        client
            .log_message(
                MessageType::WARNING,
                format!("Could not find parser for language '{language_id}'"),
            )
            .await;
        false
    }

    async fn load_language_from_path(
        &self,
        lang_name: &str,
        lib_path: &str,
        search_paths: &Option<Vec<String>>,
        client: &Client,
    ) -> bool {
        match self.load_language(lib_path, lang_name) {
            Ok(language) => {
                // Try to load highlight queries from searchPaths
                if let Some(runtime_bases) = search_paths
                    && let Some(path) =
                        self.find_query_file(runtime_bases, lang_name, "highlights.scm")
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(query) = Query::new(&language, &content)
                {
                    self.queries
                        .lock()
                        .unwrap()
                        .insert(lang_name.to_string(), query);
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Dynamically loaded highlights for {lang_name}"),
                        )
                        .await;
                }

                // Try to load locals queries from searchPaths
                if let Some(runtime_bases) = search_paths
                    && let Some(path) = self.find_query_file(runtime_bases, lang_name, "locals.scm")
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(query) = Query::new(&language, &content)
                {
                    self.locals_queries
                        .lock()
                        .unwrap()
                        .insert(lang_name.to_string(), query);
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Dynamically loaded locals for {lang_name}"),
                        )
                        .await;
                }

                // Try to load injections queries from searchPaths
                if let Some(runtime_bases) = search_paths
                    && let Some(path) =
                        self.find_query_file(runtime_bases, lang_name, "injections.scm")
                    && let Ok(content) = fs::read_to_string(&path)
                    && let Ok(query) = Query::new(&language, &content)
                {
                    self.injections_queries
                        .lock()
                        .unwrap()
                        .insert(lang_name.to_string(), query);
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Dynamically loaded injections for {lang_name}"),
                        )
                        .await;
                }

                // Store the language
                self.language_registry
                    .register(lang_name.to_string(), language);

                client
                    .log_message(
                        MessageType::INFO,
                        format!("Dynamically loaded language {lang_name} from {lib_path}"),
                    )
                    .await;

                // Request semantic tokens refresh after successful loading
                if client.semantic_tokens_refresh().await.is_ok() {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!("Requested semantic tokens refresh for {lang_name}"),
                        )
                        .await;
                }

                true
            }
            Err(err) => {
                client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to load language {lang_name} from {lib_path}: {err}"),
                    )
                    .await;
                false
            }
        }
    }
}
