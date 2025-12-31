//! Text document related LSP methods.

mod code_action;
mod completion;
mod definition;
mod document_highlight;
mod formatting;
mod hover;
mod implementation;
mod references;
mod rename;
mod selection_range;
mod semantic_tokens;
mod signature_help;
mod type_definition;

// Re-export the methods (they are implemented as impl blocks on TreeSitterLs)
