//! Pure AST to SelectionRange conversion utilities.
//!
//! This module contains functions that build LSP SelectionRange hierarchies
//! from Tree-sitter AST nodes. These are pure functions with no injection awareness.

use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::Node;

use crate::text::position::PositionMapper;

/// Convert tree-sitter Node to LSP Range with proper UTF-16 encoding.
///
/// Tree-sitter stores positions as byte offsets, but LSP requires UTF-16 code units.
/// This function uses the provided PositionMapper to perform the correct conversion.
///
/// # Arguments
/// * `node` - The Tree-sitter node to convert
/// * `mapper` - PositionMapper for byte-to-UTF16 position conversion
///
/// # Returns
/// LSP Range with proper UTF-16 column positions
pub fn node_to_range(node: Node, mapper: &PositionMapper) -> Range {
    let start = mapper
        .byte_to_position(node.start_byte())
        .unwrap_or_else(|| Position::new(node.start_position().row as u32, 0));
    let end = mapper
        .byte_to_position(node.end_byte())
        .unwrap_or_else(|| Position::new(node.end_position().row as u32, 0));
    Range::new(start, end)
}

/// Find the next parent node that has a different (larger) byte range than the current node.
///
/// This ensures the LSP selection range hierarchy is strictly expanding.
/// The function walks up the AST tree until it finds a parent whose byte range
/// differs from the provided `current_range`.
///
/// # Arguments
/// * `node` - The starting node
/// * `current_range` - The byte range to compare against (typically the node's own range)
///
/// # Returns
/// The first ancestor with a different byte range, or None if no such ancestor exists
pub fn find_distinct_parent<'a>(
    node: Node<'a>,
    current_range: &std::ops::Range<usize>,
) -> Option<Node<'a>> {
    let mut current = node.parent();
    while let Some(parent) = current {
        let parent_range = parent.byte_range();
        // If parent has a different range, use it
        if parent_range != *current_range {
            return Some(parent);
        }
        current = parent.parent();
    }
    None
}

// Note: Unit tests for these functions require a Tree-sitter parser.
// The project loads parsers dynamically at runtime, so we test these functions
// through integration tests in selection.rs that use the existing test infrastructure.
// The tests in selection.rs already cover:
// - test_selection_range_output_uses_utf16_columns (ASCII)
// - test_selection_range_handles_multibyte_utf8 (multibyte UTF-8)
// - test_selection_range_deduplicates_same_range_nodes (find_distinct_parent)
