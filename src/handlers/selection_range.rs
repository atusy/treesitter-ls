use tower_lsp::lsp_types::{Position, Range, SelectionRange};
use tree_sitter::{Node, Point, Tree};

/// Convert LSP Position to tree-sitter Point
fn position_to_point(pos: &Position) -> Point {
    Point::new(pos.line as usize, pos.character as usize)
}

/// Convert tree-sitter Point to LSP Position
fn point_to_position(point: Point) -> Position {
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

/// Handle textDocument/selectionRange request
///
/// Returns selection ranges that expand intelligently by syntax boundaries.
///
/// # Arguments
/// * `tree` - The parsed syntax tree
/// * `positions` - The requested positions
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range(tree: &Tree, positions: &[Position]) -> Option<Vec<SelectionRange>> {
    let root = tree.root_node();

    let ranges = positions
        .iter()
        .map(|pos| {
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

    // Note: Testing the full handle_selection_range function would require
    // a tree-sitter language parser to be available, which we don't have
    // in the test environment. Integration tests with actual language parsers
    // would be needed for full coverage.
}
