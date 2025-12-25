use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::InputEdit;

// Note: position_to_point from selection.rs is deprecated - use PositionMapper.position_to_point() instead
use crate::analysis::{DefinitionResolver, LEGEND_MODIFIERS, LEGEND_TYPES, SemanticTokenCache};
use crate::analysis::{
    handle_code_actions, handle_goto_definition, handle_selection_range,
    handle_semantic_tokens_full_delta, next_result_id,
};
use crate::config::{TreeSitterSettings, WorkspaceSettings};
use crate::document::DocumentStore;
use crate::error::LockResultExt;
use crate::language::{DocumentParserPool, FailedParserRegistry, LanguageCoordinator};
use crate::language::{LanguageEvent, LanguageLogLevel};
use crate::lsp::{SettingsEvent, SettingsEventKind, SettingsSource, load_settings};
use crate::text::PositionMapper;
use arc_swap::ArcSwap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::auto_install::{InstallingLanguages, get_injected_languages};
use super::progress::{create_progress_begin, create_progress_end};

fn lsp_legend_types() -> Vec<SemanticTokenType> {
    LEGEND_TYPES
        .iter()
        .map(|t| SemanticTokenType::new(t.as_str()))
        .collect()
}

fn lsp_legend_modifiers() -> Vec<SemanticTokenModifier> {
    LEGEND_MODIFIERS
        .iter()
        .map(|m| SemanticTokenModifier::new(m.as_str()))
        .collect()
}

pub struct TreeSitterLs {
    client: Client,
    language: LanguageCoordinator,
    parser_pool: Mutex<DocumentParserPool>,
    documents: DocumentStore,
    /// Dedicated cache for semantic tokens with result_id validation
    semantic_cache: SemanticTokenCache,
    root_path: ArcSwap<Option<PathBuf>>,
    /// Settings including auto_install flag
    settings: ArcSwap<WorkspaceSettings>,
    /// Tracks languages currently being installed
    installing_languages: InstallingLanguages,
    /// Tracks parsers that have crashed
    failed_parsers: FailedParserRegistry,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("language", &"LanguageCoordinator")
            .field("parser_pool", &"Mutex<DocumentParserPool>")
            .field("documents", &"DocumentStore")
            .field("semantic_cache", &"SemanticTokenCache")
            .field("root_path", &"ArcSwap<Option<PathBuf>>")
            .field("settings", &"ArcSwap<WorkspaceSettings>")
            .field("installing_languages", &"InstallingLanguages")
            .field("failed_parsers", &"FailedParserRegistry")
            .finish_non_exhaustive()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        let language = LanguageCoordinator::new();
        let parser_pool = language.create_document_parser_pool();

        // Initialize failed parser registry with crash detection
        let failed_parsers = Self::init_failed_parser_registry();

