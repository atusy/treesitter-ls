//! Rayon-based parallel injection processing for semantic tokens.
//!
//! This module provides work-stealing parallelism for processing language
//! injections, replacing the previous JoinSet + Semaphore async model.
//!
//! Key design:
//! - Thread-local parser caching (no cross-thread synchronization during parsing)
//! - Work-stealing via Rayon's par_iter() for top-level injections
//! - Sequential processing for nested injections (same thread, no coordination)
//! - Single spawn_blocking bridge at the top level

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use tree_sitter::{Parser, Query, Tree};

use super::injection::MAX_INJECTION_DEPTH;
use super::token_collector::{RawToken, collect_host_tokens};
use crate::config::CaptureMappings;
use crate::language::LanguageCoordinator;

// Thread-local parser cache for Rayon worker threads.
//
// Each Rayon worker thread maintains its own cache of parsers keyed by language.
// This avoids cross-thread synchronization during parallel injection processing.
thread_local! {
    static PARSER_CACHE: RefCell<HashMap<String, Parser>> = RefCell::new(HashMap::new());
}

/// Factory for creating parsers with thread-local caching.
///
/// Uses the `LanguageRegistry` to create parsers on demand, caching them
/// in thread-local storage for reuse within the same Rayon worker.
#[allow(dead_code)] // Will be used in collect_injection_tokens_parallel
pub(crate) struct ThreadLocalParserFactory {
    registry: crate::language::registry::LanguageRegistry,
}

#[allow(dead_code)] // Will be used in collect_injection_tokens_parallel
impl ThreadLocalParserFactory {
    /// Create a new factory with the given language registry.
    pub fn new(registry: crate::language::registry::LanguageRegistry) -> Self {
        Self { registry }
    }

    /// Parse text using a cached parser for the given language.
    ///
    /// The parser is created on first use and cached in thread-local storage.
    /// This method handles the borrowing internally, returning an owned Tree.
    ///
    /// # Arguments
    /// * `language_id` - The language to use for parsing
    /// * `text` - The source text to parse
    ///
    /// # Returns
    /// - `Some(tree)` if parsing succeeds
    /// - `None` if the language is not registered or parsing fails
    pub fn parse(&self, language_id: &str, text: &str) -> Option<Tree> {
        PARSER_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();

            // Get or create parser for this language
            if !cache.contains_key(language_id) {
                let language = self.registry.get(language_id)?;
                let mut parser = Parser::new();
                parser.set_language(&language).ok()?;
                cache.insert(language_id.to_string(), parser);
            }

            // Parse using the cached parser
            let parser = cache.get_mut(language_id)?;
            parser.parse(text, None)
        })
    }

    /// Check if a language is available for parsing.
    ///
    /// # Returns
    /// `true` if the language is registered in the registry
    pub fn has_language(&self, language_id: &str) -> bool {
        self.registry.contains(language_id)
    }

    /// Clear the thread-local parser cache.
    ///
    /// Useful for testing or when languages are reloaded.
    pub fn clear_cache() {
        PARSER_CACHE.with(|cache| {
            cache.borrow_mut().clear();
        });
    }
}

/// Context for processing a single injection synchronously.
///
/// This struct captures all the information needed to process one injection,
/// including the content text, language info, and position mappings.
#[allow(dead_code)] // Will be used in parallel processing
pub(crate) struct InjectionContext<'a> {
    /// The resolved language name (e.g., "lua", "python")
    pub resolved_lang: String,
    /// The highlight query for this language
    pub highlight_query: Arc<Query>,
    /// The text content of the injection
    pub content_text: &'a str,
    /// Byte offset in the host document where this injection starts
    pub host_start_byte: usize,
}

/// Process a single injection synchronously, collecting tokens.
///
/// This function parses the injection content and collects semantic tokens,
/// including any nested injections (processed recursively in the same thread).
///
/// # Arguments
/// * `ctx` - The injection context containing language and content info
/// * `factory` - Thread-local parser factory for creating parsers
/// * `coordinator` - Language coordinator for nested injection resolution
/// * `capture_mappings` - Optional capture mappings for token type translation
/// * `host_text` - The full host document text (for position calculations)
/// * `host_lines` - Pre-split lines of the host document
/// * `depth` - Current injection depth (0 = host document)
/// * `supports_multiline` - Whether the client supports multiline tokens
///
/// # Returns
/// Vector of raw tokens collected from this injection and any nested injections
#[allow(dead_code)] // Will be used in collect_injection_tokens_parallel
#[allow(clippy::too_many_arguments)]
pub(crate) fn process_injection_sync(
    ctx: &InjectionContext<'_>,
    factory: &ThreadLocalParserFactory,
    coordinator: &LanguageCoordinator,
    capture_mappings: Option<&CaptureMappings>,
    host_text: &str,
    host_lines: &[&str],
    depth: usize,
    supports_multiline: bool,
) -> Vec<RawToken> {
    // Check recursion depth
    if depth >= MAX_INJECTION_DEPTH {
        return Vec::new();
    }

    // Parse the injection content
    let Some(tree) = factory.parse(&ctx.resolved_lang, ctx.content_text) else {
        return Vec::new();
    };

    let mut tokens = Vec::new();

    // Collect tokens from this injection's highlight query
    collect_host_tokens(
        ctx.content_text,
        &tree,
        &ctx.highlight_query,
        Some(&ctx.resolved_lang),
        capture_mappings,
        host_text,
        host_lines,
        ctx.host_start_byte,
        depth,
        supports_multiline,
        &mut tokens,
    );

    // Recursively process nested injections (same thread, no parallelism)
    let nested_contexts = collect_injection_contexts_sync(
        ctx.content_text,
        &tree,
        Some(&ctx.resolved_lang),
        coordinator,
        ctx.host_start_byte,
    );

    for nested_ctx in nested_contexts {
        let nested_tokens = process_injection_sync(
            &nested_ctx,
            factory,
            coordinator,
            capture_mappings,
            host_text,
            host_lines,
            depth + 1,
            supports_multiline,
        );
        tokens.extend(nested_tokens);
    }

    tokens
}

