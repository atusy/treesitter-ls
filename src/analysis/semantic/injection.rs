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

/// Validate injection node bounds before slicing text.
///
/// Returns `Some((start_byte, end_byte))` if the bounds are valid,
/// or `None` if the injection should be skipped due to invalid bounds.
///
/// Invalid bounds can occur when trees become stale relative to the text,
/// which is possible during rapid edits (race condition).
fn validate_injection_bounds(
    content_node: &tree_sitter::Node,
    text_len: usize,
) -> Option<(usize, usize)> {
    let start = content_node.start_byte();
    let end = content_node.end_byte();
    if start > end || end > text_len {
        log::debug!(
            target: "kakehashi::semantic",
            "Skipping injection with invalid bounds: start={}, end={}, text_len={}",
            start,
            end,
            text_len
        );
        None
    } else {
        Some((start, end))
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

        // Validate range: ensure bounds are valid AND start <= end
        // The start > end case can occur during concurrent edits or with certain offset calculations
        if inj_start_byte >= text.len()
            || inj_end_byte > text.len()
            || inj_start_byte > inj_end_byte
        {
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

        // Validate range: ensure bounds are valid AND start <= end
        if inj_start_byte >= text.len()
            || inj_end_byte > text.len()
            || inj_start_byte > inj_end_byte
        {
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

/// Recursively collect semantic tokens from a document and its injections.
///
/// This function processes the given text and tree, collecting tokens from both
/// the current language's highlight query and any language injections found.
/// Nested injections are processed recursively up to MAX_INJECTION_DEPTH.
///
/// When coordinator or parser_pool is None, only host document tokens are collected
/// (no injection processing).
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_injection_tokens_recursive(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
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
    let (Some(coordinator), Some(parser_pool)) = (coordinator, parser_pool) else {
        return; // No injection support available
    };

    let contexts =
        collect_injection_contexts(text, tree, filetype, coordinator, content_start_byte);

    for ctx in contexts {
        // Acquire parser from pool
        let Some(mut parser) = parser_pool.acquire(&ctx.resolved_lang) else {
            continue;
        };

        // Parse the injected content
        let Some(injected_tree) = parser.parse(ctx.content_text, None) else {
            parser_pool.release(ctx.resolved_lang.clone(), parser);
            continue;
        };

        // Recursively collect tokens from the injected content
        collect_injection_tokens_recursive(
            ctx.content_text,
            &injected_tree,
            &ctx.highlight_query,
            Some(&ctx.resolved_lang),
            capture_mappings,
            Some(coordinator),
            Some(parser_pool),
            host_text,
            host_lines,
            ctx.host_start_byte,
            depth + 1,
            supports_multiline,
            all_tokens,
        );

        parser_pool.release(ctx.resolved_lang.clone(), parser);
    }
}

/// Recursive helper for collecting tokens with local parsers.
///
/// Similar to `collect_injection_tokens_recursive` but uses a HashMap of
/// pre-acquired parsers instead of a parser pool.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_injection_tokens_recursive_with_local_parsers(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    local_parsers: &mut HashMap<String, tree_sitter::Parser>,
    host_text: &str,
    host_lines: &[&str],
    content_start_byte: usize,
    depth: usize,
    supports_multiline: bool,
    all_tokens: &mut Vec<RawToken>,
) {
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
    let Some(coordinator) = coordinator else {
        return;
    };

    let contexts =
        collect_injection_contexts(text, tree, filetype, coordinator, content_start_byte);

    for ctx in contexts {
        // Use parser from local map (if available)
        let Some(parser) = local_parsers.get_mut(&ctx.resolved_lang) else {
            continue;
        };

        let Some(injected_tree) = parser.parse(ctx.content_text, None) else {
            continue;
        };

        // Recursively collect tokens
        collect_injection_tokens_recursive_with_local_parsers(
            ctx.content_text,
            &injected_tree,
            &ctx.highlight_query,
            Some(&ctx.resolved_lang),
            capture_mappings,
            Some(coordinator),
            local_parsers,
            host_text,
            host_lines,
            ctx.host_start_byte,
            depth + 1,
            supports_multiline,
            all_tokens,
        );
    }
}
