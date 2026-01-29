//! Language injection processing for semantic tokens.
//!
//! This module handles the discovery and recursive processing of language
//! injections (e.g., Lua code blocks inside Markdown).

use std::collections::HashMap;
use std::sync::Arc;

use tree_sitter::{Query, Tree};

use super::token_collector::{RawToken, collect_host_tokens};

/// Maximum recursion depth for nested injections to prevent stack overflow
pub(super) const MAX_INJECTION_DEPTH: usize = 10;

/// Check if byte range is valid for slicing text.
///
/// Returns `true` if start <= end and both are within text bounds.
/// Returns `false` for invalid ranges that would cause panics or be meaningless.
///
/// Invalid bounds can occur when:
/// - Trees become stale relative to the text during rapid edits (race condition)
/// - Offset calculations result in inverted ranges
/// - Content nodes extend beyond current text length
#[inline]
fn is_valid_byte_range(start: usize, end: usize, text_len: usize) -> bool {
    start <= end && end <= text_len
}

/// Validate injection node bounds before slicing text.
///
/// Returns `Some((start_byte, end_byte))` if the bounds are valid,
/// or `None` if the injection should be skipped due to invalid bounds.
fn validate_injection_bounds(
    content_node: &tree_sitter::Node,
    text_len: usize,
) -> Option<(usize, usize)> {
    let start = content_node.start_byte();
    let end = content_node.end_byte();
    if is_valid_byte_range(start, end, text_len) {
        Some((start, end))
    } else {
        log::debug!(
            target: "kakehashi::semantic",
            "Skipping injection with invalid bounds: start={}, end={}, text_len={}",
            start,
            end,
            text_len
        );
        None
    }
}

/// Data for processing a single injection (parser-agnostic).
///
/// This struct captures all the information needed to process an injection
/// before the actual parsing step.
struct InjectionContext<'a> {
    resolved_lang: String,
    highlight_query: Arc<Query>,
    content_text: &'a str,
    host_start_byte: usize,
}

/// Collect all injection contexts from a document tree.
///
/// This function processes the injection query and returns a list of
/// `InjectionContext` structs, each containing the information needed
/// to parse and process one injection. This is parser-agnostic; actual
/// parsing happens after this function returns.
fn collect_injection_contexts<'a>(
    text: &'a str,
    tree: &Tree,
    filetype: Option<&str>,
    coordinator: &crate::language::LanguageCoordinator,
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
        let Some((inj_start, inj_end)) =
            validate_injection_bounds(&injection.content_node, text.len())
        else {
            continue;
        };

        // Extract injection content for first-line detection (shebang, mode line)
        let injection_content = &text[inj_start..inj_end];

        // Resolve injection language with unified detection
        let Some((resolved_lang, _)) =
            coordinator.resolve_injection_language(&injection.language, injection_content)
        else {
            continue;
        };

        // Get highlight query for resolved language
        let Some(inj_highlight_query) = coordinator.get_highlight_query(&resolved_lang) else {
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
        if !is_valid_byte_range(inj_start_byte, inj_end_byte, text.len()) {
            continue;
        }

        contexts.push(InjectionContext {
            resolved_lang,
            highlight_query: inj_highlight_query,
            content_text: &text[inj_start_byte..inj_end_byte],
            host_start_byte: content_start_byte + inj_start_byte,
        });
    }

    contexts
}

/// Collect all unique injection language identifiers from a document tree recursively.
///
/// This function discovers all injection regions in the document (including nested
/// injections) and returns the unique set of language identifiers needed to process
/// them. This is essential for the narrow lock scope pattern: acquire all parsers
/// upfront, release the lock, then process without holding the mutex.
///
/// # Arguments
/// * `tree` - The parsed syntax tree of the host document
/// * `text` - The source text of the host document
/// * `host_language` - The language identifier of the host document (e.g., "markdown")
/// * `coordinator` - Language coordinator for injection query lookup
/// * `parser_pool` - Parser pool for parsing nested injection content
///
/// # Returns
/// A vector of unique resolved language identifiers for all injections found,
/// including those nested inside other injections (up to MAX_INJECTION_DEPTH).
pub(crate) fn collect_injection_languages(
    tree: &Tree,
    text: &str,
    host_language: &str,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
) -> Vec<String> {
    use std::collections::HashSet;

    let mut languages = HashSet::new();
    collect_injection_languages_recursive(
        tree,
        text,
        host_language,
        coordinator,
        parser_pool,
        &mut languages,
        0,
    );
    languages.into_iter().collect()
}

