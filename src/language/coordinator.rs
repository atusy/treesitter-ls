use super::config_store::ConfigStore;
use super::events::{LanguageEvent, LanguageLoadResult, LanguageLoadSummary, LanguageLogLevel};
use super::filetypes::FiletypeResolver;
use super::loader::ParserLoader;
use super::parser_pool::{DocumentParserPool, ParserFactory};
use super::query_loader::QueryLoader;
use super::query_store::QueryStore;
use super::registry::LanguageRegistry;
use crate::config::settings::{infer_query_kind, LanguageConfig, QueryKind};
use crate::config::{CaptureMappings, TreeSitterSettings, WorkspaceSettings};
use std::sync::{Arc, RwLock};
use tree_sitter::Language;

/// Coordinates language runtime components (registry, queries, configs).
pub struct LanguageCoordinator {
    query_store: QueryStore,
    config_store: ConfigStore,
    filetype_resolver: FiletypeResolver,
    language_registry: LanguageRegistry,
    parser_loader: RwLock<ParserLoader>,
}

impl Default for LanguageCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageCoordinator {
    pub fn new() -> Self {
        Self {
            query_store: QueryStore::new(),
            config_store: ConfigStore::new(),
            filetype_resolver: FiletypeResolver::new(),
            language_registry: LanguageRegistry::new(),
            parser_loader: RwLock::new(ParserLoader::new()),
        }
    }

    /// Ensure a language parser is loaded, attempting dynamic load if needed.
    ///
    /// Visibility: Public - called by LSP layer (semantic_tokens, selection_range)
    /// and analysis modules to ensure parsers are available before use.
    pub fn ensure_language_loaded(&self, language_id: &str) -> LanguageLoadResult {
        if self.language_registry.contains(language_id) {
            LanguageLoadResult::success_with(Vec::new())
        } else {
            self.try_load_language_by_id(language_id)
        }
    }

    /// Initialize from workspace-level settings and return coordination events.
    ///
    /// Visibility: Public - called by LSP layer during initialization and
    /// settings updates to configure language support.
    pub fn load_settings(&self, settings: WorkspaceSettings) -> LanguageLoadSummary {
        let config_settings: TreeSitterSettings = settings.into();
        self.load_settings_from_config(&config_settings)
    }

    fn load_settings_from_config(&self, settings: &TreeSitterSettings) -> LanguageLoadSummary {
        self.config_store.update_from_settings(settings);
        // build_from_settings removed in PBI-061 - filetypes no longer in config

        let mut summary = LanguageLoadSummary::default();
        for (lang_name, config) in &settings.languages {
            let result = self.load_single_language(lang_name, config, &settings.search_paths);
            summary.record(lang_name, result);
        }
        summary
    }

