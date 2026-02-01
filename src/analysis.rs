pub mod offset_calculator;
pub mod refactor;
pub mod result_id;
pub mod selection;
pub mod semantic;
pub mod semantic_cache;

// Re-export main types and functions
pub use refactor::handle_code_actions;
pub use result_id::next_result_id;
pub use selection::handle_selection_range;
pub use semantic::{
    LEGEND_MODIFIERS, LEGEND_TYPES, calculate_delta_or_full, handle_semantic_tokens_full,
    handle_semantic_tokens_full_delta, handle_semantic_tokens_full_parallel,
    handle_semantic_tokens_range,
};
// Re-export parallel processing functions for LSP integration
pub(crate) use semantic::{
    handle_semantic_tokens_full_parallel_async, handle_semantic_tokens_range_parallel_async,
};
pub use semantic_cache::{InjectionMap, InjectionTokenCache, SemanticTokenCache};
