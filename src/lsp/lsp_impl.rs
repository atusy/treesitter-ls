use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{InputEdit, Query};

use crate::config::{TreeSitterSettings, merge_settings};
use crate::document::coordinates::{PositionMapper, SimplePositionMapper};
use crate::features::{DefinitionResolver, LEGEND_MODIFIERS, LEGEND_TYPES};
use crate::features::{
    handle_code_actions, handle_goto_definition, handle_selection_range,
    handle_semantic_tokens_full_delta, handle_semantic_tokens_range,
};
use crate::injection::LanguageLayer;
use crate::language::{DocumentParserPool, ParserFactory};
use crate::syntax::tree::position_to_point;
use crate::workspace::{documents::DocumentStore, languages::LanguageService};

pub struct TreeSitterLs {
    client: Client,
    language_service: Arc<LanguageService>,
    document_store: DocumentStore,
    root_path: Mutex<Option<PathBuf>>,
    parser_factory: Arc<ParserFactory>,
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
        let language_service = Arc::new(LanguageService::new());
        let parser_factory = language_service.create_parser_factory();
        Self {
            client,
            language_service,
            document_store: DocumentStore::new(),
            root_path: Mutex::new(None),
            parser_factory,
        }
    }

    async fn parse_document(
        &self,
        uri: Url,
        text: String,
        language_id: Option<&str>,
        edits: Vec<InputEdit>,
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
            let language_loaded = self
                .language_service
                .language_registry
                .contains(&language_name);

            if !language_loaded {
                let loaded = self
                    .language_service
                    .try_load_language_by_id(&language_name, &self.client)
                    .await;

                // If language was dynamically loaded, check if queries are also loaded
                if loaded {
                    let has_queries = {
                        let queries = self.language_service.queries.lock().unwrap();
                        queries.contains_key(&language_name)
                    };

                    // If queries are loaded, request semantic tokens refresh
                    if has_queries && self.client.semantic_tokens_refresh().await.is_ok() {
                        self.client
                            .log_message(
                                MessageType::INFO,
                                "Requested semantic tokens refresh after dynamic loading",
                            )
                            .await;
                    }
                }
            }

            // Initialize parser pool if needed
            let needs_pool_init = self
                .document_store
                .get(&uri)
                .map(|doc| doc.layers().parser_pool().is_none())
                .unwrap_or(true);

            if needs_pool_init {
                // Initialize parser pool for new documents or documents without a pool
                let parser_pool = DocumentParserPool::new(self.parser_factory.clone());
                self.document_store.init_parser_pool(&uri, parser_pool);
            }

            // Create a parser for this parse operation
            let parser = self.parser_factory.create_parser(&language_name);
            if let Some(mut parser) = parser {
                // Get old tree for incremental parsing if document exists
                let old_tree = if !edits.is_empty() {
                    // Get the tree with edits applied for incremental parsing
                    self.document_store.get_edited_tree(&uri, &edits)
                } else {
                    // For non-incremental updates, check if document exists
                    self.document_store
                        .get(&uri)
                        .and_then(|doc| doc.root_layer().map(|layer| layer.tree.clone()))
                };

                // Parse the document with incremental parsing if old tree exists
                if let Some(tree) = parser.parse(&text, old_tree.as_ref()) {
                    // Update document with the new tree (handles incremental updates properly)
                    if !edits.is_empty() {
                        self.document_store
                            .update_document_with_tree(uri.clone(), text, tree);
                    } else {
                        // For initial parsing or full updates, use insert with root layer
                        let root_layer = Some(LanguageLayer::root(language_name.clone(), tree));
                        self.document_store.insert(uri.clone(), text, root_layer);
                    }

                    return;
                }
            }
        }

        self.document_store.insert(uri, text, None);
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        // First try to get language from the service (based on file extension)
        if let Some(lang) = self.language_service.get_language_for_document(uri) {
            return Some(lang);
        }

        // Fall back to the language_id stored in the document
        self.document_store
            .get(uri)
            .and_then(|doc| doc.get_language_id().map(|s| s.to_string()))
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
            vec![], // No edits for initial document open
        )
        .await;

        // Check if queries are ready for the document
        if let Some(language_name) = self.get_language_for_document(&uri) {
            let has_queries = {
                let queries = self.language_service.queries.lock().unwrap();
                queries.contains_key(&language_name)
            };

            if has_queries {
                // Always request semantic tokens refresh on file open
                // This ensures the client always has fresh tokens, especially important
                // when reopening files after they were closed
                if self.client.semantic_tokens_refresh().await.is_ok() {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            "Requested semantic tokens refresh on file open",
                        )
                        .await;
                }
            } else {
                // If document is parsed but queries aren't ready, wait and retry
                // Give a small delay for queries to load
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                // Check again after delay
                let has_queries_after_delay = {
                    let queries = self.language_service.queries.lock().unwrap();
                    queries.contains_key(&language_name)
                };

                if has_queries_after_delay && self.client.semantic_tokens_refresh().await.is_ok() {
                    self.client
                        .log_message(
                            MessageType::INFO,
                            "Requested semantic tokens refresh after queries loaded",
                        )
                        .await;
                }
            }
        }

        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        // Remove the document from the store when it's closed
        // This ensures that reopening the file will properly reinitialize everything
        self.document_store.remove(&uri);

        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Retrieve the stored document info
        let (language_id, old_text) = {
            let doc = self.document_store.get(&uri);
            match doc {
                Some(d) => (
                    d.get_language_id().map(|s| s.to_string()),
                    d.text().to_string(),
                ),
                None => {
                    self.client
                        .log_message(MessageType::WARNING, "Document not found for change event")
                        .await;
                    return;
                }
            }
        };

        let mut text = old_text.clone();
        let mut edits = Vec::new();

        // Apply incremental changes to the text and collect edit information
        for change in params.content_changes {
            if let Some(range) = change.range {
                // Incremental change - create InputEdit for tree editing
                let mapper = SimplePositionMapper::new(&text);
                let start_offset = mapper.position_to_byte(range.start).unwrap_or(text.len());
                let end_offset = mapper.position_to_byte(range.end).unwrap_or(text.len());
                let new_end_offset = start_offset + change.text.len();

                // Calculate the new end position
                let mut lines = change.text.split('\n');
                let line_count = lines.clone().count();
                let last_line_len = lines.next_back().map(|l| l.len()).unwrap_or(0);

                let new_end_position = if line_count > 1 {
                    Position::new(
                        range.start.line + (line_count - 1) as u32,
                        last_line_len as u32,
                    )
                } else {
                    Position::new(
                        range.start.line,
                        range.start.character + last_line_len as u32,
                    )
                };

                // Create InputEdit for incremental parsing
                let edit = InputEdit {
                    start_byte: start_offset,
                    old_end_byte: end_offset,
                    new_end_byte: new_end_offset,
                    start_position: position_to_point(&range.start),
                    old_end_position: position_to_point(&range.end),
                    new_end_position: position_to_point(&new_end_position),
                };
                edits.push(edit);

                // Replace the range with new text
                text.replace_range(start_offset..end_offset, &change.text);
            } else {
                // Full document change - no incremental parsing
                text = change.text;
                edits.clear(); // Clear any previous edits since it's a full replacement
            }
        }

        // Parse the updated document with edit information
        self.parse_document(uri.clone(), text, language_id.as_deref(), edits)
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
            let text = doc.text();
            let Some(root_layer) = doc.root_layer() else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };

            // Get capture mappings
            let capture_mappings = self.language_service.capture_mappings.lock().unwrap();

            // NOTE: We use the original handler instead of the layered version because:
            // 1. The layered handler has incomplete injection support (positions not mapped)
            // 2. The layered handler may fail to process tokens when queries are not found
            // 3. The original handler is stable and well-tested
            // TODO: Fix semantic_tokens_layered handler when implementing proper injection support
            crate::features::handle_semantic_tokens_full(
                text,
                &root_layer.tree,
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

            let text = doc.text();
            let Some(root_layer) = doc.root_layer() else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            // Get previous tokens from document
            let previous_tokens = doc.last_semantic_tokens();

            // Get capture mappings
            let capture_mappings = self.language_service.capture_mappings.lock().unwrap();

            // Delegate to handler
            handle_semantic_tokens_full_delta(
                text,
                &root_layer.tree,
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

        let text = doc.text();
        let Some(root_layer) = doc.root_layer() else {
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
            &root_layer.tree,
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

        // Get document
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };

        // Use layer-aware handler
        let resolver = DefinitionResolver::new();
        Ok(handle_goto_definition(
            &resolver,
            &doc,
            position,
            locals_query,
            &uri,
        ))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions = params.positions;

        // Get document
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };

        // Use layer-aware handler
        Ok(handle_selection_range(&doc, &positions))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document and tree
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text();
        let Some(root_layer) = doc.root_layer() else {
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
                &root_layer.tree,
                range,
                queries,
                language_name.as_deref(),
                Some(&*capture_mappings),
            );
            Ok(actions)
        } else {
            // No language, just basic inspect without queries
            let actions =
                handle_code_actions(&uri, text, &root_layer.tree, range, None, None, None);
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
