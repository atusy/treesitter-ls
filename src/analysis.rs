pub mod definition;
pub mod incremental_tokens;
pub mod offset_calculator;
pub mod refactor;
pub mod result_id;
pub mod selection;
pub mod semantic;
pub mod semantic_cache;

// Re-export main types and functions
pub use definition::{DefinitionResolver, handle_goto_definition};
pub use incremental_tokens::{
    AbsoluteToken, IncrementalDecision, IncrementalTokensResult, changed_ranges_to_lines,
    compute_incremental_tokens, decide_tokenization_strategy, decode_semantic_tokens,
    encode_semantic_tokens, get_changed_ranges, is_large_structural_change, merge_tokens,
};
pub use refactor::handle_code_actions;
pub use result_id::next_result_id;
pub use selection::handle_selection_range;
pub use semantic::{
    LEGEND_MODIFIERS, LEGEND_TYPES, handle_semantic_tokens_full, handle_semantic_tokens_full_delta,
    handle_semantic_tokens_range,
};
pub use semantic_cache::{InjectionMap, InjectionTokenCache, SemanticTokenCache};
