mod text_document;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::InputEdit;

use crate::analysis::next_result_id;
use crate::analysis::{
    InjectionMap, InjectionTokenCache, LEGEND_MODIFIERS, LEGEND_TYPES, SemanticTokenCache,
};
use crate::config::{
    TreeSitterSettings, WorkspaceSettings, resolve_language_server_with_wildcard,
    resolve_language_settings_with_wildcard,
};
use crate::document::DocumentStore;
use crate::language::injection::{CacheableInjectionRegion, collect_all_injections};
use crate::language::{DocumentParserPool, FailedParserRegistry, LanguageCoordinator};
use crate::language::{LanguageEvent, LanguageLogLevel};
use crate::lsp::{SettingsEvent, SettingsEventKind, SettingsSource, load_settings};
use crate::text::PositionMapper;
use arc_swap::ArcSwap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use super::auto_install::{InstallingLanguages, get_injected_languages};
use super::progress::{create_progress_begin, create_progress_end};
use super::semantic_request_tracker::SemanticRequestTracker;

/// Parse a JSON notification and extract ProgressParams if it's a $/progress notification.
///
/// Returns None if:
/// - The notification is not a $/progress notification
/// - The params field cannot be parsed as ProgressParams
fn parse_progress_notification(notification: &serde_json::Value) -> Option<ProgressParams> {
    // Check if this is a $/progress notification
    let method = notification.get("method")?.as_str()?;
    if method != "$/progress" {
        return None;
    }

    // Extract and parse the params field
    let params = notification.get("params")?;
    serde_json::from_value::<ProgressParams>(params.clone()).ok()
}

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
    /// Sender for client notifications to be forwarded to bridge.
    /// Kept alive for server lifetime to prevent channel from closing.
    tokio_notification_tx: Option<tokio::sync::mpsc::UnboundedSender<serde_json::Value>>,
    /// Receiver for client notifications to be forwarded to bridge.
    /// Wrapped in Option so it can be taken once when starting the forwarder task.
    tokio_notification_rx:
        tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>>>,
    /// Tracks active semantic token requests for cancellation support
    semantic_request_tracker: SemanticRequestTracker,
    /// Language server pool for bridging requests to injection language servers
    language_server_pool: crate::lsp::bridge::LanguageServerPool,
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
            .finish_non_exhaustive()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        let language = LanguageCoordinator::new();
        let parser_pool = language.create_document_parser_pool();

        // Initialize failed parser registry with crash detection
        let failed_parsers = Self::init_failed_parser_registry();

        // Note: Startup cleanup of bridge temp directories removed with bridge module.
        // When bridge is re-implemented (ADR-0012), add cleanup here if needed.

        // Create notification channel for async bridge
        // Store sender to keep channel alive for server lifetime (PBI-191)
        let (tokio_notification_tx, tokio_notification_rx) =
            tokio::sync::mpsc::unbounded_channel();

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
            tokio_notification_tx: Some(tokio_notification_tx),
            tokio_notification_rx: tokio::sync::Mutex::new(Some(tokio_notification_rx)),
            semantic_request_tracker: SemanticRequestTracker::new(),
            language_server_pool: crate::lsp::bridge::LanguageServerPool::new(),
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
    ///
    /// Returns `false` if:
    /// - `autoInstall` is explicitly set to `false` in settings
    /// - `searchPaths` doesn't include the default data directory (auto-install
    ///   would install to a location that isn't being searched)
    fn is_auto_install_enabled(&self) -> bool {
        let settings = self.settings.load();

        // If explicitly disabled, return false
        if !settings.auto_install {
            return false;
        }

        // Check if searchPaths includes the default data directory
        // If not, auto-install would be useless (installed parsers wouldn't be found)
        self.search_paths_include_default_data_dir(&settings.search_paths)
    }

    /// Check if the given search paths include the default data directory.
    fn search_paths_include_default_data_dir(&self, search_paths: &[String]) -> bool {
        let Some(default_dir) = crate::install::default_data_dir() else {
            // Can't determine default dir - allow auto-install anyway
            return true;
        };

        let default_str = default_dir.to_string_lossy();
        search_paths.iter().any(|p| p == default_str.as_ref())
    }

    /// Forward a client notification to the notification channel for bridge processing.
    ///
    /// PBI-191: This method sends notifications (didChange, didSave, didClose) through
    /// the tokio_notification_tx channel to the notification forwarder task, which then
    /// routes them to the bridge layer.
    ///
    /// # Arguments
    /// * `notification` - The JSON-RPC notification to forward
    ///
    /// # Returns
    /// * `Ok(())` if notification was sent successfully
    /// * `Err` if sender is None or channel is closed
    async fn handle_client_notification(
        &self,
        notification: serde_json::Value,
    ) -> Result<()> {
        if let Some(ref sender) = self.tokio_notification_tx {
            sender
                .send(notification)
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            Ok(())
        } else {
            Err(tower_lsp::jsonrpc::Error::internal_error())
        }
    }

    /// Notify user that parser is missing and needs manual installation.
    ///
    /// Called when a parser fails to load and auto-install is disabled
    /// (either explicitly or because searchPaths doesn't include the default data dir).
    async fn notify_parser_missing(&self, language: &str) {
        let settings = self.settings.load();

        // Check why auto-install is disabled
        let reason = if !settings.auto_install {
            "autoInstall is disabled".to_string()
        } else if !self.search_paths_include_default_data_dir(&settings.search_paths) {
            let default_dir = crate::install::default_data_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            format!(
                "searchPaths does not include the default data directory ({})",
                default_dir
            )
        } else {
            "unknown reason".to_string()
        };

        self.client
            .log_message(
                MessageType::WARNING,
                format!(
                    "Parser for '{}' not found. Auto-install is disabled because {}. \
                     Please install the parser manually using: treesitter-ls language install {}",
                    language, reason, language
                ),
            )
            .await;
    }

    /// Invalidate injection caches for regions that overlap with edits.
    ///
    /// Called BEFORE parse_document to use pre-edit byte offsets against pre-edit
    /// injection regions. This implements AC4/AC5 (PBI-083): edits outside injections
    /// preserve caches, edits inside invalidate only affected regions.
    ///
    /// PBI-167: Uses O(log n) interval tree query instead of O(n) iteration.
    fn invalidate_overlapping_injection_caches(&self, uri: &Url, edits: &[InputEdit]) {
        if edits.is_empty() {
            return;
        }

        // Find all regions that overlap with any edit using O(log n) queries
        for edit in edits {
            let edit_start = edit.start_byte;
            let edit_end = edit.old_end_byte;

            // Query interval tree for overlapping regions (O(log n) instead of O(n))
            if let Some(overlapping_regions) = self
                .injection_map
                .find_overlapping(uri, edit_start, edit_end)
            {
                for region in overlapping_regions {
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
            // Narrow critical section: checkout parser → release lock → parse in spawn_blocking → return parser
            let parsed_tree = {
                // Checkout parser from pool (brief lock)
                let parser = {
                    let mut pool = self.parser_pool.lock().await;
                    pool.acquire(&language_name)
                };

                if let Some(mut parser) = parser {
                    let old_tree = if !edits.is_empty() {
                        self.documents.get_edited_tree(&uri, &edits)
                    } else {
                        self.documents.get(&uri).and_then(|doc| doc.tree().cloned())
                    };

                    let language_name_clone = language_name.clone();
                    let text_clone = text.clone();
                    let failed_parsers = self.failed_parsers.clone();

                    // Parse in spawn_blocking to avoid blocking tokio worker thread
                    let result = tokio::task::spawn_blocking(move || {
                        // Record that we're about to parse (for crash detection)
                        let _ = failed_parsers.begin_parsing(&language_name_clone);

                        let parse_result = parser.parse(&text_clone, old_tree.as_ref());

                        // Parsing succeeded without crash - clear the state for this language
                        let _ = failed_parsers.end_parsing_language(&language_name_clone);

                        (parser, parse_result)
                    })
                    .await
                    .ok();

                    if let Some((parser, parse_result)) = result {
                        // Return parser to pool (brief lock)
                        let mut pool = self.parser_pool.lock().await;
                        pool.release(language_name.clone(), parser);
                        parse_result
                    } else {
                        None
                    }
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

    /// Get bridge server config for a given injection language from settings.
    ///
    /// Looks up the bridge.servers configuration and finds a server that handles
    /// the specified language. Returns None if:
    /// - No server is configured for this injection language, OR
    /// - The host language has a bridge filter that excludes this injection language
    ///
    /// Uses wildcard resolution (ADR-0011) for host language lookup:
    /// - If host language is not defined, inherits from languages._ if present
    /// - This allows setting default bridge filters for all hosts via languages._
    ///
    /// # Arguments
    /// * `host_language` - The language of the host document (e.g., "markdown")
    /// * `injection_language` - The injection language to bridge (e.g., "rust", "python")
    fn get_bridge_config_for_language(
        &self,
        host_language: &str,
        injection_language: &str,
    ) -> Option<crate::config::settings::BridgeServerConfig> {
        let settings = self.settings.load();

        // Use wildcard resolution for host language lookup (ADR-0011)
        // This allows languages._ to define default bridge filters
        if let Some(host_settings) =
            resolve_language_settings_with_wildcard(&settings.languages, host_language)
            && !host_settings.is_language_bridgeable(injection_language)
        {
            log::debug!(
                target: "treesitter_ls::bridge",
                "Bridge filter for {} blocks injection language {}",
                host_language,
                injection_language
            );
            return None;
        }

        // Check if language servers exist
        if let Some(ref servers) = settings.language_servers {
            // Look for a server that handles this language
            // ADR-0011: Resolve each server with wildcard BEFORE checking languages,
            // because languages list may be inherited from languageServers._
            for server_name in servers.keys() {
                // Skip wildcard entry - we use it for inheritance, not direct lookup
                if server_name == "_" {
                    continue;
                }

                if let Some(resolved_config) =
                    resolve_language_server_with_wildcard(servers, server_name)
                        .filter(|c| c.languages.iter().any(|l| l == injection_language))
                {
                    return Some(resolved_config);
                }
            }
        }

        None
    }

    async fn apply_settings(&self, settings: WorkspaceSettings) {
        // Store settings for auto_install check
        self.settings.store(Arc::new(settings.clone()));
        let summary = self.language.load_settings(settings);
        self.handle_language_events(&summary.events).await;
    }

    /// Start the background task that forwards $/progress notifications to the LSP client.
    ///
    /// This takes ownership of the notification receiver and spawns a task that:
    /// 1. Polls the receiver for notifications from the tokio async pool
    /// 2. Parses each notification to extract ProgressParams
    /// 3. Forwards valid progress notifications to the client
    ///
    /// This method is called once from `initialized()` after the LSP handshake completes.
    async fn start_notification_forwarder(&self) {
        // Take the receiver from the Option (can only be done once)
        let mut rx_guard = self.tokio_notification_rx.lock().await;
        let Some(mut rx) = rx_guard.take() else {
            // Receiver already taken (shouldn't happen, but defensive)
            log::warn!(
                target: "treesitter_ls::notification_forwarder",
                "Notification receiver already taken, forwarder not started"
            );
            return;
        };
        drop(rx_guard); // Release the lock before spawning

        let client = self.client.clone();

        // Spawn the forwarder task
        tokio::spawn(async move {
            log::debug!(
                target: "treesitter_ls::notification_forwarder",
                "Notification forwarder started"
            );

            while let Some(notification) = rx.recv().await {
                if let Some(progress_params) = parse_progress_notification(&notification) {
                    log::debug!(
                        target: "treesitter_ls::notification_forwarder",
                        "Forwarding $/progress notification"
                    );
                    client.send_notification::<Progress>(progress_params).await;
                }
            }

            log::debug!(
                target: "treesitter_ls::notification_forwarder",
                "Notification forwarder stopped (channel closed)"
            );
        });
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
    ///
    /// # Returns
    /// `true` if installation was triggered (caller should skip parse_document),
    /// `false` if installation was not triggered (caller should proceed with parse_document)
    async fn maybe_auto_install_language(
        &self,
        language: &str,
        uri: Url,
        text: String,
        is_injection: bool,
    ) -> bool {
        // Check if language is supported by nvim-treesitter before attempting install
        // Use cached metadata to avoid repeated HTTP requests
        let default_data_dir = crate::install::default_data_dir();
        let fetch_options =
            default_data_dir
                .as_ref()
                .map(|dir| crate::install::metadata::FetchOptions {
                    data_dir: Some(dir.as_path()),
                    use_cache: true,
                });
        let (should_skip, reason) =
            super::auto_install::should_skip_unsupported_language(language, fetch_options.as_ref())
                .await;
        if let Some(reason) = &reason {
            let message_type = reason.message_type();
            let message = reason.message();
            self.client.log_message(message_type, message).await;
        }
        if should_skip {
            return false; // Not supported - no install triggered
        }

        // Try to start installation (returns false if already installing)
        if !self.installing_languages.try_start_install(language) {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("Language '{}' is already being installed", language),
                )
                .await;
            return true; // Already installing - caller should skip parse
        }

        // Send progress Begin notification
        self.client
            .send_notification::<Progress>(create_progress_begin(language))
            .await;

        // Get data directory
        let data_dir = match default_data_dir {
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
                return false; // No data dir - install failed, no parse needed
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
            return true; // Parser exists - reload triggered, caller should skip parse
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

        // Installation was triggered and will complete in background
        // reload_language_after_install will handle parse_document
        true
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
        let updated_settings = WorkspaceSettings::with_language_servers(
            new_search_paths,
            current_settings.languages.clone(),
            current_settings.capture_mappings.clone(),
            current_settings.auto_install,
            current_settings.language_servers.clone(),
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

    /// Check injected languages and handle missing parsers.
    ///
    /// This function:
    /// 1. Gets unique injected languages from the document
    /// 2. For each language, checks if it's already loaded
    /// 3. If not loaded and auto-install is enabled, triggers maybe_auto_install_language()
    /// 4. If not loaded and auto-install is disabled, notifies user
    ///
    /// The InstallingLanguages tracker in maybe_auto_install_language prevents
    /// duplicate install attempts.
    async fn check_injected_languages_auto_install(&self, uri: &Url) {
        // Get unique injected languages from the document
        let languages = get_injected_languages(uri, &self.language, &self.documents);

        if languages.is_empty() {
            return;
        }

        let auto_install_enabled = self.is_auto_install_enabled();

        // Get document text for auto-install (needed by maybe_auto_install_language)
        let text = if auto_install_enabled {
            self.documents.get(uri).map(|doc| doc.text().to_string())
        } else {
            None
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
                if auto_install_enabled {
                    if let Some(ref text) = text {
                        // Language not loaded - trigger auto-install with resolved name
                        // maybe_auto_install_language uses InstallingLanguages to prevent duplicates
                        // is_injection=true: Don't re-parse the document with injection language
                        // Return value ignored - for injections we never skip parsing (host document already parsed)
                        let _ = self
                            .maybe_auto_install_language(
                                &resolved_lang,
                                uri.clone(),
                                text.clone(),
                                true,
                            )
                            .await;
                    }
                } else {
                    // Notify user that parser is missing and needs manual installation
                    self.notify_parser_missing(&resolved_lang).await;
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
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    ..Default::default()
                }),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: Some(vec![",".to_string()]),
                    ..Default::default()
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        // Start the background task that forwards $/progress notifications to the client
        self.start_notification_forwarder().await;

        self.client
            .log_message(MessageType::INFO, "server is ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        // Persist crash detection state before shutdown
        // This enables crash recovery to detect if parsing was in progress
        if let Err(e) = self.failed_parsers.persist_state() {
            log::warn!(
                target: "treesitter_ls::crash_recovery",
                "Failed to persist crash detection state on shutdown: {}",
                e
            );
        }
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
        let mut skip_parse = false; // Track if auto-install was triggered

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

            if !load_result.success {
                if self.is_auto_install_enabled() {
                    // Language failed to load and auto-install is enabled
                    // is_injection=false: This is the document's main language
                    // If install is triggered, skip parse_document here - reload_language_after_install will handle it
                    skip_parse = self
                        .maybe_auto_install_language(lang, uri.clone(), text.clone(), false)
                        .await;
                } else {
                    // Notify user that parser is missing and needs manual installation
                    self.notify_parser_missing(lang).await;
                }
            }
        }

        // Only parse if auto-install was NOT triggered
        // If auto-install was triggered, reload_language_after_install will call parse_document
        // after the parser file is completely written, preventing race condition
        if !skip_parse {
            self.parse_document(
                params.text_document.uri,
                params.text_document.text,
                Some(&language_id),
                vec![], // No edits for initial document open
            )
            .await;
        }

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

        // Cancel any pending semantic token requests for this document
        self.semantic_request_tracker.cancel_all_for_uri(&uri);

        // Note: When async bridge is re-implemented (ADR-0012), add cleanup call here:
        // self.language_server_pool.close_documents_for_host(uri.as_str()).await;

        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;

        self.client
            .log_message(MessageType::LOG, format!("[DID_CHANGE] START uri={}", uri))
            .await;

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
        self.semantic_tokens_full_impl(params).await
    }

    async fn semantic_tokens_full_delta(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        self.semantic_tokens_full_delta_impl(params).await
    }

    async fn semantic_tokens_range(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        self.semantic_tokens_range_impl(params).await
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        self.selection_range_impl(params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.goto_definition_impl(params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.hover_impl(params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.completion_impl(params).await
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        self.signature_help_impl(params).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::BridgeLanguageConfig;
    use std::collections::{HashMap, HashSet};

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

    /// Test that parse_progress_notification correctly extracts ProgressParams from JSON.
    ///
    /// This is the core logic used by the notification forwarder to parse
    /// $/progress notifications before forwarding to the client.
    #[test]
    fn test_parse_progress_notification_extracts_params() {
        // Create a mock $/progress notification as the reader task would receive
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "rust-analyzer/indexing",
                "value": {
                    "kind": "begin",
                    "title": "Indexing",
                    "message": "0/100 crates"
                }
            }
        });

        // Parse using the same logic as the forwarder
        let progress_params = parse_progress_notification(&notification);

        // Verify the params were extracted correctly
        assert!(
            progress_params.is_some(),
            "Should successfully parse $/progress notification"
        );

        let params = progress_params.unwrap();
        match params.token {
            NumberOrString::String(s) => {
                assert_eq!(s, "rust-analyzer/indexing");
            }
            NumberOrString::Number(_) => {
                panic!("Expected string token");
            }
        }
    }

    /// Test that parse_progress_notification returns None for non-progress notifications.
    #[test]
    fn test_parse_progress_notification_returns_none_for_other_methods() {
        // Create a different notification (not $/progress)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///test.rs",
                "diagnostics": []
            }
        });

        let progress_params = parse_progress_notification(&notification);

        assert!(
            progress_params.is_none(),
            "Should return None for non-progress notifications"
        );
    }

    /// Test that parse_progress_notification returns None for malformed params.
    #[test]
    fn test_parse_progress_notification_returns_none_for_malformed_params() {
        // Create a $/progress notification with invalid params structure
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "invalid_field": "this is not a valid ProgressParams"
            }
        });

        let progress_params = parse_progress_notification(&notification);

        // ProgressParams requires 'token' and 'value' fields
        assert!(
            progress_params.is_none(),
            "Should return None for malformed $/progress params"
        );
    }

    /// PBI-155 Subtask 2: Test wildcard language config inheritance
    ///
    /// This test verifies that languages._ (wildcard) settings are inherited
    /// by specific languages when looking up language configs.
    ///
    /// The key behavior:
    /// - languages._ defines default bridge settings (e.g., disable all by default)
    /// - languages.markdown overrides only bridge for rust (enable it)
    /// - When looking up "quarto" (not defined), it should inherit from languages._
    #[test]
    fn test_language_config_inherits_from_wildcard() {
        use crate::config::settings::BridgeLanguageConfig;
        use crate::config::{LanguageConfig, resolve_language_with_wildcard};

        let mut languages: HashMap<String, LanguageConfig> = HashMap::new();

        // Wildcard language: disable bridging by default (empty bridge filter)
        let wildcard_bridge = HashMap::new(); // Empty = disable all bridging
        languages.insert(
            "_".to_string(),
            LanguageConfig {
                library: None,
                queries: None,
                highlights: Some(vec!["/default/highlights.scm".to_string()]),
                locals: None,
                injections: None,
                bridge: Some(wildcard_bridge),
            },
        );

        // Markdown: enable only rust bridging
        let mut markdown_bridge = HashMap::new();
        markdown_bridge.insert("rust".to_string(), BridgeLanguageConfig { enabled: true });
        languages.insert(
            "markdown".to_string(),
            LanguageConfig {
                library: None,
                queries: None,
                highlights: None, // Should inherit from wildcard
                locals: None,
                injections: None,
                bridge: Some(markdown_bridge),
            },
        );

        // Test 1: "markdown" should have its own bridge filter (not wildcard's)
        let markdown = resolve_language_with_wildcard(&languages, "markdown").unwrap();
        assert!(
            markdown.highlights.is_some(),
            "markdown should inherit highlights from wildcard"
        );
        assert_eq!(
            markdown.highlights.as_ref().unwrap(),
            &vec!["/default/highlights.scm".to_string()],
            "markdown should inherit highlights from wildcard"
        );
        // Bridge should be markdown-specific, not inherited from wildcard
        let bridge = markdown.bridge.as_ref().unwrap();
        assert!(
            bridge.get("rust").is_some(),
            "markdown bridge should have rust entry"
        );

        // Test 2: "quarto" (not defined) should get wildcard settings entirely
        let quarto = resolve_language_with_wildcard(&languages, "quarto").unwrap();
        assert!(
            quarto.highlights.is_some(),
            "quarto should inherit highlights from wildcard"
        );
        // Bridge should be wildcard's empty filter (disable all)
        let quarto_bridge = quarto.bridge.as_ref().unwrap();
        assert!(
            quarto_bridge.is_empty(),
            "quarto should inherit empty bridge filter from wildcard"
        );
    }

    /// PBI-155 Subtask 2: Test that LanguageSettings lookup uses wildcard resolution
    ///
    /// This test verifies the wiring: when we look up host language settings
    /// using WorkspaceSettings.languages (HashMap<String, LanguageSettings>),
    /// we should use wildcard resolution so that undefined languages inherit
    /// from languages._ settings.
    ///
    /// Since get_bridge_config_for_language needs TreeSitterLs which is hard to instantiate
    /// in unit tests, we verify the behavior at the LanguageSettings level.
    #[test]
    fn test_language_settings_wildcard_lookup_blocks_bridging_for_undefined_host() {
        use crate::config::{LanguageSettings, resolve_language_settings_with_wildcard};

        let mut languages: HashMap<String, LanguageSettings> = HashMap::new();

        // Wildcard: block all bridging with empty filter
        languages.insert(
            "_".to_string(),
            LanguageSettings::with_bridge(None, None, Some(HashMap::new())),
        );

        // Look up "quarto" which doesn't exist - should inherit from wildcard
        let quarto = resolve_language_settings_with_wildcard(&languages, "quarto");
        assert!(
            quarto.is_some(),
            "Looking up undefined 'quarto' should return wildcard settings"
        );

        let quarto_settings = quarto.unwrap();
        // The wildcard has empty bridge filter, so is_language_bridgeable should return false
        assert!(
            !quarto_settings.is_language_bridgeable("rust"),
            "quarto (inherited from wildcard) should block bridging for rust"
        );
        assert!(
            !quarto_settings.is_language_bridgeable("python"),
            "quarto (inherited from wildcard) should block bridging for python"
        );
    }

    /// PBI-155 Subtask 3: Test that server lookup uses wildcard resolution
    ///
    /// This test verifies that when looking up a language server config by name,
    /// the wildcard server settings (languageServers._) are merged with specific
    /// server settings.
    ///
    /// Key behavior:
    /// - languageServers._ defines default initialization options
    /// - languageServers.rust-analyzer overrides only the cmd
    /// - The resolved rust-analyzer should have both cmd (from specific) and
    ///   initialization_options (inherited from wildcard)
    #[test]
    fn test_language_server_config_inherits_from_wildcard() {
        use crate::config::{resolve_language_server_with_wildcard, settings::BridgeServerConfig};
        use serde_json::json;

        let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

        // Wildcard server: default initialization options and workspace_type
        servers.insert(
            "_".to_string(),
            BridgeServerConfig {
                cmd: vec![],
                languages: vec![],
                initialization_options: Some(json!({ "checkOnSave": true })),
                workspace_type: Some(crate::config::settings::WorkspaceType::Generic),
            },
        );

        // rust-analyzer: only specifies cmd and languages
        servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec!["rust-analyzer".to_string()],
                languages: vec!["rust".to_string()],
                initialization_options: None, // Should inherit from wildcard
                workspace_type: None,         // Should inherit from wildcard
            },
        );

        // Test: rust-analyzer should merge with wildcard
        let ra = resolve_language_server_with_wildcard(&servers, "rust-analyzer").unwrap();

        // cmd from specific
        assert_eq!(ra.cmd, vec!["rust-analyzer".to_string()]);
        // languages from specific
        assert_eq!(ra.languages, vec!["rust".to_string()]);
        // initialization_options inherited from wildcard
        assert!(ra.initialization_options.is_some());
        let opts = ra.initialization_options.as_ref().unwrap();
        assert_eq!(opts.get("checkOnSave"), Some(&json!(true)));
        // workspace_type inherited from wildcard
        assert_eq!(
            ra.workspace_type,
            Some(crate::config::settings::WorkspaceType::Generic)
        );
    }

    /// Test that server lookup finds servers when languages list is inherited from wildcard.
    ///
    /// ADR-0011: When languageServers.rust-analyzer has empty languages but
    /// languageServers._ specifies languages = ["rust"], the lookup should still
    /// find rust-analyzer for Rust injections because the languages list is
    /// inherited from the wildcard during resolution.
    ///
    /// This tests the fix for a bug where get_bridge_config_for_language checked
    /// the unresolved config.languages before applying wildcard resolution.
    #[test]
    fn test_language_server_lookup_uses_resolved_languages_from_wildcard() {
        use crate::config::{resolve_language_server_with_wildcard, settings::BridgeServerConfig};

        let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

        // Wildcard server: specifies languages = ["rust", "python"]
        servers.insert(
            "_".to_string(),
            BridgeServerConfig {
                cmd: vec!["default-lsp".to_string()],
                languages: vec!["rust".to_string(), "python".to_string()],
                initialization_options: None,
                workspace_type: None,
            },
        );

        // rust-analyzer: specifies only cmd, inherits languages from wildcard
        servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec!["rust-analyzer".to_string()],
                languages: vec![], // Empty - should inherit from wildcard
                initialization_options: None,
                workspace_type: None,
            },
        );

        // Simulate the lookup logic from get_bridge_config_for_language:
        // For each server (excluding "_"), resolve it and check if it handles "rust"
        let injection_language = "rust";
        let mut found_server: Option<BridgeServerConfig> = None;

        for server_name in servers.keys() {
            if server_name == "_" {
                continue;
            }

            if let Some(resolved_config) =
                resolve_language_server_with_wildcard(&servers, server_name)
            {
                if resolved_config
                    .languages
                    .iter()
                    .any(|l| l == injection_language)
                {
                    found_server = Some(resolved_config);
                    break;
                }
            }
        }

        // Should find rust-analyzer because after resolution it has languages = ["rust", "python"]
        assert!(
            found_server.is_some(),
            "Should find a server for 'rust' when languages is inherited from wildcard"
        );
        let server = found_server.unwrap();
        assert_eq!(
            server.cmd,
            vec!["rust-analyzer".to_string()],
            "Should find rust-analyzer server"
        );
        assert!(
            server.languages.contains(&"rust".to_string()),
            "Resolved server should have 'rust' in languages (inherited from wildcard)"
        );
    }

    #[test]
    fn test_bridge_router_respects_host_filter() {
        // PBI-108 AC4: Bridge filtering is applied at request time before routing to language servers
        // This test verifies that is_language_bridgeable is correctly integrated into
        // the bridge routing logic.
        //
        // The actual routing happens in get_bridge_config_for_language which:
        // 1. Looks up host language settings
        // 2. Calls is_language_bridgeable to check filter
        // 3. Returns None if filter blocks the injection language
        //
        // We test the is_language_bridgeable logic directly since get_bridge_config_for_language
        // requires full server initialization which is tested in E2E tests.

        use crate::config::LanguageSettings;

        // Host markdown with bridge filter: only python and r enabled
        let mut bridge_filter = HashMap::new();
        bridge_filter.insert("python".to_string(), BridgeLanguageConfig { enabled: true });
        bridge_filter.insert("r".to_string(), BridgeLanguageConfig { enabled: true });
        let markdown_settings = LanguageSettings::with_bridge(None, None, Some(bridge_filter));

        // Router should allow python (enabled in filter)
        assert!(
            markdown_settings.is_language_bridgeable("python"),
            "Bridge router should allow python for markdown"
        );

        // Router should allow r (enabled in filter)
        assert!(
            markdown_settings.is_language_bridgeable("r"),
            "Bridge router should allow r for markdown"
        );

        // Router should block rust (not in filter)
        assert!(
            !markdown_settings.is_language_bridgeable("rust"),
            "Bridge router should block rust for markdown"
        );

        // Host quarto with no bridge filter (default: all)
        let quarto_settings = LanguageSettings::new(None, None);

        // Router should allow all languages
        assert!(
            quarto_settings.is_language_bridgeable("python"),
            "Bridge router should allow python for quarto (no filter)"
        );
        assert!(
            quarto_settings.is_language_bridgeable("rust"),
            "Bridge router should allow rust for quarto (no filter)"
        );

        // Host rmd with empty bridge filter (disable all)
        let rmd_settings = LanguageSettings::with_bridge(None, None, Some(HashMap::new()));

        // Router should block all languages
        assert!(
            !rmd_settings.is_language_bridgeable("r"),
            "Bridge router should block r for rmd (empty filter)"
        );
        assert!(
            !rmd_settings.is_language_bridgeable("python"),
            "Bridge router should block python for rmd (empty filter)"
        );
    }

    /// PBI-191 Subtask 1: Test that TreeSitterLs stores tokio_notification_tx sender field.
    ///
    /// This test verifies that the notification channel sender is stored in the TreeSitterLs
    /// struct and kept alive for the server's lifetime. Without this field, the sender is
    /// dropped at the end of new(), causing the receiver to immediately close.
    ///
    /// Test strategy: Create TreeSitterLs instance, verify sender can send after construction.
    #[tokio::test]
    async fn test_notification_sender_kept_alive() {
        // Create a mock client for TreeSitterLs
        let (service, _) = tower_lsp::LspService::new(|client| TreeSitterLs::new(client));
        let server = service.inner();

        // Verify that sender field exists and is kept alive
        assert!(
            server.tokio_notification_tx.is_some(),
            "TreeSitterLs should store tokio_notification_tx sender to keep channel alive"
        );

        // Verify sender is not closed (would be closed if dropped prematurely)
        let sender = server.tokio_notification_tx.as_ref().unwrap();
        assert!(
            !sender.is_closed(),
            "Sender should not be closed after TreeSitterLs construction"
        );
    }

    /// PBI-191 Subtask 2: Test that handle_client_notification() sends notifications through channel.
    ///
    /// This test verifies that when handle_client_notification() is called with a notification,
    /// it forwards the notification through the tokio_notification_tx channel instead of dropping it.
    ///
    /// Test strategy: Call handle_client_notification(), verify receiver gets the notification.
    #[tokio::test]
    async fn test_handle_client_notification_sends_to_channel() {
        // Create a mock client for TreeSitterLs
        let (service, _) = tower_lsp::LspService::new(|client| TreeSitterLs::new(client));
        let server = service.inner();

        // Create test notification (didChange)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": "file:///test.lua",
                    "version": 2
                },
                "contentChanges": [{
                    "text": "local x = 1"
                }]
            }
        });

        // Send notification through handle_client_notification
        server
            .handle_client_notification(notification.clone())
            .await
            .expect("handle_client_notification should succeed");

        // Verify notification was sent to channel by receiving it
        let mut rx_guard = server.tokio_notification_rx.lock().await;
        let rx = rx_guard.as_mut().expect("Receiver should exist");

        // Try to receive with timeout to avoid hanging if nothing was sent
        let received = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            rx.recv()
        )
        .await
        .expect("Should receive notification within timeout")
        .expect("Channel should not be closed");

        assert_eq!(
            received, notification,
            "Received notification should match sent notification"
        );
    }
}
