use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point};

/// Convert LSP Position to tree-sitter Point
pub fn position_to_point(pos: &Position) -> Point {
    Point::new(pos.line as usize, pos.character as usize)
}

/// Convert tree-sitter Point to LSP Position
pub fn point_to_position(point: Point) -> Position {
    Position::new(point.row as u32, point.column as u32)
}

/// Convert tree-sitter Node to LSP Range
pub fn node_to_range(node: Node) -> Range {
    Range::new(
        point_to_position(node.start_position()),
        point_to_position(node.end_position()),
    )
}
