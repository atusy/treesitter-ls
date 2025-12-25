pub mod definition;
pub mod offset_calculator;
pub mod refactor;
pub mod result_id;
pub mod selection;
pub mod semantic;

// Re-export main types and functions
pub use definition::{DefinitionResolver, handle_goto_definition};
pub use refactor::handle_code_actions;
pub use result_id::next_result_id;
pub use selection::handle_selection_range;
pub use semantic::{
    LEGEND_MODIFIERS, LEGEND_TYPES, handle_semantic_tokens_full, handle_semantic_tokens_full_delta,
    handle_semantic_tokens_range,
};
