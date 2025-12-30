use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::InputEdit;

// Note: position_to_point from selection.rs is deprecated - use PositionMapper.position_to_point() instead
use crate::analysis::{
    IncrementalDecision, compute_incremental_tokens, decide_tokenization_strategy,
    decode_semantic_tokens, encode_semantic_tokens, handle_code_actions, handle_selection_range,
    handle_semantic_tokens_full_delta, next_result_id,
};
use crate::analysis::{
    InjectionMap, InjectionTokenCache, LEGEND_MODIFIERS, LEGEND_TYPES, SemanticTokenCache,
};
use crate::config::{TreeSitterSettings, WorkspaceSettings};
use crate::document::DocumentStore;
use crate::error::LockResultExt;
use crate::language::injection::{CacheableInjectionRegion, collect_all_injections};
use crate::language::{DocumentParserPool, FailedParserRegistry, LanguageCoordinator};
use crate::language::{LanguageEvent, LanguageLogLevel};
use crate::lsp::{SettingsEvent, SettingsEventKind, SettingsSource, load_settings};
use crate::text::PositionMapper;
use arc_swap::ArcSwap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::auto_install::{InstallingLanguages, get_injected_languages};
use super::progress::{create_progress_begin, create_progress_end};
use super::redirection::RustAnalyzerPool;

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
    /// Tracks injection regions per document for targeted invalidation
    injection_map: InjectionMap,
    /// Per-injection semantic token cache (AC4/AC5 targeted invalidation)
    injection_token_cache: InjectionTokenCache,
    root_path: ArcSwap<Option<PathBuf>>,
    /// Settings including auto_install flag
    settings: ArcSwap<WorkspaceSettings>,
    /// Tracks languages currently being installed
    installing_languages: InstallingLanguages,
    /// Tracks parsers that have crashed
    failed_parsers: FailedParserRegistry,
    /// Pool of rust-analyzer connections for Rust injection redirection
    rust_analyzer_pool: RustAnalyzerPool,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("language", &"LanguageCoordinator")
            .field("parser_pool", &"Mutex<DocumentParserPool>")
            .field("documents", &"DocumentStore")
            .field("semantic_cache", &"SemanticTokenCache")
            .field("injection_map", &"InjectionMap")
            .field("injection_token_cache", &"InjectionTokenCache")
            .field("root_path", &"ArcSwap<Option<PathBuf>>")
            .field("settings", &"ArcSwap<WorkspaceSettings>")
            .field("installing_languages", &"InstallingLanguages")
            .field("failed_parsers", &"FailedParserRegistry")
            .field("rust_analyzer_pool", &"RustAnalyzerPool")
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
            injection_map: InjectionMap::new(),
            injection_token_cache: InjectionTokenCache::new(),
            root_path: ArcSwap::new(Arc::new(None)),
            settings: ArcSwap::new(Arc::new(WorkspaceSettings::default())),
            installing_languages: InstallingLanguages::new(),
            failed_parsers,
            rust_analyzer_pool: RustAnalyzerPool::new(),
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

    /// Invalidate injection caches for regions that overlap with edits.
    ///
    /// Called BEFORE parse_document to use pre-edit byte offsets against pre-edit
    /// injection regions. This implements AC4/AC5 (PBI-083): edits outside injections
    /// preserve caches, edits inside invalidate only affected regions.
    fn invalidate_overlapping_injection_caches(&self, uri: &Url, edits: &[InputEdit]) {
        // Get pre-edit injection regions
        let Some(regions) = self.injection_map.get(uri) else {
            return; // No injection regions tracked for this document
        };

        if regions.is_empty() || edits.is_empty() {
            return;
        }

        // Find all regions that overlap with any edit
        for edit in edits {
            let edit_start = edit.start_byte;
            let edit_end = edit.old_end_byte;

            for region in &regions {
                // Check if edit overlaps with region's byte range
                // Overlap: edit_start < region_end AND edit_end > region_start
                if edit_start < region.byte_range.end && edit_end > region.byte_range.start {
                    // This region is affected - invalidate its cache
                    self.injection_token_cache.remove(uri, &region.result_id);
                    log::debug!(
                        target: "treesitter_ls::injection_cache",
                        "Invalidated injection cache for {} region (edit bytes {}..{})",
                        region.language,
                        edit_start,
                        edit_end
                    );
                }
            }
        }
    }

    /// Populate InjectionMap with injection regions from the parsed tree.
    ///
    /// This enables targeted cache invalidation (PBI-083): when an edit occurs,
    /// we can check which injection regions overlap and only invalidate those.
    ///
    /// AC6: Also clears stale InjectionTokenCache entries for removed regions.
    /// Since result_ids are regenerated on each parse, we clear the entire
    /// document's injection token cache and let it be repopulated on demand.
    fn populate_injection_map(
        &self,
        uri: &Url,
        text: &str,
        tree: &tree_sitter::Tree,
        language_name: &str,
    ) {
        // Get the injection query for this language
        let injection_query = match self.language.get_injection_query(language_name) {
            Some(q) => q,
            None => {
                // No injection query = no injections to track
                // Clear any stale injection caches
                self.injection_map.clear(uri);
                self.injection_token_cache.clear_document(uri);
                return;
            }
        };

        // Collect all injection regions from the parsed tree
        if let Some(regions) =
            collect_all_injections(&tree.root_node(), text, Some(injection_query.as_ref()))
        {
            if regions.is_empty() {
                // Clear any existing regions and caches for this document
                self.injection_map.clear(uri);
                self.injection_token_cache.clear_document(uri);
                return;
            }

            // Build map of existing regions by (language, content_hash) for stable ID matching
            // This enables cache reuse when document structure changes but injection content stays same
            let existing_regions = self.injection_map.get(uri);
            let existing_by_hash: std::collections::HashMap<
                (&str, u64),
                &CacheableInjectionRegion,
            > = existing_regions
                .as_ref()
                .map(|regions| {
                    regions
                        .iter()
                        .map(|r| ((r.language.as_str(), r.content_hash), r))
                        .collect()
                })
                .unwrap_or_default();

            // Convert to CacheableInjectionRegion, reusing result_ids for unchanged content
            let cacheable_regions: Vec<CacheableInjectionRegion> = regions
                .iter()
                .map(|info| {
                    // Compute hash for the new region's content
                    let temp_region = CacheableInjectionRegion::from_region_info(info, "", text);
                    let key = (info.language.as_str(), temp_region.content_hash);

                    // Check if we have an existing region with same (language, content_hash)
                    if let Some(existing) = existing_by_hash.get(&key) {
                        // Reuse the existing result_id - this enables cache hit!
                        CacheableInjectionRegion {
                            language: temp_region.language,
                            byte_range: temp_region.byte_range,
                            line_range: temp_region.line_range,
                            result_id: existing.result_id.clone(),
                            content_hash: temp_region.content_hash,
                        }
                    } else {
                        // New content - generate new result_id
                        CacheableInjectionRegion {
                            result_id: next_result_id(),
                            ..temp_region
                        }
                    }
                })
                .collect();

            // Find stale region IDs that are no longer present
            if let Some(old_regions) = existing_regions {
                let new_hashes: std::collections::HashSet<_> = cacheable_regions
                    .iter()
                    .map(|r| (r.language.as_str(), r.content_hash))
                    .collect();
                for old in old_regions.iter() {
                    if !new_hashes.contains(&(old.language.as_str(), old.content_hash)) {
                        // This region no longer exists - clear its cache
                        self.injection_token_cache.remove(uri, &old.result_id);
                    }
                }
            }

            // Store in injection map
            self.injection_map.insert(uri.clone(), cacheable_regions);
        }
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
                // Populate InjectionMap with injection regions for targeted cache invalidation
                self.populate_injection_map(&uri, &text, &tree, &language_name);

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
    ///
    /// # Arguments
    /// * `language` - The language to install
    /// * `uri` - The document URI that triggered the install
    /// * `text` - The document text
    /// * `is_injection` - True if this is an injection language (not the document's main language)
    async fn maybe_auto_install_language(
        &self,
        language: &str,
        uri: Url,
        text: String,
        is_injection: bool,
    ) {
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
            self.reload_language_after_install(language, &data_dir, uri, text, is_injection)
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
            self.reload_language_after_install(&lang, &data_dir, uri, text, is_injection)
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
            self.reload_language_after_install(&lang, &data_dir, uri, text, is_injection)
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

    /// Reload a language after installation and optionally re-parse the document.
    ///
    /// # Arguments
    /// * `language` - The language that was installed
    /// * `data_dir` - The data directory where parsers/queries are stored
    /// * `uri` - The document URI that triggered the install
    /// * `text` - The document text (used for re-parsing document languages)
    /// * `is_injection` - If true, this is an injection language and we should NOT
    ///   re-parse the document (which would use the wrong language).
    ///   Instead, we just refresh semantic tokens so the injection
    ///   gets highlighted on next request.
    async fn reload_language_after_install(
        &self,
        language: &str,
        data_dir: &std::path::Path,
        uri: Url,
        text: String,
        is_injection: bool,
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

        // Ensure the language is loaded
        // apply_settings only stores configuration but doesn't load the parser.
        let _load_result = self.language.ensure_language_loaded(language);

        // For document languages, re-parse the document that triggered the install.
        // For injection languages, DON'T re-parse - the host document is already parsed
        // with the correct language. Re-parsing with the injection language would break
        // all highlighting. Instead, just refresh semantic tokens.
        if !is_injection {
            // Get the host language for this document (not the installed language)
            let host_language = self.get_language_for_document(&uri);
            let lang_for_parse = host_language.as_deref();
            self.parse_document(uri.clone(), text, lang_for_parse, vec![])
                .await;
        }

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
                // is_injection=true: Don't re-parse the document with injection language
                self.maybe_auto_install_language(&resolved_lang, uri.clone(), text.clone(), true)
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
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
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

        // Insert document immediately (without tree) so concurrent requests can find it.
        // This handles race conditions where semanticTokens/full arrives before
        // parse_document completes. The tree will be updated by parse_document.
        self.documents
            .insert(uri.clone(), text.clone(), language_name.clone(), None);

        // Check if we need to auto-install
        let mut deferred_events = Vec::new();
        if let Some(ref lang) = language_name {
            let load_result = self.language.ensure_language_loaded(lang);

            // Defer SemanticTokensRefresh events until after parse_document completes
            // to avoid race condition where tokens are requested before tree exists.
            // Log events immediately but defer refresh.
            for event in &load_result.events {
                match event {
                    crate::language::LanguageEvent::SemanticTokensRefresh { .. } => {
                        deferred_events.push(event.clone());
                    }
                    _ => {
                        self.handle_language_events(std::slice::from_ref(event))
                            .await;
                    }
                }
            }

            if !load_result.success && self.is_auto_install_enabled() {
                // Language failed to load and auto-install is enabled
                // is_injection=false: This is the document's main language
                self.maybe_auto_install_language(lang, uri.clone(), text.clone(), false)
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

        // Now handle deferred SemanticTokensRefresh events after document is parsed
        if !deferred_events.is_empty() {
            self.handle_language_events(&deferred_events).await;
        }

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

        // Invalidate injection caches for regions overlapping with edits (AC4/AC5)
        // Must be called BEFORE parse_document which updates the injection_map
        self.invalidate_overlapping_injection_caches(&uri, &edits);

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

        // Ensure language is loaded before trying to get queries.
        // This handles the race condition where semanticTokens/full arrives
        // before didOpen finishes loading the language.
        let load_result = self.language.ensure_language_loaded(&language_name);
        if !load_result.success {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        }

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
            let text = doc.text().to_string();
            let tree = match doc.tree() {
                Some(t) => t.clone(),
                None => {
                    // Document has no tree yet - parse it now.
                    // This handles the race condition where semantic tokens are
                    // requested before didOpen finishes parsing.
                    drop(doc); // Release lock before acquiring parser pool
                    let sync_parse_result = {
                        let mut pool = self
                            .parser_pool
                            .lock()
                            .recover_poison("semantic_tokens_full sync_parse")
                            .unwrap();
                        if let Some(mut parser) = pool.acquire(&language_name) {
                            let result = parser.parse(&text, None);
                            pool.release(language_name.clone(), parser);
                            result
                        } else {
                            None
                        }
                    }; // pool lock released here

                    match sync_parse_result {
                        Some(tree) => {
                            // Update document with parsed tree
                            self.documents.update_document(
                                uri.clone(),
                                text.clone(),
                                Some(tree.clone()),
                            );
                            tree
                        }
                        None => {
                            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                                result_id: None,
                                data: vec![],
                            })));
                        }
                    }
                }
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
                &text,
                &tree,
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
        self.semantic_cache.store(uri.clone(), stored_tokens);
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

            // Get previous text for incremental tokenization
            let previous_text = doc.previous_text().map(|s| s.to_string());

            // Decide tokenization strategy based on change size
            let strategy = decide_tokenization_strategy(doc.previous_tree(), tree, text.len());

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self
                .parser_pool
                .lock()
                .recover_poison("semantic_tokens_full_delta parser_pool")
                .unwrap();

            // Incremental Tokenization Path
            // ==============================
            // When UseIncremental strategy is selected AND we have all required state:
            // 1. Decode previous tokens to absolute (line, column) format
            // 2. Compute new tokens for the ENTIRE document (needed for changed regions)
            // 3. Use Tree-sitter's changed_ranges() to find what lines changed
            // 4. Merge: preserve old tokens outside changed lines, use new for changed lines
            // 5. Encode back to delta format and compute LSP delta
            //
            // This preserves cached tokens for unchanged regions, reducing redundant work.
            // Falls back to full path if any required state is missing.
            let use_incremental = matches!(strategy, IncrementalDecision::UseIncremental)
                && previous_tokens.is_some()
                && doc.previous_tree().is_some()
                && previous_text.is_some();

            if use_incremental {
                log::debug!(
                    target: "treesitter_ls::semantic",
                    "Using incremental tokenization path"
                );

                // Safe to unwrap because we checked above
                let prev_tokens = previous_tokens.as_ref().unwrap();
                let prev_tree = doc.previous_tree().unwrap();
                let prev_text = previous_text.as_ref().unwrap();

                // Decode previous tokens to AbsoluteToken format
                let old_absolute = decode_semantic_tokens(prev_tokens);

                // Get new tokens via full computation (still needed for changed region)
                let new_tokens_result = handle_semantic_tokens_full_delta(
                    text,
                    tree,
                    &query,
                    &previous_result_id,
                    None, // Don't pass previous - we'll merge ourselves
                    Some(&language_name),
                    Some(&capture_mappings),
                    Some(&self.language),
                    Some(&mut pool),
                );

                // Extract current tokens from the result
                if let Some(result) = new_tokens_result {
                    let current_tokens = match &result {
                        SemanticTokensFullDeltaResult::Tokens(tokens) => tokens.clone(),
                        SemanticTokensFullDeltaResult::TokensDelta(_)
                        | SemanticTokensFullDeltaResult::PartialTokensDelta { .. } => {
                            // If we got a delta, we need the full tokens
                            // This shouldn't happen since we passed None for previous
                            log::warn!(
                                target: "treesitter_ls::semantic",
                                "Unexpected delta result when computing full tokens"
                            );
                            return Ok(Some(result));
                        }
                    };

                    // Decode new tokens to AbsoluteToken format
                    let new_absolute = decode_semantic_tokens(&current_tokens);

                    // Use incremental merge
                    let merge_result = compute_incremental_tokens(
                        &old_absolute,
                        prev_tree,
                        tree,
                        prev_text,
                        text,
                        &new_absolute,
                    );

                    log::debug!(
                        target: "treesitter_ls::semantic",
                        "Incremental merge: {} changed lines, line_delta={}",
                        merge_result.changed_lines.len(),
                        merge_result.line_delta
                    );

                    // Encode merged tokens back to SemanticTokens
                    let merged_tokens = encode_semantic_tokens(
                        &merge_result.tokens,
                        current_tokens.result_id.clone(),
                    );

                    // Calculate delta against original previous tokens
                    Some(crate::analysis::semantic::calculate_delta_or_full(
                        prev_tokens,
                        &merged_tokens,
                        &previous_result_id,
                    ))
                } else {
                    None
                }
            } else {
                log::debug!(
                    target: "treesitter_ls::semantic",
                    "Using full tokenization path (strategy={:?}, has_prev_tokens={}, has_prev_tree={}, has_prev_text={})",
                    strategy,
                    previous_tokens.is_some(),
                    doc.previous_tree().is_some(),
                    previous_text.is_some()
                );

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
            }
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
                self.semantic_cache.store(uri.clone(), stored_tokens);
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

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions = params.positions;

        // Get language for document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(None);
        };

        // Ensure language is loaded (handles race condition with didOpen)
        let load_result = self.language.ensure_language_loaded(&language_name);
        if !load_result.success {
            return Ok(None);
        }

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        // Check if document has a tree, if not parse it synchronously
        if doc.tree().is_none() {
            let text = doc.text().to_string();
            drop(doc); // Release lock before acquiring parser pool

            let sync_parse_result = {
                let mut pool = self
                    .parser_pool
                    .lock()
                    .recover_poison("selection_range sync_parse")
                    .unwrap();
                if let Some(mut parser) = pool.acquire(&language_name) {
                    let result = parser.parse(&text, None);
                    pool.release(language_name.clone(), parser);
                    result
                } else {
                    None
                }
            };

            if let Some(tree) = sync_parse_result {
                self.documents
                    .update_document(uri.clone(), text, Some(tree));
            } else {
                return Ok(None);
            }

            // Re-acquire document after update
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

            return Ok(Some(result));
        }

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

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "goto_definition called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
            )
            .await;

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            self.client
                .log_message(MessageType::INFO, "No document found")
                .await;
            return Ok(None);
        };
        let text = doc.text();

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "treesitter_ls::definition", "No language detected");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::definition", "Language: {}", language_name);

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("No injection query for {}", language_name),
                )
                .await;
            return Ok(None);
        };

        // Get the parse tree
        let Some(tree) = doc.tree() else {
            log::debug!(target: "treesitter_ls::definition", "No parse tree");
            return Ok(None);
        };

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            text,
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            self.client
                .log_message(MessageType::INFO, "No injections found")
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Found {} injection regions", injections.len()),
            )
            .await;

        // Convert Position to byte offset
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            log::debug!(target: "treesitter_ls::definition", "Failed to convert position to byte");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::definition", "Byte offset: {}", byte_offset);

        // Find which injection region (if any) contains this position
        // Log all regions for debugging
        for inj in &injections {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "Region {} bytes {}..{}, contains {}? {}",
                        inj.language,
                        start,
                        end,
                        byte_offset,
                        byte_offset >= start && byte_offset < end
                    ),
                )
                .await;
        }
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            // Not in an injection region
            self.client
                .log_message(MessageType::INFO, "Position not in any injection region")
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Found matching region: {}", region.language),
            )
            .await;

        // Only handle Rust for now (PoC)
        if region.language != "rust" {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Language {} not supported (only rust)", region.language),
                )
                .await;
            return Ok(None);
        }

        // Create cacheable region for position translation
        self.client
            .log_message(MessageType::INFO, "Creating cacheable region...")
            .await;
        let cacheable = crate::language::injection::CacheableInjectionRegion::from_region_info(
            region, "temp", text,
        );

        // Extract virtual document content (own it for the blocking task)
        let virtual_content = cacheable.extract_content(text).to_owned();
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Virtual content ({} chars): {}",
                    virtual_content.len(),
                    &virtual_content[..virtual_content.len().min(50)]
                ),
            )
            .await;

        // Translate host position to virtual position
        let virtual_position = cacheable.translate_host_to_virtual(position);
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Translated position: host line {} -> virtual line {}",
                    position.line, virtual_position.line
                ),
            )
            .await;

        // Create a virtual URI for the injection
        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );
        self.client
            .log_message(MessageType::INFO, format!("Virtual URI: {}", virtual_uri))
            .await;

        // Get rust-analyzer connection from pool (or spawn new one)
        // Use spawn_blocking because rust-analyzer communication is synchronous blocking I/O
        let pool_key = "rust-analyzer".to_string();
        let has_existing = self.rust_analyzer_pool.has_connection(&pool_key);
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Getting rust-analyzer from pool (existing: {})...",
                    has_existing
                ),
            )
            .await;

        // Take connection from pool (will spawn if none exists)
        let conn = match self.rust_analyzer_pool.take_connection(&pool_key) {
            Some(c) => c,
            None => {
                self.client
                    .log_message(MessageType::ERROR, "Failed to spawn rust-analyzer")
                    .await;
                return Ok(None);
            }
        };

        let virtual_uri_clone = virtual_uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            // Open the virtual document
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

            // Request definition
            let result = conn.goto_definition(&virtual_uri_clone, virtual_position);

            // Return both result and connection for pool return
            (result, conn)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let definition = match result {
            Ok((def, conn)) => {
                self.rust_analyzer_pool.return_connection(&pool_key, conn);
                def
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("spawn_blocking failed: {}", e))
                    .await;
                None
            }
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Definition response: {:?}", definition),
            )
            .await;

        // Translate response positions back to host document
        let Some(def_response) = definition else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "No definition response from rust-analyzer",
                )
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Got definition response: {:?}", def_response),
            )
            .await;

        // Map the response locations back to host document
        let mapped_response = match def_response {
            GotoDefinitionResponse::Scalar(loc) => {
                let mapped_pos = cacheable.translate_virtual_to_host(loc.range.start);
                let mapped_end = cacheable.translate_virtual_to_host(loc.range.end);
                GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: mapped_pos,
                        end: mapped_end,
                    },
                })
            }
            GotoDefinitionResponse::Array(locs) => {
                let mapped: Vec<Location> = locs
                    .into_iter()
                    .map(|loc| {
                        let mapped_pos = cacheable.translate_virtual_to_host(loc.range.start);
                        let mapped_end = cacheable.translate_virtual_to_host(loc.range.end);
                        Location {
                            uri: uri.clone(),
                            range: Range {
                                start: mapped_pos,
                                end: mapped_end,
                            },
                        }
                    })
                    .collect();
                GotoDefinitionResponse::Array(mapped)
            }
            GotoDefinitionResponse::Link(links) => {
                let mapped: Vec<LocationLink> = links
                    .into_iter()
                    .map(|link| {
                        let mapped_start =
                            cacheable.translate_virtual_to_host(link.target_range.start);
                        let mapped_end = cacheable.translate_virtual_to_host(link.target_range.end);
                        let sel_start =
                            cacheable.translate_virtual_to_host(link.target_selection_range.start);
                        let sel_end =
                            cacheable.translate_virtual_to_host(link.target_selection_range.end);
                        LocationLink {
                            origin_selection_range: link.origin_selection_range,
                            target_uri: uri.clone(),
                            target_range: Range {
                                start: mapped_start,
                                end: mapped_end,
                            },
                            target_selection_range: Range {
                                start: sel_start,
                                end: sel_end,
                            },
                        }
                    })
                    .collect();
                GotoDefinitionResponse::Link(mapped)
            }
        };

        Ok(Some(mapped_response))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "hover called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
            )
            .await;

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            self.client
                .log_message(MessageType::INFO, "No document found")
                .await;
            return Ok(None);
        };
        let text = doc.text();

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "treesitter_ls::hover", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Get the parse tree
        let Some(tree) = doc.tree() else {
            return Ok(None);
        };

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            text,
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            return Ok(None);
        };

        // Convert Position to byte offset
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            return Ok(None);
        };

        // Find which injection region (if any) contains this position
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            // Not in an injection region - return None
            return Ok(None);
        };

        // Only handle Rust for now
        if region.language != "rust" {
            return Ok(None);
        }

        // Create cacheable region for position translation
        let cacheable = crate::language::injection::CacheableInjectionRegion::from_region_info(
            region, "temp", text,
        );

        // Extract virtual document content
        let virtual_content = cacheable.extract_content(text).to_owned();

        // Translate host position to virtual position
        let virtual_position = cacheable.translate_host_to_virtual(position);

        // Create a virtual URI for the injection
        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );

        // Get rust-analyzer connection from pool
        let pool_key = "rust-analyzer".to_string();

        // Take connection from pool (will spawn if none exists)
        let conn = match self.rust_analyzer_pool.take_connection(&pool_key) {
            Some(c) => c,
            None => {
                self.client
                    .log_message(MessageType::ERROR, "Failed to spawn rust-analyzer")
                    .await;
                return Ok(None);
            }
        };

        let virtual_uri_clone = virtual_uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            // Open the virtual document
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

            // Request hover
            let result = conn.hover(&virtual_uri_clone, virtual_position);

            // Return both result and connection for pool return
            (result, conn)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let hover = match result {
            Ok((hover, conn)) => {
                self.rust_analyzer_pool.return_connection(&pool_key, conn);
                hover
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("spawn_blocking failed: {}", e))
                    .await;
                None
            }
        };

        // Translate hover response range back to host document (if present)
        let Some(mut hover_response) = hover else {
            return Ok(None);
        };

        // Translate the range if present
        if let Some(range) = hover_response.range {
            let mapped_start = cacheable.translate_virtual_to_host(range.start);
            let mapped_end = cacheable.translate_virtual_to_host(range.end);
            hover_response.range = Some(Range {
                start: mapped_start,
                end: mapped_end,
            });
        }

        Ok(Some(hover_response))
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
