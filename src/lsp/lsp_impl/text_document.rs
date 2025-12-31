//! Text document related LSP methods.

mod call_hierarchy;
mod code_action;
mod completion;
mod declaration;
mod definition;
mod document_highlight;
mod document_link;
mod formatting;
mod hover;
mod implementation;
mod inlay_hint;
mod references;
mod rename;
mod selection_range;
mod semantic_tokens;
mod signature_help;
mod type_definition;
mod type_hierarchy;

// Re-export the methods (they are implemented as impl blocks on TreeSitterLs)