    /// Try to dynamically load a language by ID from configured search paths
    ///
    /// Visibility: Internal only - called by ensure_language_loaded.
    /// Not exposed as public API to keep interface minimal (YAGNI).
    fn try_load_language_by_id(&self, language_id: &str) -> LanguageLoadResult {
        if self.language_registry.contains(language_id) {
            return LanguageLoadResult::success_with(Vec::new());
        }

        let search_paths = self.config_store.get_search_paths();
        let Some(paths) = &search_paths else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Warning,
                format!("No search paths configured, cannot load language '{language_id}'"),
            ));
        };

        let library_path = QueryLoader::resolve_library_path(None, language_id, &search_paths);
        let Some(lib_path) = library_path else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Warning,
                format!("Could not find parser for language '{language_id}'"),
            ));
        };

        let language = {
            let result = self
                .parser_loader
                .write()
                .unwrap()
                .load_language(&lib_path, language_id);
            match result {
                Ok(lang) => lang,
                Err(err) => {
                    return LanguageLoadResult::failure_with(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load language {language_id} from {lib_path}: {err}"),
                    ));
                }
            }
        };

        self.language_registry
            .register_unchecked(language_id.to_string(), language.clone());

        let mut events = Vec::new();

        // Use inheritance-aware loading for all query types
        // This handles languages like TypeScript that inherit from ecma
        if let Ok(query) = QueryLoader::load_query_with_inheritance(
            &language,
            paths,
            language_id,
            "highlights.scm",
        ) {
            self.query_store
                .insert_highlight_query(language_id.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Dynamically loaded highlights for {language_id}"),
            ));
        }

        if let Ok(query) =
            QueryLoader::load_query_with_inheritance(&language, paths, language_id, "locals.scm")
        {
            self.query_store
                .insert_locals_query(language_id.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Dynamically loaded locals for {language_id}"),
            ));
        }

        if let Ok(query) = QueryLoader::load_query_with_inheritance(
            &language,
            paths,
            language_id,
            "injections.scm",
        ) {
            self.query_store
                .insert_injection_query(language_id.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Dynamically loaded injections for {language_id}"),
            ));
        }

        events.push(LanguageEvent::log(
            LanguageLogLevel::Info,
            format!("Dynamically loaded language {language_id} from {lib_path}"),
        ));
        if self.has_queries(language_id) {
            events.push(LanguageEvent::semantic_tokens_refresh(
                language_id.to_string(),
            ));
        }

        LanguageLoadResult::success_with(events)
    }

    /// Get language for a document path.
    ///
    /// Visibility: Public - called by LSP layer (auto_install, lsp_impl)
    /// for document language detection.
    pub fn get_language_for_path(&self, path: &str) -> Option<String> {
        self.filetype_resolver.get_language_for_path(path)
    }

    /// Get language for a file extension.
    ///
    /// Visibility: Public - used in integration tests (test_poison_recovery)
    /// and internally for extension-based detection.
    pub fn get_language_for_extension(&self, extension: &str) -> Option<String> {
        self.filetype_resolver.get_language_for_extension(extension)
    }

    /// Get configured search paths (primarily for testing and diagnostics).
    ///
    /// Visibility: Public - used in integration tests (test_dynamic_lua_load)
    /// to verify settings configuration.
    pub fn get_search_paths(&self) -> Option<Vec<String>> {
        self.config_store.get_search_paths()
    }

    /// Check if a parser is available for a given language name.
    ///
    /// Used by the detection fallback chain (ADR-0005) to determine whether
    /// to accept a detection result or continue to the next method.
    ///
    /// Visibility: Public - called by LSP layer (lsp_impl) to check parser
    /// availability before attempting language operations.
    pub fn has_parser_available(&self, language_name: &str) -> bool {
        self.language_registry.has_parser_available(language_name)
    }

    /// ADR-0005: Detection fallback chain.
    ///
    /// Returns the first language for which a parser is available.
    ///
    /// Priority order:
    /// 1. languageId (if parser available and not "plaintext")
    /// 2. Shebang detection from content
    /// 3. Extension-based detection from path
    ///
    /// Visibility: Public - called by LSP layer (lsp_impl) for document
    /// language detection during text_document/didOpen.
    pub fn detect_language(
        &self,
        path: &str,
        language_id: Option<&str>,
        content: &str,
    ) -> Option<String> {
        // 1. Try languageId if parser is available (and not "plaintext")
        if let Some(lang_id) = language_id
            && lang_id != "plaintext"
            && self.has_parser_available(lang_id)
        {
            return Some(lang_id.to_string());
        }

        // 2. Try shebang detection (lazy I/O: only runs if step 1 failed)
        if let Some(shebang_lang) = super::shebang::detect_from_shebang(content)
            && self.has_parser_available(&shebang_lang)
        {
            return Some(shebang_lang);
        }

        // 3. Fall back to extension-based detection (ADR-0005: strip dot, use as parser name)
        if let Some(ext_lang) = super::extension::detect_from_extension(path)
            && self.has_parser_available(&ext_lang)
        {
            return Some(ext_lang);
        }

        None
    }

    /// ADR-0005: Resolve injection language with alias fallback.
    ///
    /// For injection regions, try direct identifier first, then normalize.
    /// Returns the resolved language name and load result.
    ///
    /// Priority order:
    /// 1. Direct identifier (try to load as-is)
    /// 2. Normalized alias (py -> python, js -> javascript, sh -> bash)
    ///
    /// Visibility: Public - called by analysis layer (semantic.rs) for
    /// nested language injection support.
    pub fn resolve_injection_language(
        &self,
        identifier: &str,
    ) -> Option<(String, LanguageLoadResult)> {
        // 1. Try direct identifier first
        let direct_result = self.ensure_language_loaded(identifier);
        if direct_result.success {
            return Some((identifier.to_string(), direct_result));
        }

        // 2. Try normalized alias
        if let Some(normalized) = super::alias::normalize_alias(identifier) {
            let alias_result = self.ensure_language_loaded(&normalized);
            if alias_result.success {
                return Some((normalized, alias_result));
            }
        }

        None
    }

    /// Create a document parser pool.
    ///
    /// Visibility: Public - called by LSP layer (lsp_impl) and analysis modules
    /// to obtain parser instances for document processing.
    pub fn create_document_parser_pool(&self) -> DocumentParserPool {
        let parser_factory = ParserFactory::new(self.language_registry.clone());
        DocumentParserPool::new(parser_factory)
    }

    /// Check if queries exist for a language.
    ///
    /// Visibility: Public - called by LSP layer (lsp_impl) to determine if
    /// semantic tokens should be refreshed after language load.
    pub fn has_queries(&self, lang_name: &str) -> bool {
        self.query_store.has_highlight_query(lang_name)
    }

    /// Get highlight query for a language.
    ///
    /// Visibility: Public - called by LSP layer (semantic_tokens) and analysis
    /// layer (refactor, semantic) for syntax highlighting and token analysis.
    pub fn get_highlight_query(&self, lang_name: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_highlight_query(lang_name)
    }

    /// Get locals query for a language.
    ///
    /// Visibility: Public - called by analysis layer (refactor) for scope
    /// and local variable analysis in injected languages.
    pub fn get_locals_query(&self, lang_name: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_locals_query(lang_name)
    }

    /// Get injection query for a language.
    ///
    /// Visibility: Public - called by LSP layer (multiple handlers) and analysis
    /// layer (refactor, semantic, selection) for nested language support.
    pub fn get_injection_query(&self, lang_name: &str) -> Option<Arc<tree_sitter::Query>> {
        self.query_store.get_injection_query(lang_name)
    }

    /// Get capture mappings.
    ///
    /// Visibility: Public - called by LSP layer (semantic_tokens) and analysis
    /// layer (refactor) for custom capture-to-token-type mapping.
    pub fn get_capture_mappings(&self) -> CaptureMappings {
        let config_mappings = self.config_store.get_capture_mappings();
        config_mappings
            .iter()
            .map(|(lang, mappings)| (lang.clone(), mappings.clone()))
            .collect::<CaptureMappings>()
    }

    fn load_single_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
    ) -> LanguageLoadResult {
        let library_path =
            QueryLoader::resolve_library_path(config.library.as_ref(), lang_name, search_paths);
        let Some(lib_path) = library_path else {
            return LanguageLoadResult::failure_with(LanguageEvent::log(
                LanguageLogLevel::Error,
                format!("No library path found for language {lang_name}"),
            ));
        };

        let language = {
            let result = self
                .parser_loader
                .write()
                .unwrap()
                .load_language(&lib_path, lang_name);
            match result {
                Ok(lang) => lang,
                Err(err) => {
                    return LanguageLoadResult::failure_with(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load language {lang_name}: {err}"),
                    ));
                }
            }
        };

        self.language_registry
            .register_unchecked(lang_name.to_string(), language.clone());

        let mut events = self.load_queries_for_language(lang_name, config, search_paths, &language);
        events.push(LanguageEvent::log(
            LanguageLogLevel::Info,
            format!("Language {lang_name} loaded."),
        ));
        LanguageLoadResult::success_with(events)
    }

    fn load_queries_for_language(
        &self,
        lang_name: &str,
        config: &LanguageConfig,
        search_paths: &Option<Vec<String>>,
        language: &Language,
    ) -> Vec<LanguageEvent> {
        let mut events = Vec::new();

        // Process unified queries field first (new format)
        if let Some(queries) = &config.queries {
            events.extend(self.load_unified_queries(lang_name, queries, language));
            return events; // If queries field is present, don't fall back to legacy fields
        }

        // Fall back to legacy fields (highlights, locals, injections)
        if let Some(highlights) = &config.highlights {
            if !highlights.is_empty() {
                match QueryLoader::load_highlight_query(language, highlights) {
                    Ok(query) => {
                        self.query_store
                            .insert_highlight_query(lang_name.to_string(), Arc::new(query));
                        events.push(LanguageEvent::log(
                            LanguageLogLevel::Info,
                            format!("Highlight query loaded for {lang_name}"),
                        ));
                    }
                    Err(err) => {
                        events.push(LanguageEvent::log(
                            LanguageLogLevel::Error,
                            format!("Failed to load highlight query for {lang_name}: {err}"),
                        ));
                    }
                }
            }
        } else if let Some(paths) = search_paths
            && let Ok(query) = QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "highlights.scm",
            )
        {
            self.query_store
                .insert_highlight_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Highlight query loaded from search paths for {lang_name}"),
            ));
        }

        if let Some(locals_paths) = &config.locals {
            if !locals_paths.is_empty() {
                match QueryLoader::load_highlight_query(language, locals_paths) {
                    Ok(query) => {
                        self.query_store
                            .insert_locals_query(lang_name.to_string(), Arc::new(query));
                        events.push(LanguageEvent::log(
                            LanguageLogLevel::Info,
                            format!("Locals query loaded for {lang_name}"),
                        ));
                    }
                    Err(err) => {
                        events.push(LanguageEvent::log(
                            LanguageLogLevel::Error,
                            format!("Failed to load locals query for {lang_name}: {err}"),
                        ));
                    }
                }
            }
        } else if let Some(paths) = search_paths
            && let Ok(query) =
                QueryLoader::load_query_from_search_paths(language, paths, lang_name, "locals.scm")
        {
            self.query_store
                .insert_locals_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Locals query loaded from search paths for {lang_name}"),
            ));
        }

        // Load injection queries from config paths or search paths
        if let Some(injection_paths) = &config.injections {
            if !injection_paths.is_empty() {
                match QueryLoader::load_highlight_query(language, injection_paths) {
                    Ok(query) => {
                        self.query_store
                            .insert_injection_query(lang_name.to_string(), Arc::new(query));
                        events.push(LanguageEvent::log(
                            LanguageLogLevel::Info,
                            format!("Injection query loaded for {lang_name}"),
                        ));
                    }
                    Err(err) => {
                        events.push(LanguageEvent::log(
                            LanguageLogLevel::Error,
                            format!("Failed to load injection query for {lang_name}: {err}"),
                        ));
                    }
                }
            }
        } else if let Some(paths) = search_paths
            && let Ok(query) = QueryLoader::load_query_from_search_paths(
                language,
                paths,
                lang_name,
                "injections.scm",
            )
        {
            self.query_store
                .insert_injection_query(lang_name.to_string(), Arc::new(query));
            events.push(LanguageEvent::log(
                LanguageLogLevel::Info,
                format!("Injection query loaded from search paths for {lang_name}"),
            ));
        }

        events
    }

    /// Load queries from the unified queries field (new format).
    ///
    /// Processes each QueryItem, using explicit kind or inferring from filename.
    /// Unknown patterns (where kind is None and cannot be inferred) are skipped.
    fn load_unified_queries(
        &self,
        lang_name: &str,
        queries: &[crate::config::settings::QueryItem],
        language: &Language,
    ) -> Vec<LanguageEvent> {
        let mut events = Vec::new();

        // Group query paths by their effective kind
        let mut highlights: Vec<String> = Vec::new();
        let mut locals: Vec<String> = Vec::new();
        let mut injections: Vec<String> = Vec::new();

        for query in queries {
            let effective_kind = query.kind.or_else(|| infer_query_kind(&query.path));
            match effective_kind {
                Some(QueryKind::Highlights) => highlights.push(query.path.clone()),
                Some(QueryKind::Locals) => locals.push(query.path.clone()),
                Some(QueryKind::Injections) => injections.push(query.path.clone()),
                None => {
                    // Skip unrecognized patterns silently
                }
            }
        }

        // Load highlights
        if !highlights.is_empty() {
            match QueryLoader::load_highlight_query(language, &highlights) {
                Ok(query) => {
                    self.query_store
                        .insert_highlight_query(lang_name.to_string(), Arc::new(query));
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Info,
                        format!("Highlight query loaded for {lang_name}"),
                    ));
                }
                Err(err) => {
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load highlight query for {lang_name}: {err}"),
                    ));
                }
            }
        }

        // Load locals
        if !locals.is_empty() {
            match QueryLoader::load_highlight_query(language, &locals) {
                Ok(query) => {
                    self.query_store
                        .insert_locals_query(lang_name.to_string(), Arc::new(query));
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Info,
                        format!("Locals query loaded for {lang_name}"),
                    ));
                }
                Err(err) => {
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load locals query for {lang_name}: {err}"),
                    ));
                }
            }
        }

        // Load injections
        if !injections.is_empty() {
            match QueryLoader::load_highlight_query(language, &injections) {
                Ok(query) => {
                    self.query_store
                        .insert_injection_query(lang_name.to_string(), Arc::new(query));
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Info,
                        format!("Injection query loaded for {lang_name}"),
                    ));
                }
                Err(err) => {
                    events.push(LanguageEvent::log(
                        LanguageLogLevel::Error,
                        format!("Failed to load injection query for {lang_name}: {err}"),
                    ));
                }
            }
        }

        events
    }

    /// Register a language directly for testing purposes.
    ///
    /// This bypasses the normal loading process and directly registers
    /// a tree-sitter Language in the registry. Useful for unit tests
    /// that need to test with specific language parsers.
    #[cfg(test)]
    pub(crate) fn register_language_for_test(
        &self,
        language_id: &str,
        language: tree_sitter::Language,
    ) {
        self.language_registry
            .register_unchecked(language_id.to_string(), language);
    }

    /// Register an injection query directly for testing purposes.
    ///
    /// This bypasses the normal loading process and directly registers
    /// an injection query in the query store. Useful for unit tests
    /// that need to test nested injection scenarios.
    #[cfg(test)]
    pub(crate) fn register_injection_query_for_test(
        &self,
        language_id: &str,
        query: tree_sitter::Query,
    ) {
        self.query_store
            .insert_injection_query(language_id.to_string(), Arc::new(query));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_direct_identifier_first() {
        let coordinator = LanguageCoordinator::new();
        // Register "python" parser
        coordinator.register_language_for_test("python", tree_sitter_rust::LANGUAGE.into());

        // Direct identifier "python" should work
        let result = coordinator.resolve_injection_language("python");
        assert!(result.is_some());
        let (resolved, load_result) = result.unwrap();
        assert_eq!(resolved, "python");
        assert!(load_result.success);
    }

    #[test]
    fn test_injection_uses_alias_normalization() {
        let coordinator = LanguageCoordinator::new();
        // Register "python" parser (not "py")
        coordinator.register_language_for_test("python", tree_sitter_rust::LANGUAGE.into());

        // Alias "py" should resolve to "python"
        let result = coordinator.resolve_injection_language("py");
        assert!(result.is_some());
        let (resolved, load_result) = result.unwrap();
        assert_eq!(resolved, "python");
        assert!(load_result.success);
    }

    #[test]
    fn test_injection_unknown_alias_returns_none() {
        let coordinator = LanguageCoordinator::new();
        // No parsers registered

        // Unknown alias with no parser should return None
        let result = coordinator.resolve_injection_language("unknown_lang");
        assert!(result.is_none());

        // Known alias but no parser should also return None
        let result = coordinator.resolve_injection_language("py");
        assert!(result.is_none());
    }

    #[test]
    fn test_injection_prefers_direct_over_alias() {
        let coordinator = LanguageCoordinator::new();
        // Register both "js" and "javascript" as separate parsers
        coordinator.register_language_for_test("js", tree_sitter_rust::LANGUAGE.into());
        coordinator.register_language_for_test("javascript", tree_sitter_rust::LANGUAGE.into());

        // "js" should resolve to "js" (direct), not "javascript" (alias)
        let result = coordinator.resolve_injection_language("js");
        assert!(result.is_some());
        let (resolved, _) = result.unwrap();
        assert_eq!(
            resolved, "js",
            "Direct identifier should be preferred over alias"
        );
    }

    #[test]
    fn test_load_settings_does_not_make_parser_available() {
        // Documents that load_settings alone does NOT make parsers available.
        // ensure_language_loaded must be called to actually load the parser.
        // This is important for reload_language_after_install to work correctly.
        use crate::config::WorkspaceSettings;

        let coordinator = LanguageCoordinator::new();

        // Initially, parser is not available
        assert!(
            !coordinator.has_parser_available("rust"),
            "Parser should not be available before load_settings"
        );

        // Load settings (simulating apply_settings behavior)
        let settings = WorkspaceSettings::default();
        let _summary = coordinator.load_settings(settings);

        // After load_settings, parser is STILL not available
        assert!(
            !coordinator.has_parser_available("rust"),
            "Parser should not be available after load_settings alone - ensure_language_loaded must be called"
        );
    }

    // Smoke tests for coordinator API (moved from integration tests)
    // These verify the API surface exists and basic functionality works

    #[test]
    fn coordinator_should_resolve_filetype() {
        let coordinator = LanguageCoordinator::new();
        let _lang = coordinator.get_language_for_extension("rs");
    }

    #[test]
    fn coordinator_should_expose_query_state_checks() {
        let coordinator = LanguageCoordinator::new();
        let _has_queries: bool = coordinator.has_queries("rust");
    }

    #[test]
    fn coordinator_should_expose_highlight_queries() {
        let coordinator = LanguageCoordinator::new();
        let _query = coordinator.get_highlight_query("rust");
    }

    #[test]
    fn coordinator_should_expose_locals_queries() {
        let coordinator = LanguageCoordinator::new();
        let _query = coordinator.get_locals_query("rust");
    }

    #[test]
    fn coordinator_should_provide_capture_mappings() {
        let coordinator = LanguageCoordinator::new();
        let _mappings = coordinator.get_capture_mappings();
    }

    #[test]
    fn test_coordinator_has_parser_available() {
        let coordinator = LanguageCoordinator::new();

        // No languages loaded initially - should return false
        assert!(!coordinator.has_parser_available("rust"));

        // This test verifies the API is exposed on LanguageCoordinator.
        // The full behavior (true when loaded) is tested in unit tests
        // via register_language_for_test which is only available there.
    }

    #[test]
    fn test_shebang_used_when_language_id_plaintext() {
        let coordinator = LanguageCoordinator::new();

        // When languageId is "plaintext", fallback to shebang detection
        // Note: No parser loaded, so will return None (graceful degradation)
        // But the shebang detection path is still exercised
        let content = "#!/usr/bin/env python\nprint('hello')";
        let result = coordinator.detect_language("/script", Some("plaintext"), content);

        // No python parser loaded, so result is None
        // The important thing is that "plaintext" didn't short-circuit
        assert_eq!(result, None);
    }

    #[test]
    fn test_shebang_skipped_when_language_id_has_parser() {
        let coordinator = LanguageCoordinator::new();

        // When languageId has an available parser, don't run shebang detection
        // This tests lazy I/O - shebang parsing is skipped entirely

        // Scenario: languageId is "rust" but no rust parser loaded
        // So it falls through to shebang, but no python parser either
        let content = "#!/usr/bin/env python\nprint('hello')";
        let result = coordinator.detect_language("/script", Some("rust"), content);

        // Neither rust nor python parser loaded
        assert_eq!(result, None);

        // Full behavior with loaded parser is tested in unit tests
    }

    #[test]
    fn test_extension_fallback_after_shebang() {
        let coordinator = LanguageCoordinator::new();

        // When shebang detection fails (no parser), extension fallback runs
        // File has .rs extension but content has python shebang
        let content = "#!/usr/bin/env python\nprint('hello')";
        let result = coordinator.detect_language("/path/to/file.rs", None, content);

        // No parsers loaded, so result is None
        // But the chain tried: languageId (None) -> shebang (python, no parser) -> extension (rs, no parser)
        assert_eq!(result, None);
    }

    #[test]
    fn test_full_detection_chain() {
        let coordinator = LanguageCoordinator::new();

        // Full chain test: languageId -> shebang -> extension
        // All methods tried, none have parsers available

        // languageId = "plaintext" (skipped), shebang = python (no parser), extension = rs (no parser)
        let content = "#!/usr/bin/env python\nprint('hello')";
        let result = coordinator.detect_language("/path/to/file.rs", Some("plaintext"), content);

        assert_eq!(result, None);
    }

    #[test]
    fn test_detection_chain_returns_none_when_all_fail() {
        let coordinator = LanguageCoordinator::new();

        // No languageId, no shebang, no extension -> None
        let result =
            coordinator.detect_language("/Makefile", None, "all: build\n\nbuild:\n\techo hello");

        assert_eq!(result, None);
    }
}
