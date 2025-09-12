use std::path::PathBuf;
use std::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{InputEdit, Parser, Point, Query, Tree};

use crate::config::{TreeSitterSettings, merge_settings};
use crate::handlers::{DefinitionResolver, LEGEND_MODIFIERS, LEGEND_TYPES};
use crate::handlers::{
    handle_code_actions, handle_goto_definition, handle_selection_range,
    handle_semantic_tokens_full, handle_semantic_tokens_full_delta, handle_semantic_tokens_range,
};
use crate::state::{DocumentStore, LanguageService};
use crate::treesitter::position_to_byte_offset;

pub struct TreeSitterLs {
    client: Client,
    language_service: LanguageService,
    document_store: DocumentStore,
    root_path: Mutex<Option<PathBuf>>,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("document_store", &"DocumentStore")
            .finish_non_exhaustive()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            language_service: LanguageService::new(),
            document_store: DocumentStore::new(),
            root_path: Mutex::new(None),
        }
    }

    async fn parse_document(&self, uri: Url, text: String, language_id: Option<&str>) {
        // Get the current tree for incremental parsing
        let old_tree = self
            .document_store
            .get(&uri)
            .and_then(|doc| doc.tree.clone());

        self.parse_document_with_tree(uri, text, old_tree, language_id)
            .await;
    }

    async fn parse_document_with_tree(
        &self,
        uri: Url,
        text: String,
        old_tree: Option<Tree>,
        language_id: Option<&str>,
    ) {
        // Detect file extension
        let extension = uri.path().split('.').next_back().unwrap_or("");

        // Find the language for this file extension
        let language_name = {
            let filetype_map = self.language_service.filetype_map.lock().unwrap();
            filetype_map.get(extension).cloned()
        };

        // Use the configured language or fall back to language_id
        let language_name = language_name.or_else(|| language_id.map(|s| s.to_string()));

        if let Some(language_name) = language_name {
            // Try to dynamically load the language if not already loaded
            let language_loaded = {
                let languages = self.language_service.languages.lock().unwrap();
                languages.contains_key(&language_name)
            };
            
            if !language_loaded {
                let loaded = self.language_service
                    .try_load_language_by_id(&language_name, &self.client)
                    .await;
                    
                // If language was dynamically loaded, check if queries are also loaded
                if loaded {
                    let has_queries = {
                        let queries = self.language_service.queries.lock().unwrap();
                        queries.contains_key(&language_name)
                    };
                    
                    // If queries are loaded, request semantic tokens refresh
                    if has_queries {
                        if self.client.semantic_tokens_refresh().await.is_ok() {
                            self.client
                                .log_message(MessageType::INFO, "Requested semantic tokens refresh after dynamic loading")
                                .await;
                        }
                    }
                }
            }

            let languages = self.language_service.languages.lock().unwrap();
            if let Some(language) = languages.get(&language_name) {
                // Get or create a parser for this language
                let mut parsers = self.language_service.parsers.lock().unwrap();
                let parser = parsers.entry(language_name.clone()).or_insert_with(|| {
                    let mut p = Parser::new();
                    p.set_language(language).unwrap();
                    p
                });

                // Parse the document incrementally if old_tree exists
                if let Some(tree) = parser.parse(&text, old_tree.as_ref()) {
                    self.document_store.insert(
                        uri,
                        text,
                        Some(tree),
                        language_id.map(|s| s.to_string()),
                    );
                    return;
                }
            }
        }

        self.document_store
            .insert(uri, text, None, language_id.map(|s| s.to_string()));
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        // First try to get language from the service (based on file extension)
        if let Some(lang) = self.language_service.get_language_for_document(uri) {
            return Some(lang);
        }

        // Fall back to the language_id stored in the document
        self.document_store
            .get(uri)
            .and_then(|doc| doc.language_id.clone())
    }

    async fn load_settings(&self, settings: TreeSitterSettings) {
        self.language_service
            .load_settings(settings, &self.client)
            .await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for TreeSitterLs {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Debug: Log initialization
        self.client
            .log_message(MessageType::INFO, "Received initialization request")
            .await;

        // Get root path from workspace folders, root_uri, deprecated root_path, or current directory
        let root_path = if let Some(folders) = &params.workspace_folders {
            folders
                .first()
                .and_then(|folder| folder.uri.to_file_path().ok())
        } else if let Some(root_uri) = &params.root_uri {
            root_uri.to_file_path().ok()
        } else {
            #[allow(deprecated)] // Support for older LSP clients
            if let Some(root_path) = &params.root_path {
                // Support deprecated root_path field for compatibility
                Some(PathBuf::from(root_path))
            } else {
                // Fallback to current working directory
                std::env::current_dir().ok()
            }
        };

        // Store root path for later use and log the source
        if let Some(ref path) = root_path {
            let source = if params.workspace_folders.is_some() {
                "workspace folders"
            } else if params.root_uri.is_some() {
                "root_uri"
            } else {
                #[allow(deprecated)]
                if params.root_path.is_some() {
                    "root_path (deprecated)"
                } else {
                    "current working directory (fallback)"
                }
            };

            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Using workspace root from {}: {}", source, path.display()),
                )
                .await;
            *self.root_path.lock().unwrap() = Some(path.clone());
        } else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    "Failed to determine workspace root - config file will not be loaded",
                )
                .await;
        }

        // Try to load configuration from treesitter-ls.toml file
        let mut toml_settings = None;
        if let Some(root) = &root_path {
            let config_path = root.join("treesitter-ls.toml");
            if config_path.exists() {
                self.client
                    .log_message(
                        MessageType::INFO,
                        format!("Found config file: {}", config_path.display()),
                    )
                    .await;

                if let Ok(toml_contents) = std::fs::read_to_string(&config_path) {
                    match toml::from_str::<TreeSitterSettings>(&toml_contents) {
                        Ok(settings) => {
                            self.client
                                .log_message(
                                    MessageType::INFO,
                                    "Successfully loaded treesitter-ls.toml",
                                )
                                .await;
                            toml_settings = Some(settings);
                        }
                        Err(e) => {
                            self.client
                                .log_message(
                                    MessageType::WARNING,
                                    format!("Failed to parse treesitter-ls.toml: {}", e),
                                )
                                .await;
                        }
                    }
                }
            }
        }

        // Parse configuration from initialization_options
        let init_settings = if let Some(options) = params.initialization_options {
            match serde_json::from_value::<TreeSitterSettings>(options) {
                Ok(settings) => {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            "Parsed initialization options as TreeSitterSettings",
                        )
                        .await;
                    Some(settings)
                }
                Err(_) => {
                    self.client
                        .log_message(
                            MessageType::WARNING,
                            "Failed to parse initialization options",
                        )
                        .await;
                    None
                }
            }
        } else {
            None
        };

        // Merge settings (prefer init_settings over toml_settings)
        let final_settings = merge_settings(toml_settings, init_settings);

        if let Some(settings) = final_settings {
            self.load_settings(settings).await;
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
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::INCREMENTAL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(false),
                        })),
                    },
                )),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: LEGEND_TYPES.to_vec(),
                                token_modifiers: LEGEND_MODIFIERS.to_vec(),
                            },
                            full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
                            range: Some(true),
                            ..Default::default()
                        },
                    ),
                ),
                definition_provider: Some(OneOf::Left(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
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
        let language_id = params.text_document.language_id;
        let uri = params.text_document.uri.clone();
        
        self.parse_document(
            params.text_document.uri,
            params.text_document.text,
            Some(&language_id),
        )
        .await;
        
        // Check if queries are ready for the document
        if let Some(language_name) = self.get_language_for_document(&uri) {
            let has_queries = {
                let queries = self.language_service.queries.lock().unwrap();
                queries.contains_key(&language_name)
            };
            
            // If document is parsed but queries aren't ready, request refresh
            if !has_queries {
                // Give a small delay for queries to load
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                
                // Check again after delay
                let has_queries_after_delay = {
                    let queries = self.language_service.queries.lock().unwrap();
                    queries.contains_key(&language_name)
                };
                
                if has_queries_after_delay {
                    if self.client.semantic_tokens_refresh().await.is_ok() {
                        self.client
                            .log_message(MessageType::INFO, "Requested semantic tokens refresh after queries loaded")
                            .await;
                    }
                }
            }
        }
        
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Retrieve the stored document info
        let (language_id, old_text, mut old_tree) = {
            let doc = self.document_store.get(&uri);
            match doc {
                Some(d) => (d.language_id.clone(), d.text.clone(), d.tree.clone()),
                None => {
                    self.client
                        .log_message(MessageType::WARNING, "Document not found for change event")
                        .await;
                    return;
                }
            }
        };

        let mut text = old_text.clone();

        // Apply incremental changes to the text and tree
        for change in params.content_changes {
            if let Some(range) = change.range {
                // Incremental change - update tree with edit information
                let start_offset = position_to_byte_offset(&text, range.start);
                let end_offset = position_to_byte_offset(&text, range.end);
                let new_end_offset = start_offset + change.text.len();

                // Calculate the new end position correctly using UTF-16 code units
                let lines_in_change = change.text.matches('\n').count();
                let new_end_line = range.start.line + lines_in_change as u32;
                let _new_end_character = if lines_in_change > 0 {
                    // For multi-line changes, count UTF-16 code units in the last line
                    change
                        .text
                        .split('\n')
                        .next_back()
                        .unwrap_or("")
                        .chars()
                        .map(|c| c.len_utf16())
                        .sum::<usize>() as u32
                } else {
                    // For single-line changes, add UTF-16 length to the start character
                    range.start.character
                        + change.text.chars().map(|c| c.len_utf16()).sum::<usize>() as u32
                };

                // Apply edit to the tree if it exists
                if let Some(ref mut tree) = old_tree {
                    // Tree-sitter Point uses byte offsets for columns, not UTF-16 code units
                    // We need to calculate the byte column for each position
                    let start_byte_column = text
                        .lines()
                        .nth(range.start.line as usize)
                        .map(|line| {
                            let mut byte_col = 0;
                            let mut utf16_col = 0;
                            for ch in line.chars() {
                                if utf16_col >= range.start.character as usize {
                                    break;
                                }
                                byte_col += ch.len_utf8();
                                utf16_col += ch.len_utf16();
                            }
                            byte_col
                        })
                        .unwrap_or(0);

                    let old_end_byte_column = text
                        .lines()
                        .nth(range.end.line as usize)
                        .map(|line| {
                            let mut byte_col = 0;
                            let mut utf16_col = 0;
                            for ch in line.chars() {
                                if utf16_col >= range.end.character as usize {
                                    break;
                                }
                                byte_col += ch.len_utf8();
                                utf16_col += ch.len_utf16();
                            }
                            byte_col
                        })
                        .unwrap_or(0);

                    // Calculate new end byte column
                    let new_end_byte_column = if lines_in_change > 0 {
                        // For multi-line changes, calculate byte length of last line
                        change.text.split('\n').next_back().unwrap_or("").len()
                    } else {
                        // For single-line changes, add byte length to start column
                        start_byte_column + change.text.len()
                    };

                    let edit = InputEdit {
                        start_byte: start_offset,
                        old_end_byte: end_offset,
                        new_end_byte: new_end_offset,
                        start_position: Point::new(range.start.line as usize, start_byte_column),
                        old_end_position: Point::new(range.end.line as usize, old_end_byte_column),
                        new_end_position: Point::new(new_end_line as usize, new_end_byte_column),
                    };
                    tree.edit(&edit);
                }

                // Replace the range with new text
                text.replace_range(start_offset..end_offset, &change.text);
            } else {
                // Full document change - clear old tree
                text = change.text;
                old_tree = None;
            }
        }

        // Parse with the edited tree for true incremental parsing
        self.parse_document_with_tree(uri.clone(), text, old_tree, language_id.as_deref())
            .await;

        // Request the client to refresh semantic tokens
        // This will trigger the client to request new semantic tokens
        if self.client.semantic_tokens_refresh().await.is_ok() {
            self.client
                .log_message(MessageType::INFO, "Requested semantic tokens refresh")
                .await;
        }

        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        // Try to load configuration from treesitter-ls.toml file
        let mut toml_settings = None;
        let root_path = self.root_path.lock().unwrap().clone();
        if let Some(root) = root_path {
            let config_path = root.join("treesitter-ls.toml");
            if config_path.exists()
                && let Ok(toml_contents) = std::fs::read_to_string(&config_path)
            {
                match toml::from_str::<TreeSitterSettings>(&toml_contents) {
                    Ok(settings) => {
                        self.client
                            .log_message(MessageType::INFO, "Reloaded treesitter-ls.toml")
                            .await;
                        toml_settings = Some(settings);
                    }
                    Err(e) => {
                        self.client
                            .log_message(
                                MessageType::WARNING,
                                format!("Failed to parse treesitter-ls.toml: {}", e),
                            )
                            .await;
                    }
                }
            }
        }

        // Parse configuration from settings
        let config_settings = serde_json::from_value::<TreeSitterSettings>(params.settings).ok();

        // Merge settings (prefer config_settings over toml_settings)
        let final_settings = merge_settings(toml_settings, config_settings);

        if let Some(settings) = final_settings {
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
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };
        let queries = self.language_service.queries.lock().unwrap();
        let Some(query) = queries.get(&language_name) else {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get document data and compute tokens, then drop the reference
        let result = {
            let Some(doc) = self.document_store.get(&uri) else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };
            let text = &doc.text;
            let Some(tree) = doc.tree.as_ref() else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };

            // Get capture mappings
            let capture_mappings = self.language_service.capture_mappings.lock().unwrap();

            // Delegate to handler - this already returns Some(SemanticTokensResult) or None
            // We need to ensure it always returns Some
            handle_semantic_tokens_full(
                text,
                tree,
                query,
                Some(&language_name),
                Some(&*capture_mappings),
            )
            .or_else(|| {
                Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                }))
            })
        }; // doc reference is dropped here

        // Store the tokens for delta calculation
        if let Some(SemanticTokensResult::Tokens(ref tokens)) = result {
            let mut tokens_with_id = tokens.clone();
            // Simple ID based on token count and first/last token info
            let id = if tokens.data.is_empty() {
                "empty".to_string()
            } else {
                format!(
                    "v{}_{}",
                    tokens.data.len(),
                    tokens.data.first().map(|t| t.delta_line).unwrap_or(0)
                )
            };
            tokens_with_id.result_id = Some(id);
            self.document_store
                .update_semantic_tokens(&uri, tokens_with_id.clone());
            return Ok(Some(SemanticTokensResult::Tokens(tokens_with_id)));
        }

        Ok(result)
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = params.text_document.uri;
        let previous_result_id = params.previous_result_id;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        let queries = self.language_service.queries.lock().unwrap();
        let Some(query) = queries.get(&language_name) else {
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Get document data and compute delta, then drop the reference
        let result = {
            let Some(doc) = self.document_store.get(&uri) else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            let text = &doc.text;
            let Some(tree) = doc.tree.as_ref() else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            // Get previous tokens from document
            let previous_tokens = doc.last_semantic_tokens.as_ref();

            // Get capture mappings
            let capture_mappings = self.language_service.capture_mappings.lock().unwrap();

            // Delegate to handler
            handle_semantic_tokens_full_delta(
                text,
                tree,
                query,
                &previous_result_id,
                previous_tokens,
                Some(&language_name),
                Some(&*capture_mappings),
            )
        }; // doc reference is dropped here

        // Store updated tokens if we got full tokens back
        if let Some(SemanticTokensFullDeltaResult::Tokens(ref tokens)) = result {
            let mut tokens_with_id = tokens.clone();
            let id = if tokens.data.is_empty() {
                "empty".to_string()
            } else {
                format!(
                    "v{}_{}",
                    tokens.data.len(),
                    tokens.data.first().map(|t| t.delta_line).unwrap_or(0)
                )
            };
            tokens_with_id.result_id = Some(id);
            self.document_store
                .update_semantic_tokens(&uri, tokens_with_id.clone());
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(tokens_with_id)));
        }

        Ok(result)
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let range = params.range;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let queries = self.language_service.queries.lock().unwrap();
        let Some(query) = queries.get(&language_name) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let text = &doc.text;
        let Some(tree) = doc.tree.as_ref() else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Delegate to handler
        // Get capture mappings
        let capture_mappings = self.language_service.capture_mappings.lock().unwrap();

        let result = handle_semantic_tokens_range(
            text,
            tree,
            query,
            &range,
            Some(&language_name),
            Some(&*capture_mappings),
        );

        // Convert to RangeResult
        match result {
            Some(SemanticTokensResult::Tokens(tokens)) => {
                Ok(Some(SemanticTokensRangeResult::Tokens(tokens)))
            }
            _ => Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            }))),
        }
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
        let locals_queries = self.language_service.locals_queries.lock().unwrap();
        let Some(locals_query) = locals_queries.get(&language_name) else {
            return Ok(None);
        };

        // Get document and tree
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };
        let text = &doc.text;
        let Some(tree) = doc.tree.as_ref() else {
            return Ok(None);
        };

        // Convert position to byte offset
        let byte_offset = position_to_byte_offset(text, position);

        // Create resolver
        let resolver = DefinitionResolver::new();

        // Delegate to handler
        Ok(handle_goto_definition(
            &resolver,
            text,
            tree,
            locals_query,
            byte_offset,
            &uri,
        ))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions = params.positions;

        // Get document and tree
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };
        let Some(tree) = doc.tree.as_ref() else {
            return Ok(None);
        };

        // Delegate to handler
        Ok(handle_selection_range(tree, &positions))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document and tree
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };
        let text = &doc.text;
        let Some(tree) = doc.tree.as_ref() else {
            return Ok(None);
        };

        // Get language for the document
        let language_name = self.get_language_for_document(&uri);

        // Get capture mappings
        let capture_mappings = self.language_service.capture_mappings.lock().unwrap();

        // Get queries and delegate to handler
        if let Some(lang) = language_name.clone() {
            let queries_lock = self.language_service.queries.lock().unwrap();
            let locals_queries_lock = self.language_service.locals_queries.lock().unwrap();

            let queries = queries_lock.get(&lang).map(|hq| {
                (
                    hq as &Query,
                    locals_queries_lock.get(&lang).map(|lq| lq as &Query),
                )
            });

            let actions = handle_code_actions(
                &uri,
                text,
                tree,
                range,
                queries,
                language_name.as_deref(),
                Some(&*capture_mappings),
            );
            Ok(actions)
        } else {
            // No language, just basic inspect without queries
            let actions = handle_code_actions(&uri, text, tree, range, None, None, None);
            Ok(actions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_create_valid_url_from_file_path() {
        let path = "/tmp/test.rs";
        let url = Url::from_file_path(path).unwrap();
        assert!(url.as_str().contains("test.rs"));
        assert!(url.scheme() == "file");
    }

    #[test]
    fn should_handle_invalid_file_paths() {
        let invalid_path = "not/an/absolute/path";
        let result = Url::from_file_path(invalid_path);
        assert!(result.is_err());
    }

    #[test]
    fn should_create_position_with_valid_coordinates() {
        let pos = Position {
            line: 10,
            character: 5,
        };
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn should_create_valid_range() {
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 10,
            },
        };
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 1);

        // Validate range ordering
        assert!(range.start.line <= range.end.line);
    }

    #[test]
    fn should_validate_url_schemes() {
        let valid_urls = vec![
            "file:///absolute/path/to/file.rs",
            "file:///home/user/project/src/main.rs",
        ];

        for url_str in valid_urls {
            let url = Url::parse(url_str).unwrap();
            assert_eq!(url.scheme(), "file");
        }
    }
}
