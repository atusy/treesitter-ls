// Submodules (Rust 2018+ style)
// These are located in src/analysis/selection/*.rs
pub mod hierarchy_chain;
pub mod injection_aware;
pub mod range_builder;

// Re-export from submodules
pub use hierarchy_chain::{
    chain_injected_to_host, is_range_strictly_larger, range_contains, ranges_equal,
    skip_to_distinct_host,
};
pub use injection_aware::{
    adjust_range_to_host, calculate_effective_lsp_range, is_cursor_within_effective_range,
    is_node_in_selection_chain,
};
pub use range_builder::{
    build_selection_range, find_distinct_parent, find_next_distinct_parent, node_to_range,
};

use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range_with_text};
use crate::document::DocumentHandle;
use crate::language::injection::{self, parse_offset_directive_for_pattern};
use crate::language::{DocumentParserPool, LanguageCoordinator};
use crate::text::PositionMapper;
use tower_lsp::lsp_types::{Position, Range, SelectionRange};
use tree_sitter::{Node, Query};

/// Maximum depth for nested injection recursion (prevents stack overflow)
const MAX_INJECTION_DEPTH: usize = 10;

/// Build selection range with parsed injection content (Sprint 3 + Sprint 5 nested support)
///
/// This function parses the injected content using the appropriate language parser
/// and builds a selection hierarchy that includes nodes from the injected language's AST.
/// It recursively handles nested injections up to MAX_INJECTION_DEPTH levels.
///
/// # Arguments
/// * `node` - The node at cursor position in the host document
/// * `root` - The root node of the host document tree
/// * `text` - The full document text
/// * `mapper` - PositionMapper for UTF-16 column conversion
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
/// * `coordinator` - Language coordinator for getting parsers
/// * `parser_pool` - Parser pool for acquiring/releasing parsers
/// * `cursor_byte` - The byte offset of cursor position for offset checking
///
/// # Returns
/// SelectionRange that includes nodes from both injected and host language ASTs
#[allow(clippy::too_many_arguments)]
fn build_selection_range_with_parsed_injection(
    node: Node,
    root: &Node,
    text: &str,
    mapper: &PositionMapper,
    injection_query: Option<&Query>,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
) -> SelectionRange {
    // First, detect if we're inside an injection region
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    let Some((hierarchy, content_node, pattern_index)) = injection_info else {
        // Not in injection - fall back to normal selection
        return build_selection_range(node, mapper);
    };

    // Need at least 2 entries in hierarchy: base language + injected language
    if hierarchy.len() < 2 {
        return build_selection_range(node, mapper);
    }

    // Check for offset directive on this specific pattern
    let offset_from_query =
        injection_query.and_then(|q| parse_offset_directive_for_pattern(q, pattern_index));

    // If offset exists, check if cursor is within effective range
    if let Some(offset) = offset_from_query
        && !is_cursor_within_effective_range(text, &content_node, cursor_byte, offset)
    {
        // Cursor is outside effective range - return base language selection
        return build_selection_range(node, mapper);
    }

    // Get the injected language name (last in hierarchy)
    let injected_lang = &hierarchy[hierarchy.len() - 1];

    // Helper closure to build fallback selection with or without effective range
    let build_fallback = || {
        let effective_range = offset_from_query
            .map(|offset| calculate_effective_lsp_range(text, mapper, &content_node, offset));
        build_injection_aware_selection(node, content_node, effective_range, mapper)
    };

    // Ensure the injected language is loaded before trying to acquire a parser
    // This dynamically loads the language from search paths if not already registered
    let load_result = coordinator.ensure_language_loaded(injected_lang);
    if !load_result.success {
        return build_fallback();
    }

    // Try to acquire a parser for the injected language
    let Some(mut parser) = parser_pool.acquire(injected_lang) else {
        return build_fallback();
    };

    // Extract the injected content text - use effective range if offset exists
    // Calculate byte offset in host document for proper UTF-16 conversion (Sprint 9 fix)
    let (content_text, effective_start_byte) = if let Some(offset) = offset_from_query {
        let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
        let effective = calculate_effective_range_with_text(text, byte_range, offset);
        let effective_text = &text[effective.start..effective.end];

        (effective_text, effective.start)
    } else {
        (&text[content_node.byte_range()], content_node.start_byte())
    };

    // Parse the injected content
    let Some(injected_tree) = parser.parse(content_text, None) else {
        parser_pool.release(injected_lang.to_string(), parser);
        return build_fallback();
    };

    // Calculate cursor position relative to effective injection content
    let relative_byte = cursor_byte.saturating_sub(effective_start_byte);

    // Find the node at cursor position in the injected AST
    let injected_root = injected_tree.root_node();
    let Some(injected_node) = injected_root.descendant_for_byte_range(relative_byte, relative_byte)
    else {
        parser_pool.release(injected_lang.to_string(), parser);
        return build_fallback();
    };

    // Sprint 5: Check for nested injection within the injected content
    // Get the injection query for the injected language
    let nested_injection_query = coordinator.get_injection_query(injected_lang);

    let injected_selection = if let Some(nested_inj_query) = nested_injection_query.as_ref() {
        // Check if cursor is inside a nested injection
        let nested_injection_info = injection::detect_injection_with_content(
            &injected_node,
            &injected_root,
            content_text,
            Some(nested_inj_query.as_ref()),
            injected_lang,
        );

        if let Some((nested_hierarchy, nested_content_node, nested_pattern_index)) =
            nested_injection_info
        {
            // Check offset for nested injection
            let nested_offset =
                parse_offset_directive_for_pattern(nested_inj_query.as_ref(), nested_pattern_index);

            let cursor_in_nested = match nested_offset {
                Some(offset) => is_cursor_within_effective_range(
                    content_text,
                    &nested_content_node,
                    relative_byte,
                    offset,
                ),
                None => true, // No offset means cursor is always "inside" if detected
            };

            if cursor_in_nested && nested_hierarchy.len() >= 2 {
                // We have a nested injection! Recursively build selection for it
                build_nested_injection_selection(
                    &injected_node,
                    &injected_root,
                    content_text,
                    nested_inj_query.as_ref(),
                    injected_lang,
                    coordinator,
                    parser_pool,
                    relative_byte,
                    effective_start_byte,
                    mapper,
                    1, // First level of nested injection
                )
            } else {
                // No valid nested injection - build selection from current injected node
                build_injected_selection_range(
                    injected_node,
                    &injected_root,
                    effective_start_byte,
                    mapper,
                )
            }
        } else {
            // No nested injection detected - build selection from current injected node
            build_injected_selection_range(
                injected_node,
                &injected_root,
                effective_start_byte,
                mapper,
            )
        }
    } else {
        // No injection query for the injected language - build selection from current injected node
        build_injected_selection_range(injected_node, &injected_root, effective_start_byte, mapper)
    };

    // Now chain the injected selection to the host document's selection
    // Include the content_node (e.g., minus_metadata, code_fence_content) in the host selection
    // so that the full content boundary is available in the selection hierarchy.
    // For offset cases: content_node's full range (e.g., YAML with --- markers) provides valuable context
    // For non-offset cases: content_node's parent (e.g., fenced_code_block) wraps the injection
    let host_selection = Some(build_selection_range(content_node, mapper));

    // Connect injected hierarchy to host hierarchy
    let result = chain_injected_to_host(injected_selection, host_selection);

    // Release the parser back to the pool
    parser_pool.release(injected_lang.to_string(), parser);

    result
}

