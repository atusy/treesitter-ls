//! Injection-aware SelectionRange building.
//!
//! This module contains functions that build LSP SelectionRange hierarchies
//! while respecting language injections (e.g., YAML in Markdown frontmatter,
//! Lua in Markdown code blocks).
//!
//! ## Architecture
//!
//! The injection-aware building process:
//! 1. Detect if cursor is within an injection region
//! 2. Parse the injected content with the appropriate language parser
//! 3. Build SelectionRange from the injected AST
//! 4. Chain the injected selection to the host document selection
//!
//! This module uses `DocumentContext` and `InjectionContext` to reduce
//! parameter counts and make dependencies explicit.

use tower_lsp::lsp_types::SelectionRange;
use tree_sitter::Node;

use super::injection_aware::adjust_range_to_host;
use super::range_builder::find_distinct_parent;
use crate::text::PositionMapper;

/// Build selection range for nodes in injected content.
///
/// This builds SelectionRange from injected AST nodes, adjusting positions
/// to be relative to the host document (not the injection slice).
/// Nodes with identical ranges are deduplicated (LSP spec requires strictly expanding ranges).
///
/// # Arguments
///
/// * `node` - The Tree-sitter node from the injected language parse tree
/// * `content_start_byte` - The byte offset where the injection content starts in the host document
/// * `mapper` - PositionMapper for the host document (for UTF-16 column conversion)
///
/// # Returns
///
/// SelectionRange hierarchy for the injected content, with positions adjusted to host document
pub fn build_injected_selection_range(
    node: Node,
    content_start_byte: usize,
    mapper: &PositionMapper,
) -> SelectionRange {
    let parent = find_distinct_parent(node, &node.byte_range()).map(|parent_node| {
        Box::new(build_injected_selection_range(
            parent_node,
            content_start_byte,
            mapper,
        ))
    });

    SelectionRange {
        range: adjust_range_to_host(node, content_start_byte, mapper),
        parent,
    }
}

// Note: The main injection building functions (build_selection_range_with_parsed_injection,
// build_recursive_injection_selection) will be migrated here in subsequent iterations,
// using the context structs to reduce their parameter counts.
