use crate::analysis::ParserLoader;
use crate::config::{HighlightItem, HighlightSource, LanguageConfig, TreeSitterSettings};
use std::collections::HashMap;
use std::sync::Mutex;
use tower_lsp::Client;
use tower_lsp::lsp_types::{MessageType, Url};
use tree_sitter::{Language, Query};

pub struct LanguageService {
    pub languages: Mutex<HashMap<String, Language>>,
    pub queries: Mutex<HashMap<String, Query>>,
    pub locals_queries: Mutex<HashMap<String, Query>>,
    pub language_configs: Mutex<HashMap<String, LanguageConfig>>,
    pub filetype_map: Mutex<HashMap<String, String>>,
    pub library_loader: Mutex<ParserLoader>,
}

impl Default for LanguageService {
    fn default() -> Self {
        Self {
            languages: Mutex::new(HashMap::new()),
            queries: Mutex::new(HashMap::new()),
            locals_queries: Mutex::new(HashMap::new()),
            language_configs: Mutex::new(HashMap::new()),
            filetype_map: Mutex::new(HashMap::new()),
            library_loader: Mutex::new(ParserLoader::new()),
        }
    }
}

impl LanguageService {
    pub fn new() -> Self {
        Self::default()
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
        runtimepath: &Option<Vec<String>>,
    ) -> Option<String> {
        // If explicit library path is provided, use it
        if let Some(library) = &config.library {
            return Some(library.clone());
        }

        // Otherwise, search in runtimepath
        if let Some(paths) = runtimepath {
            for path in paths {
                // Try .so extension first (Linux)
                let so_path = format!("{path}/{language}.so");
                if std::path::Path::new(&so_path).exists() {
                    return Some(so_path);
                }

                // Try .dylib extension (macOS)
                let dylib_path = format!("{path}/{language}.dylib");
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

        // Load languages and queries
        for (lang_name, config) in &settings.languages {
            self.load_single_language(lang_name, config, &settings.runtimepath, client)
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

    async fn load_single_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        runtimepath: &Option<Vec<String>>,
        client: &Client,
    ) {
        let library_path = self.resolve_library_path(config, lang_name, runtimepath);

        match library_path {
            Some(lib_path) => match self.load_language(&lib_path, lang_name) {
                Ok(language) => {
                    // Load highlight queries
                    self.load_highlight_queries(lang_name, config, &language, client)
                        .await;

                    // Load locals queries if available
                    if let Some(locals_sources) = &config.locals {
                        self.load_locals_queries(lang_name, locals_sources, &language, client)
                            .await;
                    }

                    // Store the language
                    self.languages
                        .lock()
                        .unwrap()
                        .insert(lang_name.to_string(), language);

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
                        format!("No library path found for language {lang_name}: neither explicit library path nor valid runtimepath entry"),
                    )
                    .await;
            }
        }
    }

    async fn load_highlight_queries(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        language: &Language,
        client: &Client,
    ) {
        match self.load_query_from_highlight(&config.highlight) {
            Ok(combined_query) => match Query::new(language, &combined_query) {
                Ok(query) => {
                    self.queries
                        .lock()
                        .unwrap()
                        .insert(lang_name.to_string(), query);
                    client
                        .log_message(MessageType::INFO, format!("Query loaded for {lang_name}."))
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
}