/// Build selection for a nested injection (Sprint 5)
///
/// This handles the recursive case where we're inside an injection that itself
/// contains another injection.
///
/// The `parent_start_byte` is the byte offset where the parent injection content
/// starts in the host document. The `mapper` is for the host document.
#[allow(clippy::too_many_arguments)]
fn build_nested_injection_selection(
    node: &Node,
    root: &Node,
    text: &str,
    injection_query: &Query,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
    parent_start_byte: usize,
    mapper: &PositionMapper,
    depth: usize,
) -> SelectionRange {
    // Safety check
    if depth >= MAX_INJECTION_DEPTH {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    }

    // Detect the nested injection
    let injection_info = injection::detect_injection_with_content(
        node,
        root,
        text,
        Some(injection_query),
        base_language,
    );

    let Some((hierarchy, content_node, pattern_index)) = injection_info else {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    // Get nested language name from hierarchy (last element)
    if hierarchy.len() < 2 {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    }
    let nested_lang = hierarchy.last().unwrap().clone();

    // Check offset
    let offset = parse_offset_directive_for_pattern(injection_query, pattern_index);

    // Ensure nested language is loaded
    let load_result = coordinator.ensure_language_loaded(&nested_lang);
    if !load_result.success {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    }

    // Acquire parser for nested language
    let Some(mut nested_parser) = parser_pool.acquire(&nested_lang) else {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    // Extract nested content text and calculate byte offset in host document
    let (nested_text, nested_effective_start_byte) = if let Some(off) = offset {
        let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
        let effective = calculate_effective_range_with_text(text, byte_range, off);
        let effective_text = &text[effective.start..effective.end];

        // Effective byte in host = parent_start_byte + relative byte in parent content
        // Since `text` is the parent injection content, effective.start is relative to text start
        // which is at parent_start_byte in the host document
        // Actually, for nested injections, we need to track the actual host byte offset
        // The `text` here is the parent injection's content, not the host document
        // So we need to add parent_start_byte + effective.start
        // Wait, effective.start is relative to `text` which starts at parent_start_byte
        // So host_byte = parent_start_byte + effective.start - content's start relative to text
        // This is getting complex. Let's simplify:
        // effective.start is already the byte offset within the parent injection's text slice
        // So the host document byte = parent_start_byte + effective.start (if text starts at 0)
        // But wait, text is sliced, so effective.start is already correct relative to text start
        // We need: parent_start_byte (where parent injection starts) + relative position in parent
        // Hmm, content_node.start_byte() is relative to `text`, so:
        // nested_start_in_host = parent_start_byte + effective.start (relative to text)
        // Actually, effective.start is an absolute byte in text, which started at 0
        // So we need parent_start_byte + effective.start
        let nested_start_in_host = parent_start_byte + effective.start;

        (effective_text, nested_start_in_host)
    } else {
        // No offset - nested content starts at content_node position relative to parent text
        // content_node.start_byte() is relative to `text`
        let nested_start_in_host = parent_start_byte + content_node.start_byte();
        (&text[content_node.byte_range()], nested_start_in_host)
    };

    // Parse nested content
    let Some(nested_tree) = nested_parser.parse(nested_text, None) else {
        parser_pool.release(nested_lang.to_string(), nested_parser);
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    // cursor_byte is relative to the parent injection's text
    // nested_effective_start_byte is the host document byte offset
    // We need nested_relative_byte relative to nested_text start
    let nested_relative_byte = if let Some(off) = offset {
        let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
        let effective = calculate_effective_range_with_text(text, byte_range, off);
        cursor_byte.saturating_sub(effective.start)
    } else {
        cursor_byte.saturating_sub(content_node.start_byte())
    };

    let nested_root = nested_tree.root_node();

    let Some(nested_node) =
        nested_root.descendant_for_byte_range(nested_relative_byte, nested_relative_byte)
    else {
        parser_pool.release(nested_lang.to_string(), nested_parser);
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    // Check for even deeper nesting (recursive)
    let deeply_nested_injection_query = coordinator.get_injection_query(&nested_lang);

    let nested_selection = if let Some(deep_inj_query) = deeply_nested_injection_query.as_ref() {
        let deep_injection_info = injection::detect_injection_with_content(
            &nested_node,
            &nested_root,
            nested_text,
            Some(deep_inj_query.as_ref()),
            &nested_lang,
        );

        if deep_injection_info.is_some() {
            // Even deeper nesting - recurse
            build_nested_injection_selection(
                &nested_node,
                &nested_root,
                nested_text,
                deep_inj_query.as_ref(),
                &nested_lang,
                coordinator,
                parser_pool,
                nested_relative_byte,
                nested_effective_start_byte,
                mapper,
                depth + 1,
            )
        } else {
            build_injected_selection_range(
                nested_node,
                &nested_root,
                nested_effective_start_byte,
                mapper,
            )
        }
    } else {
        build_injected_selection_range(
            nested_node,
            &nested_root,
            nested_effective_start_byte,
            mapper,
        )
    };

    // Chain nested selection to parent injected content
    // Include content_node itself in the chain (like the top-level path does)
    // so that users can "select the whole nested snippet"
    let content_node_selection = Some(build_injected_selection_range(
        content_node,
        root,
        parent_start_byte,
        mapper,
    ));

    let result = chain_injected_to_host(nested_selection, content_node_selection);

    parser_pool.release(nested_lang.to_string(), nested_parser);
    result
}

/// Calculate the start position for nested injection relative to host document
///
/// This function handles signed offsets from injection directives like
/// `(#offset! @injection.content -1 0 0 0)` used in markdown frontmatter.
/// Negative offsets are handled with saturating arithmetic to prevent underflow.
///
/// Column calculation logic:
/// - If the *effective* row (content_start.row + offset_rows) is 0, we're on the
///   same row as the parent, so we add parent's column offset.
/// - If the effective row is > 0, we've moved to a later row (e.g., after skipping
///   a fence line), so the column is absolute within the content.
///
/// Note: This function was used for Point-based calculation before Sprint 9.
/// It's now kept for test coverage but production code uses byte-based offsets.
#[cfg(test)]
fn calculate_nested_start_position(
    parent_start: tree_sitter::Point,
    content_start: tree_sitter::Point,
    offset_rows: i32,
    offset_cols: i32,
) -> tree_sitter::Point {
    // The content_start is relative to the parent injection
    // We need to add the parent's start position and apply any offset
    // Use saturating arithmetic to handle negative offsets safely
    let base_row = (parent_start.row + content_start.row) as i64;
    let row = (base_row + offset_rows as i64).max(0) as usize;

    // Calculate the effective row relative to the content start
    // This determines whether we're on the "first line" of effective content
    let effective_content_row = (content_start.row as i32 + offset_rows).max(0);

    let col = if effective_content_row == 0 {
        // First row of effective content - add parent's column
        let base_col = (parent_start.column + content_start.column) as i64;
        (base_col + offset_cols as i64).max(0) as usize
    } else {
        // Later rows - column is absolute within the parent
        let base_col = content_start.column as i64;
        (base_col + offset_cols as i64).max(0) as usize
    };
    tree_sitter::Point::new(row, col)
}

/// Build selection range for nodes in injected content
///
/// This builds SelectionRange from injected AST nodes, adjusting positions
/// to be relative to the host document (not the injection slice).
/// Nodes with identical ranges are deduplicated (LSP spec requires strictly expanding ranges).
///
/// The `content_start_byte` is the byte offset where the injection content starts
/// in the host document. The `mapper` is used for proper UTF-16 column conversion.
fn build_injected_selection_range(
    node: Node,
    injected_root: &Node,
    content_start_byte: usize,
    mapper: &PositionMapper,
) -> SelectionRange {
    // Adjust the node's range to be relative to the host document
    let adjusted_range = adjust_range_to_host(node, content_start_byte, mapper);
    let node_byte_range = node.byte_range();

    // Build parent chain within injected content, skipping nodes with same range
    let parent =
        find_next_distinct_parent(node, &node_byte_range, injected_root).map(|parent_node| {
            // Stop at the root of the injected content
            if parent_node.id() == injected_root.id() {
                // The root of injected content - adjust its range too
                Box::new(SelectionRange {
                    range: adjust_range_to_host(parent_node, content_start_byte, mapper),
                    parent: None, // Will be connected to host in chain_injected_to_host
                })
            } else {
                Box::new(build_injected_selection_range(
                    parent_node,
                    injected_root,
                    content_start_byte,
                    mapper,
                ))
            }
        });

    SelectionRange {
        range: adjusted_range,
        parent,
    }
}

/// Build selection hierarchy with injection content node included
///
/// When `effective_range` is provided (from an offset directive), it replaces the
/// content node's range in the selection hierarchy. This ensures that excluded
/// regions (like `---` boundaries in YAML frontmatter) are not included.
///
/// When `effective_range` is None, uses the full content node range.
fn build_injection_aware_selection(
    node: Node,
    content_node: Node,
    effective_range: Option<Range>,
    mapper: &PositionMapper,
) -> SelectionRange {
    let content_node_range = node_to_range(content_node, mapper);
    let target_range = effective_range.unwrap_or(content_node_range);

    // Build base selection from the starting node
    let inner_selection = build_selection_range(node, mapper);

    // If we have an effective range different from content node range,
    // we need to handle range replacement
    if let Some(eff_range) = effective_range {
        // If the starting node IS the content node, replace its range with effective range
        if ranges_equal(&inner_selection.range, &content_node_range) {
            return SelectionRange {
                range: eff_range,
                parent: inner_selection
                    .parent
                    .map(|p| Box::new(replace_range_in_chain(*p, content_node_range, eff_range))),
            };
        }

        // Check if content_node is already in the parent chain
        if is_node_in_selection_chain(&inner_selection, &content_node, mapper) {
            // content_node is in the chain - replace its range with effective range
            return replace_range_in_chain(inner_selection, content_node_range, eff_range);
        }
    } else {
        // No effective range - check if content_node is already in the chain
        if is_node_in_selection_chain(&inner_selection, &content_node, mapper) {
            // content_node is already in the chain, just return as-is
            return inner_selection;
        }
    }

    // Need to splice the target range into the hierarchy
    splice_effective_range_into_hierarchy(inner_selection, target_range, &content_node, mapper)
}

/// Replace a specific range in the selection chain with the effective range
fn replace_range_in_chain(
    selection: SelectionRange,
    target_range: Range,
    effective_range: Range,
) -> SelectionRange {
    if ranges_equal(&selection.range, &target_range) {
        // Found the target - replace with effective range
        SelectionRange {
            range: effective_range,
            parent: selection
                .parent
                .map(|p| Box::new(replace_range_in_chain(*p, target_range, effective_range))),
        }
    } else {
        // Continue up the chain
        SelectionRange {
            range: selection.range,
            parent: selection
                .parent
                .map(|p| Box::new(replace_range_in_chain(*p, target_range, effective_range))),
        }
    }
}

/// Splice effective range into hierarchy at the appropriate level
fn splice_effective_range_into_hierarchy(
    selection: SelectionRange,
    effective_range: Range,
    content_node: &Node,
    mapper: &PositionMapper,
) -> SelectionRange {
    if !range_contains(&effective_range, &selection.range) {
        return selection;
    }

    // If current selection range is smaller than or equal to effective_range,
    // we need to continue up the chain
    // Current range is inside effective_range
    let new_parent = match selection.parent {
        Some(parent) => {
            let parent_selection = *parent;
            if range_contains(&parent_selection.range, &effective_range)
                && !ranges_equal(&parent_selection.range, &effective_range)
            {
                Some(Box::new(SelectionRange {
                    range: effective_range,
                    parent: Some(Box::new(splice_effective_range_into_hierarchy(
                        parent_selection,
                        effective_range,
                        content_node,
                        mapper,
                    ))),
                }))
            } else {
                // Keep going up
                Some(Box::new(splice_effective_range_into_hierarchy(
                    parent_selection,
                    effective_range,
                    content_node,
                    mapper,
                )))
            }
        }
        None => {
            // No parent, but we're inside effective_range - add effective_range as parent
            Some(Box::new(SelectionRange {
                range: effective_range,
                parent: content_node
                    .parent()
                    .map(|p| Box::new(build_selection_range(p, mapper))),
            }))
        }
    };

    SelectionRange {
        range: selection.range,
        parent: new_parent,
    }
}

/// Handle textDocument/selectionRange request with full injection parsing support
///
/// This is the most complete version that parses injected content and builds
/// selection hierarchies from the injected language's AST.
///
/// # Arguments
/// * `document` - The document
/// * `positions` - The requested positions
/// * `injection_query` - Optional injection query for detecting language injections
/// * `base_language` - Optional base language of the document
/// * `coordinator` - Language coordinator for parser configuration
/// * `parser_pool` - Parser pool for acquiring/releasing parsers
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range(
    document: &DocumentHandle,
    positions: &[Position],
    injection_query: Option<&Query>,
    base_language: Option<&str>,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
) -> Option<Vec<SelectionRange>> {
    let text = document.text();
    // Reuse the document's cached position mapper instead of creating a new one per position.
    // This avoids O(file_size × positions) work from rebuilding LineIndex for each cursor.
    let mapper = document.position_mapper();

    // LSP Spec 3.17 requires 1:1 correspondence between positions and results.
    // We use map (not filter_map) to maintain alignment, returning an empty
    // fallback range for positions that cannot be resolved.
    let ranges: Vec<SelectionRange> = positions
        .iter()
        .map(|pos| {
            // Try to build a real selection range
            let real_range = (|| {
                // Get the tree
                let tree = document.tree()?;
                let root = tree.root_node();

                // Calculate the byte offset for the cursor position using cached mapper.
                let cursor_byte_offset = mapper.position_to_byte(*pos)?;

                // Find the smallest node containing this position
                let node =
                    root.descendant_for_byte_range(cursor_byte_offset, cursor_byte_offset)?;

                // Build the selection range hierarchy with full injection parsing
                if let Some(lang) = base_language {
                    Some(build_selection_range_with_parsed_injection(
                        node,
                        &root,
                        text,
                        &mapper,
                        injection_query,
                        lang,
                        coordinator,
                        parser_pool,
                        cursor_byte_offset,
                    ))
                } else {
                    Some(build_selection_range(node, &mapper))
                }
            })();

            // Return real range or fallback empty range at the requested position
            real_range.unwrap_or_else(|| {
                let fallback_range = Range::new(*pos, *pos);
                SelectionRange {
                    range: fallback_range,
                    parent: None,
                }
            })
        })
        .collect();

    Some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    #[test]
    fn test_selection_range_respects_offset_directive() {
        // Test that the offset directive is correctly parsed and applied
        // to determine whether cursor is within effective injection range.
        //
        // Since both "inside" and "outside" effective range produce similar
        // selection hierarchies when starting from the injection content node itself,
        // this test verifies the offset parsing and effective range calculation
        // by directly testing is_cursor_within_effective_range.
        use crate::text::PositionMapper;
        use tree_sitter::{Parser, Query};

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Rust code with regex injection - the string content is "_^\d+$"
        let text = r#"fn main() {
    let pattern = Regex::new(r"_^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query with offset directive (0, 2, 0, 0)
        // This means effective range starts 2 bytes after capture start
        // string_content is at bytes 43-49, so effective range is 45-49
        let injection_query_str = r#"
(call_expression
  function: (scoped_identifier
    path: (identifier) @_regex
    (#eq? @_regex "Regex")
    name: (identifier) @_new
    (#eq? @_new "new"))
  arguments: (arguments
    (raw_string_literal
      (string_content) @injection.content))
  (#set! injection.language "regex")
  (#offset! @injection.content 0 2 0 0))
        "#;

        let injection_query = Query::new(&language, injection_query_str).expect("valid query");
        let mapper = PositionMapper::new(text);

        // Get the string_content node
        let underscore_pos = Position::new(1, 31);
        let underscore_point = Point::new(
            underscore_pos.line as usize,
            underscore_pos.character as usize,
        );
        let underscore_byte = mapper.position_to_byte(underscore_pos).unwrap();

        let string_content_node = root
            .descendant_for_point_range(underscore_point, underscore_point)
            .expect("should find node");

        assert_eq!(string_content_node.kind(), "string_content");
        assert_eq!(string_content_node.start_byte(), 43);
        assert_eq!(string_content_node.end_byte(), 49);

        // Verify offset directive is correctly parsed for pattern 0
        let offset = parse_offset_directive_for_pattern(&injection_query, 0);
        assert!(offset.is_some(), "Offset directive should be found");
        let offset = offset.unwrap();
        assert_eq!(offset.start_row, 0);
        assert_eq!(offset.start_column, 2);
        assert_eq!(offset.end_row, 0);
        assert_eq!(offset.end_column, 0);

        // Test effective range checking
        // Cursor at underscore (byte 43) - should be OUTSIDE effective range (45-49)
        assert!(
            !is_cursor_within_effective_range(text, &string_content_node, underscore_byte, offset),
            "Cursor at byte 43 (underscore) should be OUTSIDE effective range 45-49"
        );

        // Cursor at caret position (byte 45) - should be INSIDE effective range
        let caret_pos = Position::new(1, 33);
        let caret_byte = mapper.position_to_byte(caret_pos).unwrap();
        assert_eq!(caret_byte, 45, "Caret should be at byte 45");

        assert!(
            is_cursor_within_effective_range(text, &string_content_node, caret_byte, offset),
            "Cursor at byte 45 (caret ^) should be INSIDE effective range 45-49"
        );

        // Cursor at position 44 (between underscore and caret) - should be OUTSIDE
        assert!(
            !is_cursor_within_effective_range(text, &string_content_node, 44, offset),
            "Cursor at byte 44 should be OUTSIDE effective range 45-49"
        );
    }

    /// Test that selection range handles nested injections recursively and includes content node boundary.
    ///
    /// This is the core test for Sprint 5 nested injection support: when cursor is inside
    /// a nested injection region, the selection should expand through ALL injection levels'
    /// AST nodes, and include the content node boundary (e.g., double_quote_scalar in YAML)
    /// so users can "select the whole nested snippet".
    ///
    /// Test scenario:
    /// - Host: Rust code with a raw string literal containing YAML
    /// - First injection: YAML content
    /// - Nested injection: Rust embedded in a YAML double-quoted value
    /// - Cursor: inside the nested Rust code
    /// - Expected: Selection hierarchy includes nodes from Rust, YAML, and host Rust
    #[test]
    fn test_nested_injection_includes_content_node_boundary() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        // Setup: Rust → YAML → Rust nested injection (same as test_selection_range_handles_nested_injection)
        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());
        coordinator.register_language_for_test("rust", tree_sitter_rust::LANGUAGE.into());

        // YAML injection query that matches double_quote_scalar as Rust
        let yaml_injection_query_str = r#"
((double_quote_scalar) @injection.content
 (#set! injection.language "rust"))
        "#;
        let yaml_lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
        let yaml_injection_query =
            Query::new(&yaml_lang, yaml_injection_query_str).expect("valid yaml injection query");
        coordinator.register_injection_query_for_test("yaml", yaml_injection_query);

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Host document: Rust → YAML → Rust
        // The YAML has a double_quote_scalar: "fn nested() {}"
        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        let text = r##"fn main() {
    let yaml = r#"title: "fn nested() {}""#;
}"##;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Rust → YAML injection query
        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query =
            Query::new(&rust_language, injection_query_str).expect("valid injection query");

        // Position inside the nested Rust code
        let cursor_pos = Position::new(1, 33);
        let mapper = crate::text::PositionMapper::new(text);
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();
        let point = Point::new(cursor_pos.line as usize, cursor_pos.character as usize);
        let node = root
            .descendant_for_point_range(point, point)
            .expect("find node");

        let selection = build_selection_range_with_parsed_injection(
            node,
            &root,
            text,
            &mapper,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            cursor_byte,
        );

        // Collect all ranges in the selection hierarchy
        let mut ranges: Vec<Range> = Vec::new();
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            ranges.push(sel.range);
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // Verify sufficient nesting depth (from merged test_selection_range_handles_nested_injection)
        // With nested injection (Rust → YAML → Rust), we expect at least 7 levels
        assert!(
            ranges.len() >= 7,
            "Expected at least 7 selection levels with nested injection, got {}",
            ranges.len()
        );

        // The nested content node is double_quote_scalar in YAML, which contains "fn nested() {}"
        // Its range in the host document should be around line 1, columns 25-41
        // (the exact position depends on YAML parsing, but it should be present)
        //
        // We need to find a range that corresponds to the nested injection's content node,
        // which is larger than the innermost Rust nodes but smaller than the YAML block_mapping_pair.
        //
        // Since double_quote_scalar includes the quotes, its range in the original document
        // would be roughly columns 25-41 on line 1 (where the quoted string is).
        //
        // For this test, we verify that there exists a range in the selection hierarchy
        // that matches the nested content node's expected position.
        //
        // The double_quote_scalar node (nested content) spans from the opening quote to closing quote.
        // In YAML context, it's at the beginning of the YAML content.
        // After adjustment to host coordinates, it should be around:
        // - start: line 1, col 25 (start of "fn nested() {}")
        // - end: line 1, col 41 (end of "fn nested() {}")

        // The string_content in YAML starts at col 18: `title: "fn nested() {}"`
        // The double_quote_scalar starts at col 25: `"fn nested() {}"`
        // Look for a range that starts around column 25-26 and ends around 40-41
        let nested_content_found = ranges.iter().any(|r| {
            r.start.line == 1
                && r.start.character >= 25
                && r.start.character <= 26
                && r.end.line == 1
                && r.end.character >= 40
                && r.end.character <= 42
        });

        assert!(
            nested_content_found,
            "Selection hierarchy should include nested injection content node boundary.\n\
             Ranges in hierarchy: {:?}\n\
             Expected a range around (1:25-26 to 1:40-42) for the nested content node.",
            ranges
        );
    }

    /// Test that selection range parses injected content and builds hierarchy from injected AST.
    ///
    /// This is the core test for Sprint 3: when cursor is inside an injection region,
    /// the selection should expand through the INJECTED language's AST nodes, not just
    /// the host document's content node.
    ///
    /// Test scenario:
    /// - Host: Rust code with a raw string literal
    /// - Content: YAML text inside the string
    /// - Cursor: inside "awesome" in the YAML content
    /// - Expected: Selection hierarchy includes YAML AST nodes (e.g., `double_quote_scalar`)
    #[test]
    fn test_selection_range_parses_injected_content() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        // Setup: Create a coordinator with YAML language registered
        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Host document: Rust code with a string that we'll treat as YAML injection
        // Using Rust as host because we have tree-sitter-rust available
        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        // The string content `title: "awesome"\narray: ["xxxx"]` will be our "injected" YAML
        // Using r##"..."## to allow r#"..."# inside
        let text = r##"fn main() {
    let yaml = r#"title: "awesome"
array: ["xxxx"]"#;
}"##;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query that captures the raw_string_literal content as YAML
        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query =
            Query::new(&rust_language, injection_query_str).expect("valid injection query");

        // Position inside the YAML string - at "awesome"
        // Line 0: fn main() {
        // Line 1:     let yaml = r#"title: "awesome"
        //                          ^------- column 18 is start of string_content
        //                                 ^ column 25 is 't' in 'title', col 32 is 'a' in 'awesome'
        let cursor_pos = Position::new(1, 32); // line 1, col 32 = 'a' in "awesome"
        let point = Point::new(cursor_pos.line as usize, cursor_pos.character as usize);

        let node = root
            .descendant_for_point_range(point, point)
            .expect("should find node");

        // Calculate cursor byte offset
        let mapper = crate::text::PositionMapper::new(text);
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();

        // Call the new function that parses injected content
        // This function should:
        // 1. Detect the injection (string_content with YAML)
        // 2. Parse the injected content as YAML
        // 3. Find the node at cursor in the YAML AST
        // 4. Build selection from YAML node through to host document
        let selection = build_selection_range_with_parsed_injection(
            node,
            &root,
            text,
            &mapper,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            cursor_byte,
        );

        // Verify: The selection hierarchy should include YAML-specific nodes
        // We can't directly check node kinds from SelectionRange, but we can verify
        // that we have MORE selection levels than the host document alone would provide.
        // The extra levels come from the injected YAML AST (double_quote_scalar,
        // flow_node, block_mapping_pair, block_mapping, block_node, document, stream)

        // Count selection levels
        let mut level_count = 0;
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            level_count += 1;
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // Without injection parsing: string_content → raw_string_literal → let_declaration → ... → source_file
        // With injection parsing: double_quote_scalar → flow_node → block_mapping_pair → ... → string_content → ...
        // We expect MORE levels with injection parsing (deduplication removes same-range nodes)
        assert!(
            level_count >= 7,
            "Expected at least 7 selection levels with injected YAML AST, got {}. \
             This indicates the injected content was not parsed.",
            level_count
        );
    }

    /// Test that calculate_nested_start_position handles various offset scenarios correctly.
    ///
    /// This function calculates the start position for nested injections relative to the
    /// host document. It handles:
    /// 1. Negative offsets (like markdown's `(#offset! @injection.content -1 0 0 0)`)
    /// 2. Column alignment when row offsets skip lines (e.g., fenced code blocks)
    ///
    /// Note: This function was used for Point-based calculation before Sprint 9.
    /// Production code now uses byte-based offsets, but this test validates the logic.
    #[test]
    fn test_calculate_nested_start_position() {
        // === Negative offset handling ===

        // Case 1: Negative row offset larger than combined row - should saturate to 0
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(2, 0),
            tree_sitter::Point::new(1, 0),
            -5, // offset causes underflow
            0,
        );
        assert_eq!(result.row, 0, "Row should saturate to 0 on underflow");

        // Case 2: Negative column offset - should saturate to 0
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(0, 10),
            tree_sitter::Point::new(0, 5),
            0,
            -20, // offset causes underflow
        );
        assert_eq!(result.column, 0, "Column should saturate to 0 on underflow");

        // === Column alignment with row offsets ===

        // Case 3: No row offset (effective row 0) - add parent's column
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(5, 4),
            tree_sitter::Point::new(0, 0),
            0,
            0,
        );
        assert_eq!(result.row, 5);
        assert_eq!(result.column, 4, "Add parent column when effective row is 0");

        // Case 4: Row offset moves to later row - column is absolute
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(5, 4),
            tree_sitter::Point::new(0, 0),
            1, // skip fence line
            0,
        );
        assert_eq!(result.row, 6);
        assert_eq!(
            result.column, 0,
            "Column is absolute when offset moves to later row"
        );

        // Case 5: Positive offset to later rows - column is absolute
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(10, 5),
            tree_sitter::Point::new(0, 3),
            2,
            1,
        );
        assert_eq!(result.row, 12);
        assert_eq!(result.column, 4, "Column: 3 + 1 = 4 (absolute)");

        // Case 6: No row offset, content at row 0 - add parent column
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(10, 5),
            tree_sitter::Point::new(0, 3),
            0,
            1,
        );
        assert_eq!(result.row, 10);
        assert_eq!(result.column, 9, "Column: 5 + 3 + 1 = 9 (adds parent)");

        // Case 7: Negative offset brings effective row back to 0 - add parent column
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(5, 4),
            tree_sitter::Point::new(1, 2),
            -1, // effective row becomes 0
            0,
        );
        assert_eq!(result.row, 5);
        assert_eq!(result.column, 6, "Column: 4 + 2 = 6 (adds parent)");
    }

    /// Test that build_selection_range deduplicates nodes with identical ranges.
    ///
    /// In Tree-sitter ASTs, it's common for a node and its parent to have the same
    /// byte range (e.g., `identifier` wrapped by `expression` with same range).
    /// LSP spec requires strictly expanding ranges, so we must skip duplicates.
    #[test]
    fn test_selection_range_deduplicates_same_range_nodes() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // In Rust, a simple expression like "foo" inside a function creates a chain
        // where some nodes may have identical ranges (e.g., identifier wrapped by expression)
        // Let's use a simple variable reference in a return statement
        let text = "fn f() { x }";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Find the identifier node for "x"
        let cursor_byte = 9; // position of "x"
        let node = root
            .descendant_for_byte_range(cursor_byte, cursor_byte)
            .expect("should find node");

        assert_eq!(node.kind(), "identifier", "Should find identifier node");

        // Build selection range
        let mapper = PositionMapper::new(text);
        let selection = build_selection_range(node, &mapper);

        // Collect all ranges in the hierarchy
        let mut ranges: Vec<(u32, u32, u32, u32)> = Vec::new();
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            ranges.push((
                sel.range.start.line,
                sel.range.start.character,
                sel.range.end.line,
                sel.range.end.character,
            ));
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // Check for duplicates - no two consecutive ranges should be identical
        for i in 1..ranges.len() {
            assert_ne!(
                ranges[i - 1],
                ranges[i],
                "Found duplicate ranges at positions {} and {}: {:?}. \
                 Selection range should deduplicate nodes with identical ranges.",
                i - 1,
                i,
                ranges[i]
            );
        }

        // Also verify we have reasonable number of levels (not too many due to duplicates)
        // The exact count depends on grammar, but with deduplication it should be reasonable
        // If there are duplicates, we'd have extra levels
        assert!(
            ranges.len() <= 8,
            "Expected at most 8 levels (with deduplication), got {}. Ranges: {:?}",
            ranges.len(),
            ranges
        );
    }

    /// Test that selection ranges correctly handle multi-byte UTF-8 characters.
    ///
    /// LSP positions use UTF-16 code units for columns, but tree-sitter uses byte offsets.
    /// The `node_to_range` function must convert tree-sitter byte columns to UTF-16.
    ///
    /// Example: "let あ = 1; let x = 2;"
    /// - "あ" is 3 bytes in UTF-8 but 1 UTF-16 code unit
    /// - 'x' is at byte 17 (0-indexed) in UTF-8
    /// - 'x' is at column 15 (0-indexed) in UTF-16
    /// - If `node_to_range` outputs byte columns, we'd get character=17 (WRONG)
    /// - Correct output should have character=15
    #[test]
    fn test_selection_range_output_uses_utf16_columns() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // "あ" is 3 bytes in UTF-8 but 1 UTF-16 code unit
        // "let あ = 1; let x = 2;"
        //  0123 4 567890123456789...  (UTF-16 columns)
        //  0123 456 789...            (UTF-8 bytes, where あ takes 3 bytes at positions 4,5,6)
        //
        // UTF-16 breakdown:
        // "let " = cols 0-3 (4 chars)
        // "あ"   = col 4 (1 char)
        // " = 1; let " = cols 5-14 (10 chars)
        // "x"    = col 15 (1 char)
        //
        // UTF-8 byte breakdown:
        // "let " = bytes 0-3 (4 bytes)
        // "あ"   = bytes 4-6 (3 bytes: E3 81 82)
        // " = 1; let " = bytes 7-16 (10 bytes)
        // "x"    = byte 17 (1 byte)
        let text = "let あ = 1; let x = 2;";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Note: We need a PositionMapper because build_selection_range now requires it
        // This is the Sprint 7 change: pass the cached mapper instead of text
        let mapper = PositionMapper::new(text);

        // Find 'x' using byte offset
        let byte_offset = 17; // 'x' is at byte 17
        let node = root
            .descendant_for_byte_range(byte_offset, byte_offset)
            .expect("should find node");

        assert_eq!(node.kind(), "identifier");
        assert_eq!(&text[node.byte_range()], "x");

        // Build selection range - this is what the LSP returns to the client
        let selection = build_selection_range(node, &mapper);

        // CRITICAL ASSERTION: The output range MUST use UTF-16 columns!
        // 'x' starts at UTF-16 column 15, not byte 17
        assert_eq!(
            selection.range.start.character, 15,
            "Selection start should be UTF-16 column 15, not byte offset 17. \
             node_to_range must convert tree-sitter byte columns to UTF-16."
        );
        assert_eq!(
            selection.range.end.character, 16,
            "Selection end should be UTF-16 column 16 (one past 'x')"
        );
    }

    /// Test that injected content selection ranges use UTF-16 columns correctly.
    ///
    /// This is the core test for review.md Issue 1 (second review): `adjust_range_to_host`
    /// creates LSP Positions directly from byte columns, but should use UTF-16.
    ///
    /// Scenario: Rust code with a raw string containing Japanese text treated as YAML.
    /// The YAML content has multi-byte characters, and the selection ranges should
    /// use UTF-16 columns when reported to the LSP client.
    ///
    /// Example: `let yaml = r#"あ: 0"#;`
    /// The "あ" in the YAML content is at a certain byte position, but the LSP
    /// should report its UTF-16 column position.
    #[test]
    fn test_injected_selection_range_uses_utf16_columns() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());

        // YAML injection query that matches double_quote_scalar
        let yaml_injection_query_str = r#"
