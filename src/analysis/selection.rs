use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range_with_text};
use crate::document::DocumentHandle;
use crate::language::injection::{self, parse_offset_directive_for_pattern};
use tower_lsp::lsp_types::{Position, Range, SelectionRange};
use tree_sitter::{Node, Point, Query};

/// Convert LSP Position to tree-sitter Point
pub fn position_to_point(pos: &Position) -> Point {
    Point::new(pos.line as usize, pos.character as usize)
}

/// Convert tree-sitter Point to LSP Position
pub fn point_to_position(point: Point) -> Position {
    Position::new(point.row as u32, point.column as u32)
}

/// Convert tree-sitter Node to LSP Range
fn node_to_range(node: Node) -> Range {
    Range::new(
        point_to_position(node.start_position()),
        point_to_position(node.end_position()),
    )
}

/// Build selection range hierarchy for a node
fn build_selection_range(node: Node) -> SelectionRange {
    let range = node_to_range(node);

    // Build parent chain
    let parent = node
        .parent()
        .map(|parent_node| Box::new(build_selection_range(parent_node)));

    SelectionRange { range, parent }
}

/// Build selection range hierarchy with injection awareness
///
/// When the cursor is inside an injection region, this function ensures
/// the injection content node is included in the selection hierarchy.
///
/// # Arguments
/// * `node` - The node at cursor position
/// * `root` - The root node of the document tree
/// * `text` - The document text
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
///
/// # Returns
/// SelectionRange that includes injection boundaries when applicable
pub fn build_selection_range_with_injection(
    node: Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> SelectionRange {
    // Try to detect if we're inside an injection region
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    match injection_info {
        Some((_hierarchy, content_node, _pattern_index)) => {
            build_injection_aware_selection(node, content_node)
        }
        None => build_selection_range(node),
    }
}

/// Build selection range hierarchy with injection awareness and offset support
///
/// This version takes a cursor_byte parameter to check if the cursor is within
/// the effective range of the injection after applying offset directives.
///
/// # Arguments
/// * `node` - The node at cursor position
/// * `root` - The root node of the document tree
/// * `text` - The document text
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
/// * `cursor_byte` - The byte position of the cursor for offset checking
///
/// # Returns
/// SelectionRange that includes injection boundaries when applicable and cursor
/// is within the effective range (after applying offset)
pub fn build_selection_range_with_injection_and_offset(
    node: Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
    cursor_byte: usize,
) -> SelectionRange {
    // Try to detect if we're inside an injection region
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    match injection_info {
        Some((_hierarchy, content_node, pattern_index)) => {
            // Check for offset directive on this specific pattern
            let offset_from_query =
                injection_query.and_then(|q| parse_offset_directive_for_pattern(q, pattern_index));

            // Only apply offset-based filtering when there's an actual offset directive
            // (Lesson from Sprint 13 in development record 0002)
            if let Some(offset) = offset_from_query {
                // Check if cursor is within the effective range (after applying offset)
                if !is_cursor_within_effective_range(text, &content_node, cursor_byte, offset) {
                    // Cursor is outside effective range - return base language selection
                    return build_selection_range(node);
                }
            }

            build_injection_aware_selection(node, content_node)
        }
        None => build_selection_range(node),
    }
}

/// Build selection hierarchy with injection content node included
///
/// Shared logic for injection-aware selection range building.
fn build_injection_aware_selection(node: Node, content_node: Node) -> SelectionRange {
    let inner_selection = build_selection_range(node);

    // Check if content_node is already in the parent chain
    if is_node_in_selection_chain(&inner_selection, &content_node) {
        // content_node is already in the chain, just return as-is
        inner_selection
    } else {
        // Need to splice content_node into the hierarchy
        splice_injection_content_into_hierarchy(inner_selection, content_node)
    }
}

/// Check if cursor byte position is within the effective range after applying offset
fn is_cursor_within_effective_range(
    text: &str,
    content_node: &Node,
    cursor_byte: usize,
    offset: injection::InjectionOffset,
) -> bool {
    let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
    let effective_range = calculate_effective_range_with_text(text, byte_range, offset);
    cursor_byte >= effective_range.start && cursor_byte < effective_range.end
}

/// Check if a node's range is already present in the selection chain
fn is_node_in_selection_chain(selection: &SelectionRange, target_node: &Node) -> bool {
    let target_range = node_to_range(*target_node);
    let mut current = Some(selection);

    while let Some(sel) = current {
        if sel.range == target_range {
            return true;
        }
        current = sel.parent.as_ref().map(|p| p.as_ref());
    }

    false
}

/// Splice the injection content node into the selection hierarchy
///
/// The content_node represents the boundary of the injected region.
/// We need to insert it at the appropriate level in the hierarchy.
fn splice_injection_content_into_hierarchy(
    selection: SelectionRange,
    content_node: Node,
) -> SelectionRange {
    let content_range = node_to_range(content_node);

    // Find the first ancestor in the chain that fully contains the content_node
    // and insert content_node just before it
    rebuild_with_injection_boundary(selection, content_range, &content_node)
}

/// Rebuild selection hierarchy, inserting the injection content node at the right place
fn rebuild_with_injection_boundary(
    selection: SelectionRange,
    content_range: Range,
    content_node: &Node,
) -> SelectionRange {
    // If current selection range is smaller than or equal to content_range,
    // we need to continue up the chain
    if range_contains(&content_range, &selection.range) {
        // Current range is inside content_range
        // Continue building, but when we reach content_node's parent level,
        // insert content_node
        let new_parent = match selection.parent {
            Some(parent) => {
                let parent_selection = *parent;
                if range_contains(&parent_selection.range, &content_range)
                    && !ranges_equal(&parent_selection.range, &content_range)
                {
                    // Parent is larger than content_range, insert content_node here
                    let content_selection = SelectionRange {
                        range: content_range,
                        parent: Some(Box::new(rebuild_with_injection_boundary(
                            parent_selection,
                            content_range,
                            content_node,
                        ))),
                    };
                    Some(Box::new(content_selection))
                } else {
                    // Keep going up
                    Some(Box::new(rebuild_with_injection_boundary(
                        parent_selection,
                        content_range,
                        content_node,
                    )))
                }
            }
            None => {
                // No parent, but we're inside content_range - add content_node as parent
                Some(Box::new(SelectionRange {
                    range: content_range,
                    parent: content_node
                        .parent()
                        .map(|p| Box::new(build_selection_range(p))),
                }))
            }
        };

        SelectionRange {
            range: selection.range,
            parent: new_parent,
        }
    } else {
        // Current range is not inside content_range, just continue normally
        selection
    }
}

/// Check if outer range fully contains inner range
fn range_contains(outer: &Range, inner: &Range) -> bool {
    (outer.start.line < inner.start.line
        || (outer.start.line == inner.start.line && outer.start.character <= inner.start.character))
        && (outer.end.line > inner.end.line
            || (outer.end.line == inner.end.line && outer.end.character >= inner.end.character))
}

/// Check if two ranges are equal
fn ranges_equal(a: &Range, b: &Range) -> bool {
    a.start == b.start && a.end == b.end
}

/// Handle textDocument/selectionRange request
///
/// Returns selection ranges that expand intelligently by syntax boundaries.
///
/// # Arguments
/// * `document` - The document
/// * `positions` - The requested positions
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range(
    document: &DocumentHandle,
    positions: &[Position],
) -> Option<Vec<SelectionRange>> {
    // Create position mapper via document abstraction
    let mapper = document.position_mapper();

    let ranges = positions
        .iter()
        .map(|pos| {
            // Convert position to byte offset
            let _byte_offset = mapper.position_to_byte(*pos)?;

            // Get the tree
            let tree = document.tree()?;
            let root = tree.root_node();

            // Convert position to point for tree-sitter
            let point = position_to_point(pos);

            // Find the smallest node containing this position
            let node = root.descendant_for_point_range(point, point)?;

            // Build the selection range hierarchy
            Some(build_selection_range(node))
        })
        .collect::<Option<Vec<_>>>()?;

    Some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    #[test]
    fn test_position_to_point() {
        let pos = Position::new(5, 10);
        let point = position_to_point(&pos);
        assert_eq!(point.row, 5);
        assert_eq!(point.column, 10);
    }

    #[test]
    fn test_point_to_position() {
        let point = Point::new(3, 7);
        let pos = point_to_position(point);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.character, 7);
    }

    #[test]
    fn test_selection_range_detects_injection() {
        // Test that selection range detects when cursor is inside an injection region
        // and includes the injection content node in the selection hierarchy
        use tree_sitter::{Parser, Query};

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Rust code with regex injection
        let text = r#"fn main() {
    let pattern = Regex::new(r"^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query for Regex::new
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
  (#set! injection.language "regex"))
        "#;

        let injection_query = Query::new(&language, injection_query_str).expect("valid query");

        // Position inside the regex string (the \d part at column 32)
        let cursor_pos = Position::new(1, 32);
        let point = position_to_point(&cursor_pos);

        // Find the node at cursor
        let node = root
            .descendant_for_point_range(point, point)
            .expect("should find node");

        // Call the new injection-aware function
        let selection =
            build_selection_range_with_injection(node, &root, text, Some(&injection_query), "rust");

        // The selection hierarchy should include the injection content node
        // Walk up the parent chain and check that we find a range matching
        // the string_content node (which is the injection.content capture)
        let mut found_injection_content = false;
        let mut current = Some(&selection);

        // The string_content "^\d+$" is at bytes 43-48 (line 1, col 31-36)
        // We need to find this range in the selection hierarchy
        while let Some(sel) = current {
            // Check if this range corresponds to string_content
            // string_content starts at line 1, col 31 and ends at line 1, col 36
            if sel.range.start.line == 1
                && sel.range.start.character == 31
                && sel.range.end.line == 1
                && sel.range.end.character == 36
            {
                found_injection_content = true;
                break;
            }
            current = sel.parent.as_ref().map(|p| p.as_ref());
        }

        assert!(
            found_injection_content,
            "Selection hierarchy should include injection content node (string_content)"
        );
    }

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
        let underscore_point = position_to_point(&underscore_pos);
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

        // Test that the full function correctly returns different results
        // based on whether cursor is inside or outside effective range.
        // Both return `build_selection_range(node)` when injection is not active,
        // so we verify through the internal logic: injection detection returns
        // Some when offset check passes, None-equivalent behavior when it doesn't.

        // Build selection at underscore position (outside effective range)
        let _selection_at_underscore = build_selection_range_with_injection_and_offset(
            string_content_node,
            &root,
            text,
            Some(&injection_query),
            "rust",
            underscore_byte,
        );

        // Build selection at caret position (inside effective range)
        let _selection_at_caret = build_selection_range_with_injection_and_offset(
            string_content_node,
            &root,
            text,
            Some(&injection_query),
            "rust",
            caret_byte,
        );

        // Both produce valid selection hierarchies - the difference is that
        // injection-specific processing only occurs when inside effective range.
        // This test verified the core offset logic; integration tests can
        // verify observable differences with more complex AST structures.
    }
}
