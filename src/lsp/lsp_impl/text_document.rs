//! Text document related LSP methods.

mod completion;
mod declaration;
mod definition;
mod document_highlight;
mod document_link;
mod hover;
mod implementation;
mod references;
mod rename;
mod selection_range;
mod semantic_tokens;
mod signature_help;
mod type_definition;

// Re-export the methods (they are implemented as impl blocks on TreeSitterLs)