((double_quote_scalar) @injection.content
 (#set! injection.language "yaml"))
        "#;
        let yaml_lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
        let yaml_injection_query =
            Query::new(&yaml_lang, yaml_injection_query_str).expect("valid yaml injection query");
        coordinator.register_injection_query_for_test("yaml", yaml_injection_query);

        let mut parser_pool = coordinator.create_document_parser_pool();

        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        // Rust code with Japanese text in YAML injection
        // "あ" is 3 bytes in UTF-8 but 1 UTF-16 code unit
        // The YAML content "あ: 0" has the "0" at:
        // - Relative to YAML content start: byte 5 (あ=3 bytes + ":"+space = 2)
        // - In UTF-16 within YAML: column 3 (あ=1 + ":"+space = 2)
        //
        // In the host document:
        // let yaml = r#"あ: 0"#;
        //  0         1         2
        //  0123456789012345678901234
        //               ^-- raw string starts at byte 14
        //
        // After r#" the YAML content "あ: 0" starts at byte 17
        // The "0" is at byte 17 + 5 = 22 in the host document
        // But in UTF-16, column calculation should account for あ being 1 char
        let text = "let yaml = r#\"あ: 0\"#;";
        let tree = parser.parse(text, None).expect("parse");
        let root = tree.root_node();

        // Rust to YAML injection query
        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query = Query::new(&rust_language, injection_query_str).expect("valid query");

        // Position the cursor at "0" in the YAML content
        // In the host document, we need to find the byte position of "0"
        // let yaml = r#" is 14 bytes (all ASCII), then string_content starts
        // Then "あ: 0" where:
        // - "あ" = 3 bytes
        // - ": " = 2 bytes
        // - "0" at relative byte 5 within content
        //
        // Actually, let's verify by finding the string_content node
        let mapper = crate::text::PositionMapper::new(text);

        // Find the string_content node first
        let mut cursor = root.walk();
        let mut content_node = None;
        loop {
            let node = cursor.node();
            if node.kind() == "string_content" {
                content_node = Some(node);
                break;
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    break;
                }
            }
            if cursor.node().id() == root.id() {
                break;
            }
        }
        let content_node = content_node.expect("Should find string_content node");

        // The string_content contains "あ: 0"
        let content_text = &text[content_node.byte_range()];
        assert_eq!(content_text, "あ: 0", "Content should be the YAML text");

        // The "0" is at relative byte 5 within the content (あ=3 + ": "=2)
        // In host document: content_node.start_byte() + 5
        let zero_byte_in_host = content_node.start_byte() + 5;

        // Now use the handler function
        let selection = build_selection_range_with_parsed_injection(
            content_node,
            &root,
            text,
            &mapper,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            zero_byte_in_host,
        );

        // The innermost selection should be for the "0" in the YAML
        // CRITICAL: The column should be in UTF-16, not bytes!
        //
        // Host document UTF-16 analysis:
        // let yaml = r#"あ: 0"#;
        //  0         1         2
        //  0123456789012345678901
        //               ^-- r#" starts at col 11
        //                 ^-- content starts at col 14 (after r#")
        //                 あ at col 14
        //                  : at col 15
        //                    at col 16 (space)
        //                  0 at col 17
        //
        // Wait, let me recalculate:
        // "let yaml = r#" is 14 characters
        // Then the string content:
        // "あ" = 1 UTF-16 code unit at col 14
        // ":" = 1 at col 15
        // " " = 1 at col 16
        // "0" = 1 at col 17
        //
        // So "0" should be at UTF-16 column 17 in the host document.
        // If we incorrectly use bytes:
        // "let yaml = r#" = 14 bytes
        // "あ" = 3 bytes at byte 14-16
        // ": " = 2 bytes at byte 17-18
        // "0" at byte 19
        //
        // So incorrect byte-based would give column 19, correct UTF-16 gives 17.

        // Find the innermost range (for "0")
        let _innermost_range = selection.range;

        // The selection might be for a larger YAML node. Let's check what we got.
        // Walk up to verify we have injected content
        let mut found_small_range = false;
        let mut current = &selection;
        loop {
            // Look for a range that could be the "0" node
            // It should be small (1 character) and on line 0
            if current.range.start.line == 0
                && current.range.end.line == 0
                && current.range.end.character - current.range.start.character <= 5
            {
                // This is likely in the injected content
                // The column should NOT be the byte offset
                if current.range.start.character >= 17 && current.range.start.character <= 20 {
                    // Check that it's using UTF-16, not bytes
                    // If using bytes incorrectly, we'd see character=19 for "0"
                    // If correct UTF-16, we'd see character=17 for "0"
                    // (or similar values for enclosing YAML nodes)
                    found_small_range = true;

                    // The key assertion: if adjust_range_to_host uses bytes directly,
                    // we'd get wrong column values. After the fix, columns are UTF-16.
                    // For "0", the UTF-16 column is 17, byte offset is 19.
                    assert!(
                        current.range.start.character < 19,
                        "Expected UTF-16 column (17 or 18), got byte-based column {}. \
                         adjust_range_to_host must convert byte columns to UTF-16.",
                        current.range.start.character
                    );
                    break;
                }
            }
            if let Some(parent) = &current.parent {
                current = parent.as_ref();
            } else {
                break;
            }
        }

        assert!(
            found_small_range,
            "Should find a small range in the injected content. \
             Selection ranges: {:?}",
            collect_ranges(&selection)
        );
    }

    /// Test that invalid positions get fallback empty ranges (LSP-compliant alignment)
    ///
    /// LSP Spec 3.17 states: "To allow for results where some positions have
    /// selection ranges and others do not, result[i].range is allowed to be
    /// the empty range at positions[i]."
    ///
    /// This ensures multi-cursor editors receive correctly aligned results.
    #[test]
    fn test_selection_range_maintains_position_alignment() {
        use crate::document::store::DocumentStore;
        use crate::language::LanguageCoordinator;
        use tower_lsp::lsp_types::Url;
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = "let x = 1;\nlet y = 2;";
        let tree = parser.parse(text, None).expect("parse rust");

        // Create a document with the parsed tree
        let url = Url::parse("file:///test.rs").unwrap();
        let store = DocumentStore::new();
        store.insert(
            url.clone(),
            text.to_string(),
            Some("rust".to_string()),
            Some(tree),
        );

        // Request selection ranges for multiple positions:
        // - Position 0: valid (line 0, col 4 = 'x')
        // - Position 1: INVALID (line 100 doesn't exist!)
        // - Position 2: valid (line 1, col 4 = 'y')
        let positions = vec![
            Position::new(0, 4),   // valid: 'x'
            Position::new(100, 0), // invalid: line 100 doesn't exist
            Position::new(1, 4),   // valid: 'y'
        ];

        let coordinator = LanguageCoordinator::new();
        let mut parser_pool = coordinator.create_document_parser_pool();
        let document = store.get(&url).expect("document should exist");
        let result = handle_selection_range(
            &document,
            &positions,
            None,
            None,
            &coordinator,
            &mut parser_pool,
        );

        // LSP requires 1:1 correspondence between positions and results
        assert!(
            result.is_some(),
            "Request should not fail entirely due to one invalid position"
        );

        let ranges = result.unwrap();

        // CRITICAL: Result length MUST equal input positions length for alignment
        assert_eq!(
            ranges.len(),
            positions.len(),
            "Result length must equal input positions length for LSP alignment"
        );

        // Position 0 (valid): should have a real selection range for 'x'
        assert!(
            ranges[0].range.start.line == 0,
            "First result should be for line 0"
        );

        // Position 1 (invalid): should have an empty fallback range
        // LSP spec allows empty range at the requested position
        assert_eq!(
            ranges[1].range.start, ranges[1].range.end,
            "Invalid position should get an empty (zero-length) range"
        );
        assert!(
            ranges[1].parent.is_none(),
            "Fallback range should have no parent"
        );

        // Position 2 (valid): should have a real selection range for 'y'
        assert!(
            ranges[2].range.start.line == 1,
            "Third result should be for line 1"
        );
    }

    /// Helper to collect all ranges in a selection hierarchy for debugging
    fn collect_ranges(selection: &SelectionRange) -> Vec<Range> {
        let mut ranges = vec![selection.range];
        let mut current = &selection.parent;
        while let Some(parent) = current {
            ranges.push(parent.range);
            current = &parent.parent;
        }
        ranges
    }

    /// Test that empty documents return valid fallback ranges
    ///
    /// Empty documents are an edge case where tree-sitter produces an empty tree
    /// (or a tree with only an ERROR node). The selection range handler should
    /// return a valid empty range at the requested position.
    #[test]
    fn test_selection_range_handles_empty_document() {
        use crate::document::store::DocumentStore;
        use crate::language::LanguageCoordinator;
        use tower_lsp::lsp_types::Url;
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Empty document
        let text = "";
        let tree = parser.parse(text, None).expect("parse empty document");

        // Create a document with the parsed tree
        let url = Url::parse("file:///empty.rs").unwrap();
        let store = DocumentStore::new();
        store.insert(
            url.clone(),
            text.to_string(),
            Some("rust".to_string()),
            Some(tree),
        );

        // Request selection range at position (0, 0) - the only valid position
        let positions = vec![Position::new(0, 0)];

        let coordinator = LanguageCoordinator::new();
        let mut parser_pool = coordinator.create_document_parser_pool();
        let document = store.get(&url).expect("document should exist");
        let result = handle_selection_range(
            &document,
            &positions,
            None,
            None,
            &coordinator,
            &mut parser_pool,
        );

        // Should return a result (not fail entirely)
        assert!(
            result.is_some(),
            "Empty document should still return a selection range result"
        );

        let ranges = result.unwrap();

        // Should return exactly one result for the one position
        assert_eq!(ranges.len(), 1, "Should return one range for one position");

        // The result should be an empty range at (0, 0) since there are no AST nodes
        let range = &ranges[0];
        assert_eq!(
            range.range.start,
            Position::new(0, 0),
            "Empty document range should start at (0, 0)"
        );
        assert_eq!(
            range.range.end,
            Position::new(0, 0),
            "Empty document range should end at (0, 0)"
        );
    }
}
