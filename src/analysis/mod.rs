pub mod definition;
pub mod refactor;
pub mod selection;
pub mod semantic;

// Re-export main types and functions
pub use crate::domain::{LEGEND_MODIFIERS, LEGEND_TYPES};
pub use definition::{DefinitionResolver, handle_goto_definition};
pub use refactor::handle_code_actions;
pub use selection::handle_selection_range;
pub use semantic::{
    handle_semantic_tokens_full, handle_semantic_tokens_full_delta, handle_semantic_tokens_range,
};
