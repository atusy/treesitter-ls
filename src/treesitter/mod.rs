pub mod injection_mapper;
pub mod node_utils;
pub mod parser_loader;
pub mod query_predicates;
pub mod tree_utils;

// Re-export position_mapper from layers module for backward compatibility
pub use crate::layers::mappers::position_mapper::{
    PositionMapper, SimplePositionMapper, compute_line_starts,
};

pub use injection_mapper::InjectionPositionMapper;
pub use node_utils::{
    calculate_depth, calculate_scope_depth, determine_context, find_common_ancestor,
    find_node_at_byte, get_ancestors, get_scope_chain, get_scope_ids, is_ancestor_of,
    is_scope_node,
};
pub use parser_loader::ParserLoader;
pub use query_predicates::{check_predicate, filter_captures};
pub use tree_utils::{
    find_children_by_type, find_node_at_position, find_nodes_in_range, get_children,
    get_named_children, get_node_text, node_contains_byte, node_contains_position, node_to_range,
    point_to_position, position_to_point, walk_tree,
};
