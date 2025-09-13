pub mod code_action;
pub mod definition;
pub mod selection;
pub mod semantic_tokens;

// For backward compatibility, re-export selection as selection_range
pub use selection as selection_range;

pub use code_action::handle_code_actions;
pub use definition::{
    ContextType, DefinitionCandidate, DefinitionResolver, ReferenceContext, handle_goto_definition,
};
pub use selection::handle_selection_range;
pub use semantic_tokens::{
    LEGEND_MODIFIERS, LEGEND_TYPES, handle_semantic_tokens_full, handle_semantic_tokens_full_delta,
    handle_semantic_tokens_range,
};