/// Recursive helper for collecting injection languages at all depths.
fn collect_injection_languages_recursive(
    tree: &Tree,
    text: &str,
    language: &str,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
    languages: &mut std::collections::HashSet<String>,
    depth: usize,
) {
    use crate::language::{collect_all_injections, injection::parse_offset_directive_for_pattern};

    if depth >= MAX_INJECTION_DEPTH {
        return;
    }

    // Get injection query for this language
    let Some(injection_query) = coordinator.get_injection_query(language) else {
        return;
    };

    // Find all injection regions
    let Some(injections) = collect_all_injections(&tree.root_node(), text, Some(&injection_query))
    else {
        return;
    };

    for injection in injections {
        let Some((inj_start, inj_end)) =
            validate_injection_bounds(&injection.content_node, text.len())
        else {
            continue;
        };

        // Extract injection content for first-line detection (shebang, mode line)
        let injection_content = &text[inj_start..inj_end];

        // Resolve the injection language
        let Some((resolved_lang, _)) =
            coordinator.resolve_injection_language(&injection.language, injection_content)
        else {
            continue;
        };

        // Add to set (whether already present or not)
        languages.insert(resolved_lang.clone());

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
        if !is_valid_byte_range(inj_start_byte, inj_end_byte, text.len()) {
            continue;
        }
        let inj_content_text = &text[inj_start_byte..inj_end_byte];

        // Parse the injected content to discover nested injections
        let Some(mut parser) = parser_pool.acquire(&resolved_lang) else {
            continue;
        };
        let Some(injected_tree) = parser.parse(inj_content_text, None) else {
            parser_pool.release(resolved_lang.clone(), parser);
            continue;
        };

        // Recursively collect languages from nested injections
        collect_injection_languages_recursive(
            &injected_tree,
            inj_content_text,
            &resolved_lang,
            coordinator,
            parser_pool,
            languages,
            depth + 1,
        );

        parser_pool.release(resolved_lang, parser);
    }
}

/// Abstraction over different parser acquisition strategies.
///
/// This enum unifies the two patterns for acquiring parsers during injection processing:
/// - `Pool`: Acquires parsers from a shared `DocumentParserPool` (dynamic borrowing)
/// - `Local`: Uses pre-acquired parsers from a local `HashMap` (upfront acquisition)
///
/// Both variants support the same `acquire`/`release` interface, enabling a single
/// unified recursive token collection function.
pub(super) enum ParserProvider<'a> {
    /// Parser pool variant - acquires/releases parsers dynamically per injection
    Pool(&'a mut crate::language::DocumentParserPool),
    /// Local HashMap variant - uses pre-acquired parsers
    Local(&'a mut HashMap<String, tree_sitter::Parser>),
}

impl ParserProvider<'_> {
    /// Acquire a parser for the given language.
    ///
    /// - For `Pool`: delegates to `DocumentParserPool::acquire`
    /// - For `Local`: removes the parser from the HashMap (takes ownership)
    ///
    /// Returns `None` if no parser is available for the language.
    pub fn acquire(&mut self, lang: &str) -> Option<tree_sitter::Parser> {
        match self {
            Self::Pool(pool) => pool.acquire(lang),
            Self::Local(map) => map.remove(lang),
        }
    }

    /// Release a parser back to its source.
    ///
    /// - For `Pool`: delegates to `DocumentParserPool::release`
    /// - For `Local`: inserts the parser back into the HashMap
    pub fn release(&mut self, lang: String, parser: tree_sitter::Parser) {
        match self {
            Self::Pool(pool) => pool.release(lang, parser),
            Self::Local(map) => {
                map.insert(lang, parser);
            }
        }
    }
}

