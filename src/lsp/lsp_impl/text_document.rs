//! Text document related LSP methods.

mod completion;
mod definition;
mod hover;
mod selection_range;
mod semantic_tokens;
mod signature_help;
mod type_definition;

// Re-export the methods (they are implemented as impl blocks on TreeSitterLs)
