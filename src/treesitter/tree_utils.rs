use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point, Tree};

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

/// Find the smallest node at a given LSP position
///
/// # Arguments
/// * `tree` - The tree to search
/// * `position` - The LSP position
///
/// # Returns
/// The smallest node containing the position
pub fn find_node_at_position(tree: &Tree, position: Position) -> Option<Node<'_>> {
    let point = position_to_point(&position);
    tree.root_node().descendant_for_point_range(point, point)
}

/// Find all nodes in a given range
///
/// # Arguments
/// * `tree` - The tree to search
/// * `range` - The LSP range
///
/// # Returns
/// Vector of nodes that overlap with the range
pub fn find_nodes_in_range<'a>(tree: &'a Tree, range: &Range) -> Vec<Node<'a>> {
    let start_point = position_to_point(&range.start);
    let end_point = position_to_point(&range.end);
    
    let mut nodes = Vec::new();
    let mut cursor = tree.walk();
    
    fn collect_nodes_in_range<'a>(
        cursor: &mut tree_sitter::TreeCursor<'a>,
        start: Point,
        end: Point,
        nodes: &mut Vec<Node<'a>>,
    ) {
        loop {
            let node = cursor.node();
            let node_start = node.start_position();
            let node_end = node.end_position();
            
            // Check if node overlaps with range
            if node_end.row >= start.row && node_start.row <= end.row {
                // More precise check for nodes on boundary lines
                let overlaps = !(node_start.row == end.row && node_start.column > end.column
                    || node_end.row == start.row && node_end.column < start.column);
                
                if overlaps {
                    nodes.push(node);
                    
                    // Recurse into children
                    if cursor.goto_first_child() {
                        collect_nodes_in_range(cursor, start, end, nodes);
                        cursor.goto_parent();
                    }
                }
            }
            
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    
    collect_nodes_in_range(&mut cursor, start_point, end_point, &mut nodes);
    nodes
}

/// Get the text content of a node
///
/// # Arguments
/// * `node` - The node to get text for
/// * `source` - The source text
///
/// # Returns
/// The text content of the node
pub fn get_node_text<'a>(node: &Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

/// Check if a node contains a byte offset
///
/// # Arguments
/// * `node` - The node to check
/// * `byte_offset` - The byte offset
///
/// # Returns
/// true if the node contains the byte offset
pub fn node_contains_byte(node: &Node, byte_offset: usize) -> bool {
    let range = node.byte_range();
    byte_offset >= range.start && byte_offset <= range.end
}

/// Check if a node contains an LSP position
///
/// # Arguments
/// * `node` - The node to check
/// * `position` - The LSP position
///
/// # Returns
/// true if the node contains the position
pub fn node_contains_position(node: &Node, position: &Position) -> bool {
    let point = position_to_point(position);
    let start = node.start_position();
    let end = node.end_position();
    
    point.row >= start.row && point.row <= end.row &&
        (point.row != start.row || point.column >= start.column) &&
        (point.row != end.row || point.column <= end.column)
}

/// Get all children of a node
///
/// # Arguments
/// * `node` - The parent node
///
/// # Returns
/// Vector of all child nodes
pub fn get_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut children = Vec::new();
    let count = node.child_count();
    
    for i in 0..count {
        if let Some(child) = node.child(i) {
            children.push(child);
        }
    }
    
    children
}

/// Get all named children of a node
///
/// # Arguments
/// * `node` - The parent node
///
/// # Returns
/// Vector of all named child nodes
pub fn get_named_children<'a>(node: Node<'a>) -> Vec<Node<'a>> {
    let mut children = Vec::new();
    let count = node.named_child_count();
    
    for i in 0..count {
        if let Some(child) = node.named_child(i) {
            children.push(child);
        }
    }
    
    children
}

/// Find child nodes of a specific type
///
/// # Arguments
/// * `node` - The parent node
/// * `node_type` - The type of nodes to find
///
/// # Returns
/// Vector of child nodes matching the type
pub fn find_children_by_type<'a>(node: Node<'a>, node_type: &str) -> Vec<Node<'a>> {
    get_children(node)
        .into_iter()
        .filter(|child| child.kind() == node_type)
        .collect()
}

/// Walk the tree depth-first and apply a function to each node
///
/// # Arguments
/// * `node` - The root node to walk from
/// * `visitor` - Function to apply to each node
pub fn walk_tree<F>(node: Node, visitor: &mut F)
where
    F: FnMut(Node),
{
    visitor(node);
    
    for child in get_children(node) {
        walk_tree(child, visitor);
    }
}