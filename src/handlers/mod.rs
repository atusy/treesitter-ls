// Re-export from the new features module for backward compatibility
pub use crate::features::code_action::{self, handle_code_actions};
pub use crate::features::definition::{
    self, ContextType, DefinitionCandidate, DefinitionResolver, ReferenceContext,
    handle_goto_definition,
};
pub use crate::features::selection_range::{self, handle_selection_range};
pub use crate::features::semantic_tokens::{
    self, LEGEND_MODIFIERS, LEGEND_TYPES, handle_semantic_tokens_full,
    handle_semantic_tokens_full_delta, handle_semantic_tokens_range,
};
