use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::InputEdit;

use crate::analysis::selection::position_to_point;
use crate::analysis::{DefinitionResolver, LEGEND_MODIFIERS, LEGEND_TYPES};
use crate::analysis::{
    handle_code_actions, handle_goto_definition, handle_selection_range,
    handle_semantic_tokens_full_delta, handle_semantic_tokens_range,
};
use crate::domain::settings::WorkspaceSettings as DomainWorkspaceSettings;
use crate::domain::{
    SemanticTokens as DomainSemanticTokens,
    SemanticTokensFullDeltaResult as DomainSemanticTokensFullDeltaResult,
    SemanticTokensRangeResult as DomainSemanticTokensRangeResult,
    SemanticTokensResult as DomainSemanticTokensResult,
};
use crate::language::{LanguageEvent, LanguageLogLevel};
use crate::lsp::protocol;
use crate::text::{PositionMapper, SimplePositionMapper};
use crate::workspace::Workspace;
use crate::workspace::{SettingsEvent, SettingsEventKind, SettingsSource};

fn lsp_legend_types() -> Vec<SemanticTokenType> {
    LEGEND_TYPES
        .iter()
        .copied()
        .map(SemanticTokenType::new)
        .collect()
}

fn lsp_legend_modifiers() -> Vec<SemanticTokenModifier> {
    LEGEND_MODIFIERS
        .iter()
        .copied()
        .map(SemanticTokenModifier::new)
        .collect()
}

