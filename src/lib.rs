use dashmap::DashMap;
use serde::Deserialize;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator, Tree};

pub mod handlers;
mod safe_library_loader;

use handlers::DefinitionResolver;
use safe_library_loader::LibraryLoader;

pub const LEGEND_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::REGEXP,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::CLASS,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::ENUM,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::MACRO,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::EVENT,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::DECORATOR,
];

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct HighlightItem {
    #[serde(flatten)]
    pub source: HighlightSource,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum HighlightSource {
    Path { path: String },
    Query { query: String },
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LanguageConfig {
    pub library: Option<String>,
    pub filetypes: Vec<String>,
    pub highlight: Vec<HighlightItem>,
    pub locals: Option<Vec<HighlightItem>>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct TreeSitterSettings {
    pub runtimepath: Option<Vec<String>>,
    pub languages: std::collections::HashMap<String, LanguageConfig>,
}

pub struct TreeSitterLs {
    client: Client,
    languages: std::sync::Mutex<std::collections::HashMap<String, Language>>,
    queries: std::sync::Mutex<std::collections::HashMap<String, Query>>,
    locals_queries: std::sync::Mutex<std::collections::HashMap<String, Query>>,
    language_configs: std::sync::Mutex<std::collections::HashMap<String, LanguageConfig>>,
    filetype_map: std::sync::Mutex<std::collections::HashMap<String, String>>,
    library_loader: std::sync::Mutex<LibraryLoader>,
    document_map: DashMap<Url, (String, Option<Tree>)>,
    definition_resolver: DefinitionResolver,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("document_map", &self.document_map)
            .finish_non_exhaustive()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            languages: std::sync::Mutex::new(std::collections::HashMap::new()),
            queries: std::sync::Mutex::new(std::collections::HashMap::new()),
            locals_queries: std::sync::Mutex::new(std::collections::HashMap::new()),
            language_configs: std::sync::Mutex::new(std::collections::HashMap::new()),
            filetype_map: std::sync::Mutex::new(std::collections::HashMap::new()),
            library_loader: std::sync::Mutex::new(LibraryLoader::new()),
            document_map: DashMap::new(),
            definition_resolver: DefinitionResolver::new(),
        }
    }

    async fn parse_document(&self, uri: Url, text: String) {
        // Detect file extension
        let extension = uri.path().split('.').next_back().unwrap_or("");

        // Find the language for this file extension
        let filetype_map = self.filetype_map.lock().unwrap();
        let language_name = filetype_map.get(extension);

        if let Some(language_name) = language_name {
            let languages = self.languages.lock().unwrap();
            if let Some(language) = languages.get(language_name) {
                let mut parser = Parser::new();
                if parser.set_language(language).is_ok() {
                    if let Some(tree) = parser.parse(&text, None) {
                        self.document_map.insert(uri, (text, Some(tree)));
                        return;
                    }
                }
            }
        }

        self.document_map.insert(uri, (text, None));
    }

    fn load_language(
        &self,
        path: &str,
        func_name: &str,
        lang_name: &str,
    ) -> std::result::Result<Language, String> {
        // Use the library loader to load a language.
        self.library_loader
            .lock()
            .unwrap()
            .load_language(path, func_name, lang_name)
            .map_err(|e| e.to_string())
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        let extension = uri.path().split('.').next_back().unwrap_or("");
        let filetype_map = self.filetype_map.lock().unwrap();
        filetype_map.get(extension).cloned()
    }

    fn resolve_library_path(
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
                let so_path = format!("{}/{}.so", path, language);
                if std::path::Path::new(&so_path).exists() {
                    return Some(so_path);
                }

                // Try .dylib extension (macOS)
                let dylib_path = format!("{}/{}.dylib", path, language);
                if std::path::Path::new(&dylib_path).exists() {
                    return Some(dylib_path);
                }
            }
        }

        None
    }

    fn load_query_from_highlight(
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
                        return Err(format!("Failed to read query file {}: {}", path, e));
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

    async fn load_settings(&self, settings: TreeSitterSettings) {
        // Store language configs
        {
            *self.language_configs.lock().unwrap() = settings.languages.clone();
        }

        // Build filetype map
        {
            let mut filetype_map = self.filetype_map.lock().unwrap();
            for (language, config) in &settings.languages {
                for ext in &config.filetypes {
                    filetype_map.insert(ext.clone(), language.clone());
                }
            }
        }

        // Load languages and queries
        for (lang_name, config) in &settings.languages {
            // For now, assume the function name is tree_sitter_<language>
            let func_name = format!("tree_sitter_{}", lang_name);

            // Resolve library path
            let library_path = self.resolve_library_path(config, lang_name, &settings.runtimepath);

            match library_path {
                Some(lib_path) => match self.load_language(&lib_path, &func_name, lang_name) {
                    Ok(language) => {
                        match self.load_query_from_highlight(&config.highlight) {
                            Ok(combined_query) => match Query::new(&language, &combined_query) {
                                Ok(query) => {
                                    {
                                        self.queries
                                            .lock()
                                            .unwrap()
                                            .insert(lang_name.to_string(), query);
                                    }
                                    self.client
                                        .log_message(
                                            MessageType::INFO,
                                            format!("Query loaded for {}.", lang_name),
                                        )
                                        .await;
                                }
                                Err(err) => {
                                    self.client
                                        .log_message(
                                            MessageType::ERROR,
                                            format!(
                                                "Failed to parse query for {}: {}",
                                                lang_name, err
                                            ),
                                        )
                                        .await;
                                }
                            },
                            Err(err) => {
                                self.client
                                    .log_message(
                                        MessageType::ERROR,
                                        format!(
                                            "Failed to load highlight queries for {}: {}",
                                            lang_name, err
                                        ),
                                    )
                                    .await;
                            }
                        }

                        // Load locals queries if available
                        if let Some(locals_sources) = &config.locals {
                            match self.load_query_from_highlight(locals_sources) {
                                Ok(combined_locals_query) => {
                                    match Query::new(&language, &combined_locals_query) {
                                        Ok(locals_query) => {
                                            {
                                                self.locals_queries
                                                    .lock()
                                                    .unwrap()
                                                    .insert(lang_name.to_string(), locals_query);
                                            }
                                            self.client
                                                .log_message(
                                                    MessageType::INFO,
                                                    format!(
                                                        "Locals query loaded for {}.",
                                                        lang_name
                                                    ),
                                                )
                                                .await;
                                        }
                                        Err(err) => {
                                            self.client
                                                .log_message(
                                                    MessageType::ERROR,
                                                    format!(
                                                        "Failed to parse locals query for {}: {}",
                                                        lang_name, err
                                                    ),
                                                )
                                                .await;
                                        }
                                    }
                                }
                                Err(err) => {
                                    self.client
                                        .log_message(
                                            MessageType::ERROR,
                                            format!(
                                                "Failed to load locals queries for {}: {}",
                                                lang_name, err
                                            ),
                                        )
                                        .await;
                                }
                            }
                        }
                        {
                            self.languages
                                .lock()
                                .unwrap()
                                .insert(lang_name.to_string(), language);
                        }
                        self.client
                            .log_message(
                                MessageType::INFO,
                                format!("Language {} loaded.", lang_name),
                            )
                            .await;
                    }
                    Err(err) => {
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to load language {}: {}", lang_name, err),
                            )
                            .await;
                    }
                },
                None => {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("No library path found for language {}: neither explicit library path nor valid runtimepath entry", lang_name),
                        )
                        .await;
                }
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for TreeSitterLs {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Debug: Log initialization
        self.client
            .log_message(MessageType::INFO, "Received initialization request")
            .await;

        // Parse configuration from initialization_options
        if let Some(options) = params.initialization_options {
            if let Ok(settings) = serde_json::from_value::<TreeSitterSettings>(options) {
                self.client
                    .log_message(MessageType::INFO, "Parsed as TreeSitterSettings")
                    .await;
                self.load_settings(settings).await;
            } else {
                self.client
                    .log_message(MessageType::ERROR, "Failed to parse initialization options")
                    .await;
            }
        }

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "treesitter-ls".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: LEGEND_TYPES.to_vec(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server is ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.parse_document(params.text_document.uri, params.text_document.text)
            .await;
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.parse_document(
            params.text_document.uri,
            params.content_changes.remove(0).text,
        )
        .await;
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        if let Ok(settings) = serde_json::from_value::<TreeSitterSettings>(params.settings) {
            self.load_settings(settings).await;
            self.client
                .log_message(MessageType::INFO, "Configuration updated!")
                .await;
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(None);
        };
        let queries = self.queries.lock().unwrap();
        let Some(query) = queries.get(&language_name) else {
            return Ok(None);
        };

        let Some(doc) = self.document_map.get(&uri) else {
            return Ok(None);
        };
        let (text, tree) = &*doc;
        let Some(tree) = tree.as_ref() else {
            return Ok(None);
        };

        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

        let mut tokens = vec![];
        while let Some(m) = matches.next() {
            for c in m.captures {
                let node = c.node;
                let start_pos = node.start_position();
                let end_pos = node.end_position();
                if start_pos.row == end_pos.row {
                    tokens.push((
                        start_pos.row,
                        start_pos.column,
                        end_pos.column - start_pos.column,
                        c.index,
                    ));
                }
            }
        }
        tokens.sort();

        let mut last_line = 0;
        let mut last_start = 0;
        let mut data = Vec::new();

        for (line, start, length, capture_index) in tokens {
            let delta_line = line - last_line;
            let delta_start = if delta_line == 0 {
                start - last_start
            } else {
                start
            };

            let token_type_name = &query.capture_names()[capture_index as usize];
            let token_type = LEGEND_TYPES
                .iter()
                .position(|t| t.as_str() == *token_type_name)
                .unwrap_or(0);

            data.push(SemanticToken {
                delta_line: delta_line as u32,
                delta_start: delta_start as u32,
                length: length as u32,
                token_type: token_type as u32,
                token_modifiers_bitset: 0,
            });

            last_line = line;
            last_start = start;
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get language for document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(None);
        };

        // Get locals query
        let locals_queries = self.locals_queries.lock().unwrap();
        let Some(locals_query) = locals_queries.get(&language_name) else {
            return Ok(None);
        };

        // Get document and tree
        let Some(doc) = self.document_map.get(&uri) else {
            return Ok(None);
        };
        let (text, tree) = &*doc;
        let Some(tree) = tree.as_ref() else {
            return Ok(None);
        };

        // Convert position to byte offset
        let byte_offset = self.position_to_byte_offset(text, position);

        // Use the definition resolver
        let result =
            self.definition_resolver
                .resolve_definition(text, tree, locals_query, byte_offset);

        // Convert result to LSP response
        if let Some(definition) = result {
            let start_point = definition.start_position;
            let end_point = definition.end_position;

            let location = Location {
                uri: uri.clone(),
                range: Range {
                    start: Position {
                        line: start_point.row as u32,
                        character: start_point.column as u32,
                    },
                    end: Position {
                        line: end_point.row as u32,
                        character: end_point.column as u32,
                    },
                },
            };

            Ok(Some(GotoDefinitionResponse::Scalar(location)))
        } else {
            Ok(None)
        }
    }
}

impl TreeSitterLs {
    fn position_to_byte_offset(&self, text: &str, position: Position) -> usize {
        let mut byte_offset = 0;
        let mut current_line = 0;
        let mut current_char = 0;

        for ch in text.chars() {
            if current_line == position.line as usize && current_char == position.character as usize
            {
                return byte_offset;
            }

            if ch == '\n' {
                current_line += 1;
                current_char = 0;
            } else {
                current_char += 1;
            }

            byte_offset += ch.len_utf8();
        }

        byte_offset
    }
}

#[cfg(test)]
mod simple_tests;
