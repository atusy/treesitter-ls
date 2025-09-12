use crate::state::document::Document;
use crate::treesitter::tree_utils::{node_to_range, position_to_point};
use tower_lsp::lsp_types::{Position, SelectionRange};
use tree_sitter::{Node, Tree};

/// Build selection range hierarchy for a node
fn build_selection_range(node: Node) -> SelectionRange {
    let range = node_to_range(node);

    // Build parent chain
    let parent = node
        .parent()
        .map(|parent_node| Box::new(build_selection_range(parent_node)));

    SelectionRange { range, parent }
}

/// Handle textDocument/selectionRange request (legacy API)
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

/// Handle selection range request with layer awareness
///
/// Returns selection ranges that expand intelligently by syntax boundaries,
/// using the appropriate language layer for each position.
///
/// # Arguments
/// * `document` - The document containing layers
/// * `positions` - The requested positions
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range_layered(
    document: &Document,
    positions: &[Position],
) -> Option<Vec<SelectionRange>> {
    let mapper = document.position_mapper();
    
    let ranges = positions
        .iter()
        .map(|pos| {
            // Convert position to byte offset
            let byte_offset = mapper.position_to_byte(*pos)?;
            
            // Find the appropriate layer
            let layer = document.get_layer_at_position(byte_offset)?;
            let tree = &layer.tree;
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
    use crate::treesitter::tree_utils::point_to_position;
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

    // Note: Testing the full handle_selection_range function would require
    // a tree-sitter language parser to be available, which we don't have
    // in the test environment. Integration tests with actual language parsers
    // would be needed for full coverage.
}
