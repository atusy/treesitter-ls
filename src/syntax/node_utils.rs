use tree_sitter::Node;

/// Check if a node is of a scope type
///
/// # Arguments
/// * `node_type` - The type of the node
///
/// # Returns
/// true if the node type represents a scope
fn is_scope_node(node_type: &str) -> bool {
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
