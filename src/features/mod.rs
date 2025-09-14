// Re-export from analysis module for backward compatibility
pub use crate::analysis::{
    ContextType,
    DefinitionCandidate,
    DefinitionResolver,
    LEGEND_MODIFIERS,
    LEGEND_TYPES,
    ReferenceContext,
    // From refactor (was code_action)
    handle_code_actions,
    // From definition
    handle_goto_definition,
    // From selection
    handle_selection_range,
    // From semantic
    handle_semantic_tokens_full,
    handle_semantic_tokens_full_delta,
    handle_semantic_tokens_range,
};

// Module aliases for backward compatibility
pub mod code_action {
    pub use crate::analysis::refactor::*;
}

pub mod definition {
    pub use crate::analysis::definition::*;
}

pub mod selection {
    pub use crate::analysis::selection::*;
}

pub mod semantic_tokens {
    pub use crate::analysis::semantic::*;
}

// For backward compatibility, re-export selection as selection_range
pub use self::selection as selection_range;
