use tree_sitter::Node;

/// Calculate the depth of a node in the tree
///
/// # Arguments
/// * `node` - The node to calculate depth for
///
/// # Returns
/// The depth of the node (root = 0)
pub fn calculate_depth(node: Node) -> usize {
    let mut depth = 0;
    let mut current = node.parent();
    
    while let Some(parent) = current {
        depth += 1;
        current = parent.parent();
    }
    
    depth
}

/// Get all ancestor nodes of a given node
///
/// # Arguments
/// * `node` - The node to get ancestors for
///
/// # Returns
/// Vector of ancestor nodes from immediate parent to root
pub fn get_ancestors(node: Node) -> Vec<Node> {
    let mut ancestors = Vec::new();
    let mut current = node.parent();
    
    while let Some(parent) = current {
        ancestors.push(parent);
        current = parent.parent();
    }
    
    ancestors
}

/// Check if a node is of a scope type
///
/// # Arguments
/// * `node_type` - The type of the node
///
/// # Returns
/// true if the node type represents a scope
pub fn is_scope_node(node_type: &str) -> bool {
    matches!(
        node_type,
        "block"
            | "function_item"
            | "function_declaration"
            | "function_definition"
            | "method_definition"
            | "if_statement"
            | "if_expression"
            | "while_statement"
            | "while_expression"
            | "for_statement"
            | "for_expression"
            | "loop_expression"
            | "match_expression"
            | "match_statement"
            | "try_statement"
            | "catch_clause"
            | "class_definition"
            | "class_declaration"
            | "struct_item"
            | "enum_item"
            | "impl_item"
            | "module"
            | "namespace"
            | "scope"
            | "chunk"
            | "do_statement"
            | "closure_expression"
            | "lambda"
            | "arrow_function"
    )
}

/// Get all scope nodes containing a given node
///
/// # Arguments
/// * `node` - The node to find scopes for
///
/// # Returns
/// Vector of scope nodes from innermost to outermost
pub fn get_scope_chain(node: Node) -> Vec<Node> {
    let mut scopes = Vec::new();
    let mut current = node.parent();
    
    while let Some(parent) = current {
        if is_scope_node(parent.kind()) {
            scopes.push(parent);
        }
        current = parent.parent();
    }
    
    scopes
}

/// Get scope IDs for a node (used for scope distance calculations)
///
/// # Arguments
/// * `node` - The node to get scope IDs for
///
/// # Returns
/// Vector of scope node IDs from innermost to outermost
pub fn get_scope_ids(node: Node) -> Vec<usize> {
    let mut scope_ids = Vec::new();
    let mut current = node.parent();
    
    while let Some(n) = current {
        if is_scope_node(n.kind()) {
            scope_ids.push(n.id());
        }
        current = n.parent();
    }
    
    scope_ids
}

/// Calculate the scope depth of a node
///
/// # Arguments
/// * `node` - The node to calculate scope depth for
///
/// # Returns
/// Number of scope nodes containing this node
pub fn calculate_scope_depth(node: Node) -> usize {
    let mut depth = 0;
    let mut current = node.parent();
    
    while let Some(parent) = current {
        if is_scope_node(parent.kind()) {
            depth += 1;
        }
        current = parent.parent();
    }
    
    depth
}

/// Find the nearest common ancestor of two nodes
///
/// # Arguments
/// * `node1` - First node
/// * `node2` - Second node
///
/// # Returns
/// The nearest common ancestor node, if one exists
pub fn find_common_ancestor<'a>(node1: Node<'a>, node2: Node<'a>) -> Option<Node<'a>> {
    let ancestors1 = get_ancestors(node1);
    let ancestors2_ids: Vec<usize> = get_ancestors(node2).iter().map(|n| n.id()).collect();
    
    for ancestor in ancestors1 {
        if ancestors2_ids.contains(&ancestor.id()) {
            return Some(ancestor);
        }
    }
    
    // Check if nodes are in the same tree
    if node1.id() == node2.id() {
        return Some(node1);
    }
    
    None
}

/// Find a node at a specific byte offset
///
/// # Arguments
/// * `root` - The root node to search from
/// * `byte_offset` - The byte offset to find a node at
///
/// # Returns
/// The smallest node containing the byte offset
pub fn find_node_at_byte(root: Node, byte_offset: usize) -> Option<Node> {
    root.descendant_for_byte_range(byte_offset, byte_offset)
}

/// Check if one node is an ancestor of another
///
/// # Arguments
/// * `potential_ancestor` - The potential ancestor node
/// * `node` - The node to check
///
/// # Returns
/// true if potential_ancestor is an ancestor of node
pub fn is_ancestor_of(potential_ancestor: Node, node: Node) -> bool {
    let mut current = node.parent();
    
    while let Some(parent) = current {
        if parent.id() == potential_ancestor.id() {
            return true;
        }
        current = parent.parent();
    }
    
    false
}

/// Determine the context type based on parent nodes
///
/// # Arguments
/// * `node` - The node to determine context for
///
/// # Returns
/// A string describing the context type
pub fn determine_context(node: Node) -> &'static str {
    let mut current = node.parent();
    
    while let Some(parent) = current {
        match parent.kind() {
            "call_expression" | "function_call" => return "function_call",
            "type_annotation" | "type_identifier" | "type_parameter" => return "type_annotation",
            "field_expression" | "member_expression" | "field_access" => return "field_access",
            _ => {}
        }
        current = parent.parent();
    }
    
    "variable_reference"
}