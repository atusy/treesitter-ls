pub mod definition;
pub mod refactor;
pub mod selection;
pub mod semantic;
pub mod traits;

// Re-export main types and functions
pub use definition::{
    ContextType, DefinitionCandidate, DefinitionResolver, ReferenceContext, handle_goto_definition,
};
pub use refactor::handle_code_actions;
pub use selection::handle_selection_range;
pub use semantic::{
    LEGEND_MODIFIERS, LEGEND_TYPES, handle_semantic_tokens_full, handle_semantic_tokens_full_delta,
    handle_semantic_tokens_range,
};
pub use traits::AnalysisContext;
