//! Language injection processing for semantic tokens.
//!
//! This module handles the discovery and recursive processing of language
//! injections (e.g., Lua code blocks inside Markdown).

use std::sync::Arc;

use tree_sitter::{Query, Tree};

use super::token_collector::{RawToken, collect_host_tokens};

/// Maximum recursion depth for nested injections to prevent stack overflow
pub(super) const MAX_INJECTION_DEPTH: usize = 10;

/// Data for processing a single injection (parser-agnostic).
///
/// This struct captures all the information needed to process an injection
/// before the actual parsing step. Used by parallel injection processing.
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

/// Collect semantic tokens from the host document only.
///
/// This function processes the given text and tree using the highlight query,
/// collecting tokens only from the host document (no injection processing).
/// Injection tokens are collected separately via parallel processing.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_host_document_tokens(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
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

    // Collect tokens from this document's highlight query
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
}