pub struct TreeSitterLs {
    client: Client,
    workspace: Workspace,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("workspace", &"Workspace")
            .finish_non_exhaustive()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            workspace: Workspace::new(),
        }
    }

    async fn parse_document(
        &self,
        uri: Url,
        text: String,
        language_id: Option<&str>,
        edits: Vec<InputEdit>,
    ) {
        let outcome = self.workspace.parse_document(uri, text, language_id, edits);
        self.handle_language_events(&outcome.events).await;
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        self.workspace.language_for_document(uri)
    }

    async fn apply_settings(&self, settings: DomainWorkspaceSettings) {
        let summary = self.workspace.load_settings(settings);
        self.handle_language_events(&summary.events).await;
    }

    async fn report_settings_events(&self, events: &[SettingsEvent]) {
        for event in events {
            let message_type = match event.kind {
                SettingsEventKind::Info => MessageType::INFO,
                SettingsEventKind::Warning => MessageType::WARNING,
            };
            self.client
                .log_message(message_type, event.message.clone())
                .await;
        }
    }

    async fn handle_language_events(&self, events: &[LanguageEvent]) {
        for event in events {
            match event {
                LanguageEvent::Log { level, message } => {
                    let message_type = match level {
                        LanguageLogLevel::Error => MessageType::ERROR,
                        LanguageLogLevel::Warning => MessageType::WARNING,
                        LanguageLogLevel::Info => MessageType::INFO,
                    };
                    self.client.log_message(message_type, message.clone()).await;
                }
                LanguageEvent::SemanticTokensRefresh { language_id } => {
                    if let Err(err) = self.client.semantic_tokens_refresh().await {
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                format!(
                                    "Failed to request semantic tokens refresh for {language_id}: {err}"
                                ),
                            )
                            .await;
                    }
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

        // Get root path from workspace folders, root_uri, or current directory
        let root_path = if let Some(folders) = &params.workspace_folders {
            folders
                .first()
                .and_then(|folder| folder.uri.to_file_path().ok())
        } else if let Some(root_uri) = &params.root_uri {
            root_uri.to_file_path().ok()
        } else {
            // Fallback to current working directory
            std::env::current_dir().ok()
        };

        // Store root path for later use and log the source
        if let Some(ref path) = root_path {
            let source = if params.workspace_folders.is_some() {
                "workspace folders"
            } else if params.root_uri.is_some() {
                "root_uri"
            } else {
                "current working directory (fallback)"
            };

            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Using workspace root from {}: {}", source, path.display()),
                )
                .await;
            self.workspace.set_root_path(Some(path.clone()));
        } else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    "Failed to determine workspace root - config file will not be loaded",
                )
                .await;
        }

        let settings_outcome = self.workspace.load_workspace_settings(
            params
                .initialization_options
                .map(|options| (SettingsSource::InitializationOptions, options)),
        );
        self.report_settings_events(&settings_outcome.events).await;

        if let Some(settings) = settings_outcome.settings {
            self.apply_settings(settings).await;
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
                                token_types: lsp_legend_types(),
                                token_modifiers: lsp_legend_modifiers(),
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
            let has_queries = self.workspace.has_queries(&language_name);

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
                let has_queries_after_delay = self.workspace.has_queries(&language_name);

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
        self.workspace.remove_document(&uri);

        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Retrieve the stored document info
        let (language_id, old_text) = {
            let doc = self.workspace.document(&uri);
            match doc {
                Some(d) => (
                    d.layers().get_language_id().map(|s| s.to_string()),
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
                let start_offset = mapper
                    .position_to_byte(protocol::to_domain_position(&range.start))
                    .unwrap_or(text.len());
                let end_offset = mapper
                    .position_to_byte(protocol::to_domain_position(&range.end))
                    .unwrap_or(text.len());
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
                    start_position: position_to_point(&protocol::to_domain_position(&range.start)),
                    old_end_position: position_to_point(&protocol::to_domain_position(&range.end)),
                    new_end_position: position_to_point(&protocol::to_domain_position(
                        &new_end_position,
                    )),
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
        let settings_outcome = self
            .workspace
            .load_workspace_settings(Some((SettingsSource::ClientConfiguration, params.settings)));
        self.report_settings_events(&settings_outcome.events).await;

        if let Some(settings) = settings_outcome.settings {
            self.apply_settings(settings).await;
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
        let Some(query) = self.workspace.highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get document data and compute tokens, then drop the reference
        let result = {
            let Some(doc) = self.workspace.document(&uri) else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };
            let text = doc.text();
            let Some(root_layer) = doc.layers().root_layer() else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };

            // Get capture mappings
            let capture_mappings = self.workspace.capture_mappings();

            // Use the stable semantic tokens handler for the root layer
            crate::analysis::handle_semantic_tokens_full(
                text,
                &root_layer.tree,
                &query,
                Some(&language_name),
                Some(&capture_mappings),
            )
        }; // doc reference is dropped here

        let mut tokens_with_id = match result.unwrap_or_else(|| {
            DomainSemanticTokensResult::Tokens(DomainSemanticTokens {
                result_id: None,
                data: Vec::new(),
            })
        }) {
            DomainSemanticTokensResult::Tokens(tokens) => tokens,
            DomainSemanticTokensResult::Partial(_) => DomainSemanticTokens {
                result_id: None,
                data: Vec::new(),
            },
        };
        // Simple ID based on token count and first/last token info
        let id = if tokens_with_id.data.is_empty() {
            "empty".to_string()
        } else {
            format!(
                "v{}_{}",
                tokens_with_id.data.len(),
                tokens_with_id
                    .data
                    .first()
                    .map(|t| t.delta_line)
                    .unwrap_or(0)
            )
        };
        tokens_with_id.result_id = Some(id);
        let stored_tokens = tokens_with_id.clone();
        let lsp_tokens = protocol::to_lsp_semantic_tokens(tokens_with_id);
        self.workspace.update_semantic_tokens(&uri, stored_tokens);
        Ok(Some(SemanticTokensResult::Tokens(lsp_tokens)))
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

        let Some(query) = self.workspace.highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Get document data and compute delta, then drop the reference
        let result = {
            let Some(doc) = self.workspace.document(&uri) else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            let text = doc.text();
            let Some(root_layer) = doc.layers().root_layer() else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            // Get previous tokens from document
            let previous_tokens = doc
                .last_semantic_tokens()
                .map(|snapshot| snapshot.tokens().clone());

            // Get capture mappings
            let capture_mappings = self.workspace.capture_mappings();

            // Delegate to handler
            handle_semantic_tokens_full_delta(
                text,
                &root_layer.tree,
                &query,
                &previous_result_id,
                previous_tokens.as_ref(),
                Some(&language_name),
                Some(&capture_mappings),
            )
        }; // doc reference is dropped here

        let domain_result = result.unwrap_or_else(|| {
            DomainSemanticTokensFullDeltaResult::Tokens(DomainSemanticTokens {
                result_id: None,
                data: Vec::new(),
            })
        });

        match domain_result {
            DomainSemanticTokensFullDeltaResult::Tokens(tokens) => {
                let mut tokens_with_id = tokens;
                let id = if tokens_with_id.data.is_empty() {
                    "empty".to_string()
                } else {
                    format!(
                        "v{}_{}",
                        tokens_with_id.data.len(),
                        tokens_with_id
                            .data
                            .first()
                            .map(|t| t.delta_line)
                            .unwrap_or(0)
                    )
                };
                tokens_with_id.result_id = Some(id);
                let stored_tokens = tokens_with_id.clone();
                let lsp_tokens = protocol::to_lsp_semantic_tokens(tokens_with_id);
                self.workspace.update_semantic_tokens(&uri, stored_tokens);
                Ok(Some(SemanticTokensFullDeltaResult::Tokens(lsp_tokens)))
            }
            other => Ok(Some(protocol::to_lsp_semantic_tokens_full_delta(other))),
        }
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let domain_range = protocol::to_domain_range(&range);

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(query) = self.workspace.highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(doc) = self.workspace.document(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let text = doc.text();
        let Some(root_layer) = doc.layers().root_layer() else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Delegate to handler
        // Get capture mappings
        let capture_mappings = self.workspace.capture_mappings();

        let result = handle_semantic_tokens_range(
            text,
            &root_layer.tree,
            &query,
            &domain_range,
            Some(&language_name),
            Some(&capture_mappings),
        );

        // Convert to RangeResult, treating partial responses as empty for now
        let domain_range_result = match result.unwrap_or_else(|| {
            DomainSemanticTokensResult::Tokens(DomainSemanticTokens {
                result_id: None,
                data: Vec::new(),
            })
        }) {
            DomainSemanticTokensResult::Tokens(tokens) => {
                DomainSemanticTokensRangeResult::from(tokens)
            }
            DomainSemanticTokensResult::Partial(partial) => {
                DomainSemanticTokensRangeResult::from(partial)
            }
        };

        Ok(Some(protocol::to_lsp_semantic_tokens_range_result(
            domain_range_result,
        )))
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
        let Some(locals_query) = self.workspace.locals_query(&language_name) else {
            return Ok(None);
        };

        // Get document
        let Some(doc) = self.workspace.document(&uri) else {
            return Ok(None);
        };

        // Use layer-aware handler
        let resolver = DefinitionResolver::new();
        let response = handle_goto_definition(
            &resolver,
            &doc,
            protocol::to_domain_position(&position),
            &locals_query,
            &uri,
        );

        Ok(response.and_then(protocol::to_lsp_definition_response))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions: Vec<_> = params
            .positions
            .iter()
            .map(protocol::to_domain_position)
            .collect();

        // Get document
        let Some(doc) = self.workspace.document(&uri) else {
            return Ok(None);
        };

        // Use layer-aware handler
        let result = handle_selection_range(&doc, &positions).map(|ranges| {
            ranges
                .into_iter()
                .map(|r| protocol::to_lsp_selection_range(&r))
                .collect()
        });

        Ok(result)
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document and tree
        let Some(doc) = self.workspace.document(&uri) else {
            return Ok(None);
        };
        let text = doc.text();
        let Some(root_layer) = doc.layers().root_layer() else {
            return Ok(None);
        };

        let domain_range = protocol::to_domain_range(&range);

        // Get language for the document
        let language_name = self.get_language_for_document(&uri);

        // Get capture mappings
        let capture_mappings = self.workspace.capture_mappings();

        // Get queries and delegate to handler
        let lsp_response = if let Some(lang) = language_name.clone() {
            let highlight_query = self.workspace.highlight_query(&lang);
            let locals_query = self.workspace.locals_query(&lang);

            let queries = highlight_query
                .as_ref()
                .map(|hq| (hq.as_ref(), locals_query.as_ref().map(|lq| lq.as_ref())));

            handle_code_actions(
                &uri,
                text,
                &root_layer.tree,
                domain_range,
                queries,
                language_name.as_deref(),
                Some(&capture_mappings),
            )
            .map(protocol::to_lsp_code_action_response)
        } else {
            handle_code_actions(&uri, text, &root_layer.tree, domain_range, None, None, None)
                .map(protocol::to_lsp_code_action_response)
        };

        Ok(lsp_response)
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
