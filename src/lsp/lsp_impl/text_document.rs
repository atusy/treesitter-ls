//! Text document related LSP methods.

mod completion;
mod definition;
mod hover;
mod selection_range;
mod semantic_tokens;
mod signature_help;

// Re-export the methods (they are implemented as impl blocks on TreeSitterLs)
