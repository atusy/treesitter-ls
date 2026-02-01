//! Language injection processing for semantic tokens.
//!
//! This module handles the discovery and recursive processing of language
//! injections (e.g., Lua code blocks inside Markdown).

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
/// before the actual parsing step. Used by both sequential (recursive) and
/// parallel injection processing.
pub(super) struct InjectionContext<'a> {
    /// The resolved language name (e.g., "lua", "python")
    pub resolved_lang: String,
    /// The highlight query for this language
    pub highlight_query: Arc<Query>,
    /// The text content of the injection
    pub content_text: &'a str,
    /// Byte offset in the host document where this injection starts
    pub host_start_byte: usize,
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

/// Wrapper for parser pool access during sequential injection processing.
///
/// This struct provides a unified interface for acquiring and releasing parsers
/// from a `DocumentParserPool` during recursive injection token collection.
pub(super) struct ParserProvider<'a>(&'a mut crate::language::DocumentParserPool);

impl<'a> ParserProvider<'a> {
    /// Create a new parser provider wrapping a document parser pool.
    pub fn new(pool: &'a mut crate::language::DocumentParserPool) -> Self {
        Self(pool)
    }

    /// Acquire a parser for the given language.
    ///
    /// Returns `None` if no parser is available for the language.
    pub fn acquire(&mut self, lang: &str) -> Option<tree_sitter::Parser> {
        self.0.acquire(lang)
    }

    /// Release a parser back to the pool.
    pub fn release(&mut self, lang: String, parser: tree_sitter::Parser) {
        self.0.release(lang, parser)
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