/// Collect injection contexts from a parsed tree (sync version).
///
/// This is a synchronous version of the injection context collection that
/// works without mutable parser access. It discovers all injections in the
/// given tree and returns their contexts for processing.
#[allow(dead_code)] // Will be used by process_injection_sync
fn collect_injection_contexts_sync<'a>(
    text: &'a str,
    tree: &Tree,
    filetype: Option<&str>,
    coordinator: &LanguageCoordinator,
    content_start_byte: usize,
) -> Vec<InjectionContext<'a>> {
    use crate::language::{collect_all_injections, injection::parse_offset_directive_for_pattern};

    let current_lang = filetype.unwrap_or("unknown");
    let Some(injection_query) = coordinator.get_injection_query(current_lang) else {
        return Vec::new();
    };

    let Some(injections) = collect_all_injections(&tree.root_node(), text, Some(&injection_query))
    else {
        return Vec::new();
    };

    let mut contexts = Vec::with_capacity(injections.len());

    for injection in injections {
        let start = injection.content_node.start_byte();
        let end = injection.content_node.end_byte();

        // Validate bounds
        if start > end || end > text.len() {
            continue;
        }

        // Extract injection content for language detection
        let injection_content = &text[start..end];

        // Resolve injection language
        let Some((resolved_lang, _)) =
            coordinator.resolve_injection_language(&injection.language, injection_content)
        else {
            continue;
        };

        // Get highlight query for resolved language
        let Some(highlight_query) = coordinator.get_highlight_query(&resolved_lang) else {
            continue;
        };

        // Get offset directive if any
        let offset = parse_offset_directive_for_pattern(&injection_query, injection.pattern_index);

        // Calculate effective content range
        let content_node = injection.content_node;
        let (inj_start_byte, inj_end_byte) = if let Some(off) = offset {
            use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range};
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range(text, byte_range, off);
            (effective.start, effective.end)
        } else {
            (content_node.start_byte(), content_node.end_byte())
        };

        // Validate effective range after offset adjustment
        if inj_start_byte > inj_end_byte || inj_end_byte > text.len() {
            continue;
        }

        contexts.push(InjectionContext {
            resolved_lang,
            highlight_query,
            content_text: &text[inj_start_byte..inj_end_byte],
            host_start_byte: content_start_byte + inj_start_byte,
        });
    }

    contexts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::registry::LanguageRegistry;

    fn create_test_registry() -> LanguageRegistry {
        let registry = LanguageRegistry::new();
        registry.register_unchecked("rust".to_string(), tree_sitter_rust::LANGUAGE.into());
        registry
    }

    #[test]
    fn test_thread_local_parser_factory_parses_code() {
        let registry = create_test_registry();
        let factory = ThreadLocalParserFactory::new(registry);

        let code = "fn main() {}";
        let tree = factory.parse("rust", code);

        assert!(tree.is_some(), "Should parse registered language");
        let tree = tree.unwrap();
        assert!(
            !tree.root_node().has_error(),
            "Parse tree should not have errors"
        );
    }

    #[test]
    fn test_thread_local_parser_factory_returns_none_for_unknown() {
        let registry = create_test_registry();
        let factory = ThreadLocalParserFactory::new(registry);

        let tree = factory.parse("unknown_language", "some code");
        assert!(
            tree.is_none(),
            "Should return None for unregistered language"
        );
    }

    #[test]
    fn test_thread_local_parser_factory_caches_parser() {
        let registry = create_test_registry();
        let factory = ThreadLocalParserFactory::new(registry);

        // Clear cache first to ensure clean state
        ThreadLocalParserFactory::clear_cache();

        // First parse creates and caches parser
        let tree1 = factory.parse("rust", "fn main() {}");
        assert!(tree1.is_some());

        // Second parse reuses cached parser
        let tree2 = factory.parse("rust", "fn test() {}");
        assert!(tree2.is_some());

        // Both should produce valid parse trees
        assert!(!tree1.unwrap().root_node().has_error());
        assert!(!tree2.unwrap().root_node().has_error());
    }

    #[test]
    fn test_thread_local_parser_factory_clear_cache() {
        let registry = create_test_registry();
        let factory = ThreadLocalParserFactory::new(registry);

        // Parse to create cached parser
        let _ = factory.parse("rust", "fn main() {}");

        // Clear the cache
        ThreadLocalParserFactory::clear_cache();

        // Verify can still parse after cache clear (parser recreated)
        let tree = factory.parse("rust", "fn test() {}");
        assert!(tree.is_some(), "Should still parse after cache clear");
    }

    #[test]
    fn test_thread_local_parser_factory_has_language() {
        let registry = create_test_registry();
        let factory = ThreadLocalParserFactory::new(registry);

        assert!(
            factory.has_language("rust"),
            "Should have registered language"
        );
        assert!(
            !factory.has_language("unknown"),
            "Should not have unregistered language"
        );
    }

    #[test]
    fn test_parser_handles_complex_code() {
        let registry = create_test_registry();
        let factory = ThreadLocalParserFactory::new(registry);

        let code = r#"
            fn main() {
                let x = 42;
                let y = "hello";
                println!("{} {}", x, y);
            }
        "#;

        let tree = factory.parse("rust", code);
        assert!(tree.is_some(), "Should parse complex code");
        assert!(
            !tree.unwrap().root_node().has_error(),
            "Complex code should parse without errors"
        );
    }

    // Tests for sync injection processing
    // Note: Full integration tests require LanguageCoordinator setup with search paths,
    // so these are basic structural tests. Full testing is in the parent module.

    #[test]
    fn test_injection_context_struct_fields() {
        // Verify InjectionContext has the expected fields
        let registry = create_test_registry();
        let language = registry.get("rust").unwrap();

        // Create a simple query for testing
        let query = Query::new(&language, "(identifier) @variable").unwrap();

        let ctx = InjectionContext {
            resolved_lang: "rust".to_string(),
            highlight_query: Arc::new(query),
            content_text: "fn main() {}",
            host_start_byte: 100,
        };

        assert_eq!(ctx.resolved_lang, "rust");
        assert_eq!(ctx.content_text, "fn main() {}");
        assert_eq!(ctx.host_start_byte, 100);
    }

    #[test]
    fn test_process_injection_sync_with_simple_code() {
        use crate::config::WorkspaceSettings;

        // Set up coordinator with search paths
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load rust language
        let load_result = coordinator.ensure_language_loaded("rust");
        if !load_result.success {
            // Skip test if rust parser not available in CI
            return;
        }

        let Some(highlight_query) = coordinator.get_highlight_query("rust") else {
            return;
        };

        // Create factory with the coordinator's registry
        let factory = ThreadLocalParserFactory::new(coordinator.language_registry_for_testing());

        let code = "fn main() {}";
        let host_text = code;
        let host_lines: Vec<&str> = host_text.lines().collect();

        let ctx = InjectionContext {
            resolved_lang: "rust".to_string(),
            highlight_query,
            content_text: code,
            host_start_byte: 0,
        };

        let tokens = process_injection_sync(
            &ctx,
            &factory,
            &coordinator,
            None,
            host_text,
            &host_lines,
            1, // depth 1 (not host document)
            false,
        );

        // Should produce some tokens (at minimum "fn" keyword and "main" identifier)
        assert!(
            !tokens.is_empty(),
            "Should produce tokens for Rust code. Got: {:?}",
            tokens
        );
    }

    #[test]
    fn test_process_injection_sync_respects_max_depth() {
        use crate::config::WorkspaceSettings;

        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        let load_result = coordinator.ensure_language_loaded("rust");
        if !load_result.success {
            return;
        }

        let Some(highlight_query) = coordinator.get_highlight_query("rust") else {
            return;
        };

        let factory = ThreadLocalParserFactory::new(coordinator.language_registry_for_testing());

        let code = "fn main() {}";
        let host_text = code;
        let host_lines: Vec<&str> = host_text.lines().collect();

        let ctx = InjectionContext {
            resolved_lang: "rust".to_string(),
            highlight_query,
            content_text: code,
            host_start_byte: 0,
        };

        // Process at MAX_INJECTION_DEPTH should return empty
        let tokens = process_injection_sync(
            &ctx,
            &factory,
            &coordinator,
            None,
            host_text,
            &host_lines,
            MAX_INJECTION_DEPTH,
            false,
        );

        assert!(
            tokens.is_empty(),
            "Should return empty at MAX_INJECTION_DEPTH"
        );
    }

    /// Returns the search path for tree-sitter grammars.
    fn test_search_path() -> String {
        std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
    }
}
