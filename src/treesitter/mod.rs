pub mod node_utils;

// Re-export from new modules for backward compatibility
pub use crate::injection::InjectionPositionMapper;
pub use crate::syntax::loader::ParserLoader;
pub use crate::syntax::query::{check_predicate, filter_captures};
pub use crate::syntax::tree::{
    find_children_by_type, find_node_at_position, find_nodes_in_range, get_children,
    get_named_children, get_node_text, node_contains_byte, node_contains_position, node_to_range,
    point_to_position, position_to_point, walk_tree,
};
pub use crate::text::position::{PositionMapper, SimplePositionMapper, compute_line_starts};

// Re-export node_utils functions
pub use node_utils::{
    calculate_depth, calculate_scope_depth, determine_context, find_common_ancestor,
    find_node_at_byte, get_ancestors, get_scope_chain, get_scope_ids, is_ancestor_of,
    is_scope_node,
};