/// Recursively collect semantic tokens from a document and its injections.
///
/// This function processes the given text and tree, collecting tokens from both
/// the current language's highlight query and any language injections found.
/// Nested injections are processed recursively up to MAX_INJECTION_DEPTH.
///
/// When coordinator or parser_provider is None, only host document tokens are collected
/// (no injection processing).
///
/// # Parser Provider
///
/// The `parser_provider` parameter abstracts over two parser acquisition strategies:
/// - `ParserProvider::Pool`: Dynamic acquisition from a shared parser pool
/// - `ParserProvider::Local`: Pre-acquired parsers in a local HashMap
///
/// Both variants use the same acquire/release semantics, enabling unified handling.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_injection_tokens_recursive(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_provider: Option<&mut ParserProvider<'_>>,
    host_text: &str,
    host_lines: &[&str],
    content_start_byte: usize,
    depth: usize,
    supports_multiline: bool,
    all_tokens: &mut Vec<RawToken>,
) {
    // Safety check for recursion depth
    if depth >= MAX_INJECTION_DEPTH {
        return;
    }

    // 1. Collect tokens from this document's highlight query
    collect_host_tokens(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        host_text,
        host_lines,
        content_start_byte,
        depth,
        supports_multiline,
        all_tokens,
    );

    // 2. Find and process injections
    let (Some(coordinator), Some(parser_provider)) = (coordinator, parser_provider) else {
        return; // No injection support available
    };

    let contexts =
        collect_injection_contexts(text, tree, filetype, coordinator, content_start_byte);

    for ctx in contexts {
        // Acquire parser from provider (pool or local map)
        let Some(mut parser) = parser_provider.acquire(&ctx.resolved_lang) else {
            continue;
        };

        // Parse the injected content
        let Some(injected_tree) = parser.parse(ctx.content_text, None) else {
            parser_provider.release(ctx.resolved_lang.clone(), parser);
            continue;
        };

        // Release parser BEFORE recursive call to avoid holding mutable borrow
        // during recursion. The parser has done its job (produced injected_tree).
        parser_provider.release(ctx.resolved_lang.clone(), parser);

        // Recursively collect tokens from the injected content
        collect_injection_tokens_recursive(
            ctx.content_text,
            &injected_tree,
            &ctx.highlight_query,
            Some(&ctx.resolved_lang),
            capture_mappings,
            Some(coordinator),
            Some(parser_provider),
            host_text,
            host_lines,
            ctx.host_start_byte,
            depth + 1,
            supports_multiline,
            all_tokens,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_provider_enum_variants_exist() {
        // This test verifies that ParserProvider enum has both variants
        // and can be pattern-matched. The actual acquire/release logic
        // is tested in task 3.2.

        // We can't easily construct real DocumentParserPool or Parser in unit tests,
        // so we just verify the enum type exists and has the expected structure
        // by checking that the type is well-formed via a type annotation.
        fn _assert_pool_variant(_: ParserProvider<'_>) {}
        fn _assert_local_variant(_: ParserProvider<'_>) {}

        // Verify pattern matching compiles (won't run, just type-check)
        fn _match_provider(p: ParserProvider<'_>) {
            match p {
                ParserProvider::Pool(_) => {}
                ParserProvider::Local(_) => {}
            }
        }
    }

    #[test]
    fn test_parser_provider_local_acquire_release() {
        // Test that Local variant correctly takes and returns parsers
        let mut parsers: HashMap<String, tree_sitter::Parser> = HashMap::new();
        parsers.insert("lua".to_string(), tree_sitter::Parser::new());

        let mut provider = ParserProvider::Local(&mut parsers);

        // Acquire should remove from map
        let parser = provider.acquire("lua");
        assert!(
            parser.is_some(),
            "acquire should return Some for existing lang"
        );

        // Map should now be empty
        if let ParserProvider::Local(map) = &provider {
            assert!(
                map.get("lua").is_none(),
                "parser should be removed from map"
            );
        }

        // Acquire again should return None (parser was taken)
        let parser2 = provider.acquire("lua");
        assert!(parser2.is_none(), "second acquire should return None");

        // Release should put it back
        provider.release("lua".to_string(), parser.unwrap());

        // Now acquire should work again
        let parser3 = provider.acquire("lua");
        assert!(
            parser3.is_some(),
            "acquire after release should return Some"
        );
    }

    #[test]
    fn test_parser_provider_local_acquire_nonexistent() {
        let mut parsers: HashMap<String, tree_sitter::Parser> = HashMap::new();
        let mut provider = ParserProvider::Local(&mut parsers);

        let parser = provider.acquire("nonexistent");
        assert!(
            parser.is_none(),
            "acquire for nonexistent lang should return None"
        );
    }
}
