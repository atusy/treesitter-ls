//! Injection-aware SelectionRange building utilities.
//!
//! This module contains functions that handle language injection scenarios,
//! where one language is embedded within another (e.g., YAML in Markdown frontmatter).
//! These functions manage coordinate translation between injection and host documents,
//! and build proper selection hierarchies that respect injection boundaries.

use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::Node;

use crate::text::position::PositionMapper;

/// Adjust a node's range from injection-relative to host-document-relative coordinates.
///
/// When working with injected language content, Tree-sitter parses the injection
/// as a standalone document starting at byte 0. This function translates those
/// byte positions back to the host document's coordinate space.
///
/// Uses byte offsets and PositionMapper to ensure correct UTF-16 column conversion.
///
/// # Arguments
/// * `node` - The Tree-sitter node from the injected language parse tree
/// * `content_start_byte` - The byte offset where the injection content starts in the host document
/// * `mapper` - PositionMapper for the host document (byte-to-UTF16 conversion)
///
/// # Returns
/// LSP Range in host document coordinates with proper UTF-16 positions
pub fn adjust_range_to_host(node: Node, content_start_byte: usize, mapper: &PositionMapper) -> Range {
    // Calculate the actual byte offsets in the host document
    // Node's start_byte() and end_byte() are relative to the injection content
    let host_start_byte = content_start_byte + node.start_byte();
    let host_end_byte = content_start_byte + node.end_byte();

    // Convert byte offsets to UTF-16 positions using the mapper
    let adjusted_start = mapper
        .byte_to_position(host_start_byte)
        .unwrap_or_else(|| Position::new(0, 0));
    let adjusted_end = mapper
        .byte_to_position(host_end_byte)
        .unwrap_or_else(|| Position::new(0, 0));

    Range::new(adjusted_start, adjusted_end)
}

// Note: Unit tests for injection_aware functions require Tree-sitter parsers
// and injection scenarios which are tested through integration tests in selection.rs.
// The tests in selection.rs cover:
// - test_selection_range_handles_nested_injection
// - test_injected_selection_range_uses_utf16_columns
// - test_nested_injection_includes_content_node_boundary