        Self {
            client,
            language,
            parser_pool: Mutex::new(parser_pool),
            documents: DocumentStore::new(),
            semantic_cache: SemanticTokenCache::new(),
            root_path: ArcSwap::new(Arc::new(None)),
            settings: ArcSwap::new(Arc::new(WorkspaceSettings::default())),
            installing_languages: InstallingLanguages::new(),
            failed_parsers,
        }
    }

    /// Initialize the failed parser registry with crash detection.
    ///
    /// Uses the default data directory for state storage.
    /// If initialization fails, returns an empty registry.
    fn init_failed_parser_registry() -> FailedParserRegistry {
        let state_dir = crate::install::default_data_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp/treesitter-ls"));

        let registry = FailedParserRegistry::new(&state_dir);

        // Initialize and detect any previous crashes
        if let Err(e) = registry.init() {
            log::warn!(
                target: "treesitter_ls::crash_recovery",
                "Failed to initialize crash recovery state: {}",
                e
            );
        }

        registry
    }

    /// Check if auto-install is enabled.
    fn is_auto_install_enabled(&self) -> bool {
        self.settings.load().auto_install
    }

    async fn parse_document(
        &self,
        uri: Url,
        text: String,
        language_id: Option<&str>,
        edits: Vec<InputEdit>,
    ) {
        let mut events = Vec::new();

        // ADR-0005: Detection fallback chain via LanguageCoordinator
        let language_name = self
            .language
            .detect_language(uri.path(), language_id, &text);

        if let Some(language_name) = language_name {
            // Check if this parser has previously crashed
            if self.failed_parsers.is_failed(&language_name) {
                log::warn!(
                    target: "treesitter_ls::crash_recovery",
                    "Skipping parsing for '{}' - parser previously crashed",
                    language_name
                );
                // Store document without parsing
                self.documents.insert(uri, text, Some(language_name), None);
                self.handle_language_events(&events).await;
                return;
            }

            // Ensure language is loaded
            let load_result = self.language.ensure_language_loaded(&language_name);
            events.extend(load_result.events.clone());

            // Parse the document with crash detection
            let parsed_tree = {
                let mut pool = self
                    .parser_pool
                    .lock()
                    .recover_poison("parse_document parser_pool")
                    .unwrap();
                if let Some(mut parser) = pool.acquire(&language_name) {
                    let old_tree = if !edits.is_empty() {
                        self.documents.get_edited_tree(&uri, &edits)
                    } else {
                        self.documents.get(&uri).and_then(|doc| doc.tree().cloned())
                    };

                    // Record that we're about to parse (for crash detection)
                    let _ = self.failed_parsers.begin_parsing(&language_name);

                    let result = parser.parse(&text, old_tree.as_ref());

                    // Parsing succeeded without crash - clear the state
                    let _ = self.failed_parsers.end_parsing();

                    pool.release(language_name.clone(), parser);
                    result
                } else {
                    None
                }
            };

            // Store the parsed document
            if let Some(tree) = parsed_tree {
                if !edits.is_empty() {
                    self.documents
                        .update_document(uri.clone(), text, Some(tree));
                } else {
                    self.documents.insert(
                        uri.clone(),
                        text,
                        Some(language_name.clone()),
                        Some(tree),
                    );
                }

                self.handle_language_events(&events).await;
                return;
            }
        }

        // Store unparsed document
        self.documents.insert(uri, text, None, None);
        self.handle_language_events(&events).await;
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        super::auto_install::get_language_for_document(uri, &self.language, &self.documents)
    }

    async fn apply_settings(&self, settings: WorkspaceSettings) {
        // Store settings for auto_install check
        self.settings.store(Arc::new(settings.clone()));
        let summary = self.language.load_settings(settings);
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

    /// Try to auto-install a language if not already being installed.
    async fn maybe_auto_install_language(&self, language: &str, uri: Url, text: String) {
        // Try to start installation (returns false if already installing)
        if !self.installing_languages.try_start_install(language) {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Language '{}' is already being installed", language),
                )
                .await;
            return;
        }

        // Send progress Begin notification
        self.client
            .send_notification::<Progress>(create_progress_begin(language))
            .await;

        // Get data directory
        let data_dir = match crate::install::default_data_dir() {
            Some(dir) => dir,
            None => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        "Could not determine data directory for auto-install",
                    )
                    .await;
                // Send progress End notification (failure)
                self.client
                    .send_notification::<Progress>(create_progress_end(language, false))
                    .await;
                self.installing_languages.finish_install(language);
                return;
            }
        };

        // Check if parser already exists - skip installation and just reload
        if crate::install::parser_file_exists(language, &data_dir).is_some() {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "Parser for '{}' already exists. Loading without reinstall...",
                        language
                    ),
                )
                .await;

            // Send progress End notification (success - already installed)
            self.client
                .send_notification::<Progress>(create_progress_end(language, true))
                .await;
            self.installing_languages.finish_install(language);
            self.reload_language_after_install(language, &data_dir, uri, text)
                .await;
            return;
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!("Auto-installing language '{}' in background...", language),
            )
            .await;

        let lang = language.to_string();
        let result =
            crate::install::install_language_async(lang.clone(), data_dir.clone(), false).await;

        // Mark installation as complete
        self.installing_languages.finish_install(&lang);

        // Check if parser file exists after install attempt (even if queries failed)
        let parser_exists = crate::install::parser_file_exists(&lang, &data_dir).is_some();

        if result.is_success() {
            // Send progress End notification (success)
            self.client
                .send_notification::<Progress>(create_progress_end(&lang, true))
                .await;
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Successfully installed language '{}'. Reloading...", lang),
                )
                .await;

            // Add the installed paths to search paths and reload
            self.reload_language_after_install(&lang, &data_dir, uri, text)
                .await;
        } else if parser_exists {
            // Parser compiled successfully but queries failed (e.g., already exist)
            // Still try to reload since the parser is available
            self.client
                .send_notification::<Progress>(create_progress_end(&lang, true))
                .await;

            let mut warnings = Vec::new();
            if let Some(e) = &result.queries_error {
                warnings.push(format!("queries: {}", e));
            }
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!(
                        "Language '{}' parser installed but with warnings: {}. Reloading...",
                        lang,
                        warnings.join("; ")
                    ),
                )
                .await;

            // Still reload - parser is usable even without fresh queries
            self.reload_language_after_install(&lang, &data_dir, uri, text)
                .await;
        } else {
            // Send progress End notification (failure)
            self.client
                .send_notification::<Progress>(create_progress_end(&lang, false))
                .await;
            let mut errors = Vec::new();
            if let Some(e) = result.parser_error {
                errors.push(format!("parser: {}", e));
            }
            if let Some(e) = result.queries_error {
                errors.push(format!("queries: {}", e));
            }
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!(
                        "Failed to install language '{}': {}",
                        lang,
                        errors.join("; ")
                    ),
                )
                .await;
        }
    }

    /// Reload a language after installation and re-parse affected documents.
    async fn reload_language_after_install(
        &self,
        language: &str,
        data_dir: &std::path::Path,
        uri: Url,
        text: String,
    ) {
        // The installed files are at:
        // - Parser: {data_dir}/parsers/{language}/libtree-sitter-{language}.so
        // - Queries: {data_dir}/queries/{language}/

        // Update settings to include the new paths
        let current_settings = self.settings.load();
        let mut new_search_paths = current_settings.search_paths.clone();

        // Add parser directory to search paths
        let parser_dir = data_dir.join("parser");
        let parser_dir_str = parser_dir.to_string_lossy().to_string();
        if !new_search_paths.contains(&parser_dir_str) {
            new_search_paths.push(parser_dir_str);
        }

        // Add queries directory to search paths
        let queries_dir = data_dir.join("queries");
        let queries_dir_str = queries_dir.to_string_lossy().to_string();
        if !new_search_paths.contains(&queries_dir_str) {
            new_search_paths.push(queries_dir_str);
        }

        // Create updated settings
        let updated_settings = WorkspaceSettings::with_auto_install(
            new_search_paths,
            current_settings.languages.clone(),
            current_settings.capture_mappings.clone(),
            current_settings.auto_install,
        );

        // Apply the updated settings
        self.apply_settings(updated_settings).await;

        // Ensure the language is loaded BEFORE parsing
        // apply_settings only stores configuration but doesn't load the parser.
        // parse_document uses detect_language which checks has_parser_available.
        // Without ensure_language_loaded, has_parser_available returns false and
        // the document won't be parsed, resulting in no syntax highlighting.
        let _load_result = self.language.ensure_language_loaded(language);

        // Re-parse the document that triggered the install
        self.parse_document(uri.clone(), text, Some(language), vec![])
            .await;

        // Request semantic tokens refresh
        if self.client.semantic_tokens_refresh().await.is_ok() {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Language '{}' loaded, semantic tokens refreshed", language),
                )
                .await;
        }
    }

    /// Check injected languages and trigger auto-install for missing parsers.
    ///
    /// This function:
    /// 1. Returns early if auto-install is not enabled
    /// 2. Gets unique injected languages from the document
    /// 3. For each language, checks if it's already loaded
    /// 4. For languages not loaded, triggers maybe_auto_install_language()
    ///
    /// The InstallingLanguages tracker in maybe_auto_install_language prevents
    /// duplicate install attempts.
    async fn check_injected_languages_auto_install(&self, uri: &Url) {
        // Early return if auto-install is not enabled
        if !self.is_auto_install_enabled() {
            return;
        }

        // Get unique injected languages from the document
        let languages = get_injected_languages(uri, &self.language, &self.documents);

        if languages.is_empty() {
            return;
        }

        // Get document text for auto-install (needed by maybe_auto_install_language)
        let text = match self.documents.get(uri) {
            Some(doc) => doc.text().to_string(),
            None => return,
        };

        // Check each injected language and trigger auto-install if not loaded
        for lang in languages {
            // ADR-0005: Try direct identifier first, then normalize alias
            // This ensures "py" -> "python" before auto-install
            let resolved_lang = if self.language.has_parser_available(&lang) {
                lang.clone()
            } else if let Some(normalized) = crate::language::alias::normalize_alias(&lang) {
                normalized
            } else {
                lang.clone()
            };

            let load_result = self.language.ensure_language_loaded(&resolved_lang);
            if !load_result.success {
                // Language not loaded - trigger auto-install with resolved name
                // maybe_auto_install_language uses InstallingLanguages to prevent duplicates
                self.maybe_auto_install_language(&resolved_lang, uri.clone(), text.clone())
                    .await;
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
            self.root_path.store(Arc::new(Some(path.clone())));
        } else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    "Failed to determine workspace root - config file will not be loaded",
                )
                .await;
        }

        let root_path = self.root_path.load().as_ref().clone();
        let settings_outcome = load_settings(
            root_path.as_deref(),
            params
                .initialization_options
                .map(|options| (SettingsSource::InitializationOptions, options)),
        );
        self.report_settings_events(&settings_outcome.events).await;

        // Always apply settings (use defaults if none were loaded)
        // This ensures auto_install=true and other defaults are active for zero-config experience
        let settings = settings_outcome
            .settings
            .unwrap_or_else(|| WorkspaceSettings::from(TreeSitterSettings::default()));
        self.apply_settings(settings).await;

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
        let language_id = params.text_document.language_id.clone();
        let uri = params.text_document.uri.clone();
        let text = params.text_document.text.clone();

        // Try to determine the language
        let language_name = self
            .language
            .get_language_for_path(uri.path())
            .or_else(|| Some(language_id.clone()));

        // Check if we need to auto-install
        if let Some(ref lang) = language_name {
            let load_result = self.language.ensure_language_loaded(lang);

            if !load_result.success && self.is_auto_install_enabled() {
                // Language failed to load and auto-install is enabled
                self.maybe_auto_install_language(lang, uri.clone(), text.clone())
                    .await;
            }
        }

        self.parse_document(
            params.text_document.uri,
            params.text_document.text,
            Some(&language_id),
            vec![], // No edits for initial document open
        )
        .await;

        // Check for injected languages and trigger auto-install for missing parsers
        // This must be called AFTER parse_document so we have access to the AST
        self.check_injected_languages_auto_install(&uri).await;

        // Check if queries are ready for the document
        if let Some(language_name) = self.get_language_for_document(&uri) {
            let has_queries = self.language.has_queries(&language_name);

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
                let has_queries_after_delay = self.language.has_queries(&language_name);

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
        self.documents.remove(&uri);

        // Clean up semantic token cache for this document
        self.semantic_cache.remove(&uri);

        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        // Retrieve the stored document info
        let (language_id, old_text) = {
            let doc = self.documents.get(&uri);
            match doc {
                Some(d) => (d.language_id().map(|s| s.to_string()), d.text().to_string()),
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
                let mapper = PositionMapper::new(&text);
                let start_offset = mapper.position_to_byte(range.start).unwrap_or(text.len());
                let end_offset = mapper.position_to_byte(range.end).unwrap_or(text.len());
                let new_end_offset = start_offset + change.text.len();

                // Calculate the new end position for tree-sitter (using byte columns)
                let lines: Vec<&str> = change.text.split('\n').collect();
                let line_count = lines.len();
                // last_line_len is in BYTES (not UTF-16) because .len() on &str returns byte count
                let last_line_len = lines.last().map(|l| l.len()).unwrap_or(0);

                // Get start position with proper byte column conversion
                let start_point =
                    mapper
                        .position_to_point(range.start)
                        .unwrap_or(tree_sitter::Point::new(
                            range.start.line as usize,
                            start_offset,
                        ));

                // Calculate new end Point (tree-sitter uses byte columns)
                let new_end_point = if line_count > 1 {
                    // New content spans multiple lines
                    tree_sitter::Point::new(
                        start_point.row + line_count - 1,
                        last_line_len, // byte length of last line
                    )
                } else {
                    // New content is on same line as start
                    tree_sitter::Point::new(
                        start_point.row,
                        start_point.column + last_line_len, // add byte length
                    )
                };

                // Create InputEdit for incremental parsing
                let edit = InputEdit {
                    start_byte: start_offset,
                    old_end_byte: end_offset,
                    new_end_byte: new_end_offset,
                    start_position: start_point,
                    old_end_position: mapper
                        .position_to_point(range.end)
                        .unwrap_or(tree_sitter::Point::new(range.end.line as usize, end_offset)),
                    new_end_position: new_end_point,
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

        // Invalidate semantic token cache to ensure fresh tokens for delta calculations
        self.semantic_cache.remove(&uri);

        // Check for injected languages and trigger auto-install for missing parsers
        // This must be called AFTER parse_document so we have access to the updated AST
        self.check_injected_languages_auto_install(&uri).await;

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
        let root_path = self.root_path.load().as_ref().clone();
        let settings_outcome = load_settings(
            root_path.as_deref(),
            Some((SettingsSource::ClientConfiguration, params.settings)),
        );
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
        let Some(query) = self.language.get_highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get document data and compute tokens, then drop the reference
        let result = {
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };
            let text = doc.text();
            let Some(tree) = doc.tree() else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self
                .parser_pool
                .lock()
                .recover_poison("semantic_tokens_full parser_pool")
                .unwrap();
            crate::analysis::handle_semantic_tokens_full(
                text,
                tree,
                &query,
                Some(&language_name),
                Some(&capture_mappings),
                Some(&self.language),
                Some(&mut pool),
            )
        }; // doc reference is dropped here

        let mut tokens_with_id = match result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        }) {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(tokens) => tokens,
            tower_lsp::lsp_types::SemanticTokensResult::Partial(_) => {
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                }
            }
        };
        // Use atomic sequential ID for efficient cache validation
        tokens_with_id.result_id = Some(next_result_id());
        let stored_tokens = tokens_with_id.clone();
        let lsp_tokens = tokens_with_id;
        // Store in dedicated cache for delta requests with result_id validation
        self.semantic_cache
            .store(uri.clone(), stored_tokens.clone());
        self.documents.update_semantic_tokens(&uri, stored_tokens);
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

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Get document data and compute delta, then drop the reference
        let result = {
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            let text = doc.text();
            let Some(tree) = doc.tree() else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            // Get previous tokens from cache with result_id validation
            let previous_tokens = self.semantic_cache.get_if_valid(&uri, &previous_result_id);

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self
                .parser_pool
                .lock()
                .recover_poison("semantic_tokens_full_delta parser_pool")
                .unwrap();

            // Delegate to handler with injection support
            handle_semantic_tokens_full_delta(
                text,
                tree,
                &query,
                &previous_result_id,
                previous_tokens.as_ref(),
                Some(&language_name),
                Some(&capture_mappings),
                Some(&self.language),
                Some(&mut pool),
            )
        }; // doc reference is dropped here

        let domain_result = result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensFullDeltaResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        });

        match domain_result {
            tower_lsp::lsp_types::SemanticTokensFullDeltaResult::Tokens(tokens) => {
                let mut tokens_with_id = tokens;
                // Use atomic sequential ID for efficient cache validation
                tokens_with_id.result_id = Some(next_result_id());
                let stored_tokens = tokens_with_id.clone();
                let lsp_tokens = tokens_with_id;
                // Store in dedicated cache for next delta request
                self.semantic_cache
                    .store(uri.clone(), stored_tokens.clone());
                self.documents.update_semantic_tokens(&uri, stored_tokens);
                Ok(Some(SemanticTokensFullDeltaResult::Tokens(lsp_tokens)))
            }
            other => Ok(Some(other)),
        }
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let domain_range = range;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(doc) = self.documents.get(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let text = doc.text();
        let Some(tree) = doc.tree() else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get capture mappings
        let capture_mappings = self.language.get_capture_mappings();

        // Use injection-aware handler (works with or without injection support)
        let mut pool = self
            .parser_pool
            .lock()
            .recover_poison("semantic_tokens_range parser_pool")
            .unwrap();
        let result = crate::analysis::handle_semantic_tokens_range(
            text,
            tree,
            &query,
            &domain_range,
            Some(&language_name),
            Some(&capture_mappings),
            Some(&self.language),
            Some(&mut pool),
        );

        // Convert to RangeResult, treating partial responses as empty for now
        let domain_range_result = match result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        }) {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(tokens) => {
                tower_lsp::lsp_types::SemanticTokensRangeResult::from(tokens)
            }
            tower_lsp::lsp_types::SemanticTokensResult::Partial(partial) => {
                tower_lsp::lsp_types::SemanticTokensRangeResult::from(partial)
            }
        };

        Ok(Some(domain_range_result))
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
        let Some(locals_query) = self.language.get_locals_query(&language_name) else {
            return Ok(None);
        };

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        // Use layer-aware handler
        let resolver = DefinitionResolver::new();
        let response = handle_goto_definition(&resolver, &doc, position, &locals_query, &uri);

        Ok(response.and_then(|resp| match &resp {
            GotoDefinitionResponse::Array(locations) if locations.is_empty() => None,
            _ => Some(resp),
        }))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions = params.positions;

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        // Use full injection parsing handler with coordinator and parser pool
        let mut pool = self
            .parser_pool
            .lock()
            .recover_poison("selection_range parser_pool")
            .unwrap();
        let result = handle_selection_range(&doc, &positions, &self.language, &mut pool);

        Ok(Some(result))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document and tree
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text();
        let Some(tree) = doc.tree() else {
            return Ok(None);
        };

        let domain_range = range;

        // Get language for the document
        let language_name = self.get_language_for_document(&uri);

        // Get capture mappings
        let capture_mappings = self.language.get_capture_mappings();
        let capture_context = language_name.as_deref().map(|ft| (ft, &capture_mappings));

        // Get queries and delegate to handler
        let lsp_response = if let Some(lang) = language_name.clone() {
            let highlight_query = self.language.get_highlight_query(&lang);
            let locals_query = self.language.get_locals_query(&lang);
            let injection_query = self.language.get_injection_query(&lang);

            let queries = highlight_query
                .as_ref()
                .map(|hq| (hq.as_ref(), locals_query.as_ref().map(|lq| lq.as_ref())));

            // Build code action options with injection support
            let mut options =
                crate::analysis::refactor::CodeActionOptions::new(&uri, text, tree, domain_range)
                    .with_queries(queries)
                    .with_capture_context(capture_context);

            // Add injection query if available
            if let Some(inj_q) = injection_query.as_ref() {
                options = options.with_injection(inj_q.as_ref());
            }

            // Use coordinator if we can get parser pool lock
            if let Ok(mut pool) = self.parser_pool.lock() {
                options = options.with_coordinator(&self.language, &mut pool);
                handle_code_actions(options)
            } else {
                // Fallback without coordinator
                handle_code_actions(options)
            }
        } else {
            handle_code_actions(crate::analysis::refactor::CodeActionOptions::new(
                &uri,
                text,
                tree,
                domain_range,
            ))
        };

        Ok(lsp_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

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

    // Note: InstallingLanguages and get_injected_languages tests are in auto_install.rs

    #[test]
    fn test_check_injected_languages_identifies_missing_parsers() {
        // Test that check_injected_languages_auto_install correctly identifies
        // which injected languages need auto-installation (parsers not loaded).
        //
        // The function should:
        // 1. Get injected languages from the document using get_injected_languages()
        // 2. For each language, call ensure_language_loaded() to check if parser exists
        // 3. If parser is NOT loaded AND autoInstall is enabled, trigger maybe_auto_install_language()
        // 4. Skip languages that are already loaded or already being installed
        //
        // This test verifies the logic by checking what languages would be identified
        // as needing installation based on ensure_language_loaded() results.

        use crate::language::LanguageCoordinator;

        // Create a LanguageCoordinator to test ensure_language_loaded behavior
        let coordinator = LanguageCoordinator::new();

        // Test that ensure_language_loaded returns false for unknown languages
        // These are the languages that should trigger auto-install
        let unknown_langs = vec!["lua", "python", "rust"];
        for lang in &unknown_langs {
            let result = coordinator.ensure_language_loaded(lang);
            // Without any language configured, ensure_language_loaded should fail
            assert!(
                !result.success,
                "Expected ensure_language_loaded to fail for unconfigured language '{}'",
                lang
            );
        }

        // This verifies the core logic: if ensure_language_loaded().success is false,
        // the language should be a candidate for auto-installation.

        // The check_injected_languages_auto_install method will use this pattern:
        // 1. let languages = self.get_injected_languages(uri);
        // 2. for lang in languages {
        //        let load_result = self.language.ensure_language_loaded(&lang);
        //        if !load_result.success {
        //            self.maybe_auto_install_language(&lang, uri, text).await;
        //        }
        //    }

        // Verify that InstallingLanguages tracker would prevent duplicate installs
        let tracker = InstallingLanguages::new();
        assert!(tracker.try_start_install("lua"));
        assert!(!tracker.try_start_install("lua")); // Second attempt fails
        tracker.finish_install("lua");
        assert!(tracker.try_start_install("lua")); // After finish, can start again
    }

    #[test]
    fn test_get_languages_needing_install_filters_loaded_languages() {
        // Test the helper method that filters injected languages to only those
        // that need installation (not already loaded).
        //
        // This tests get_languages_needing_install() which takes a set of injected
        // language names and returns only those where ensure_language_loaded fails.

        use crate::language::LanguageCoordinator;

        let coordinator = LanguageCoordinator::new();

        // Create a set of injected languages (simulating what get_injected_languages returns)
        let mut injected_languages = HashSet::new();
        injected_languages.insert("lua".to_string());
        injected_languages.insert("python".to_string());
        injected_languages.insert("rust".to_string());

        // Call the helper method to filter to only languages needing install
        let languages_needing_install =
            get_languages_needing_install(&coordinator, &injected_languages);

        // Since no languages are configured in the coordinator, all should need install
        assert_eq!(languages_needing_install.len(), 3);
        assert!(languages_needing_install.contains(&"lua".to_string()));
        assert!(languages_needing_install.contains(&"python".to_string()));
        assert!(languages_needing_install.contains(&"rust".to_string()));
    }

    /// Helper function that filters a set of injected languages to only those
    /// that need installation (where ensure_language_loaded fails).
    ///
    /// This is the core logic used by check_injected_languages_auto_install.
    fn get_languages_needing_install(
        coordinator: &crate::language::LanguageCoordinator,
        injected_languages: &HashSet<String>,
    ) -> Vec<String> {
        injected_languages
            .iter()
            .filter(|lang| {
                let load_result = coordinator.ensure_language_loaded(lang);
                !load_result.success
            })
            .cloned()
            .collect()
    }

    // Note: Large integration tests for auto-install are in tests/test_auto_install_integration.rs
}
