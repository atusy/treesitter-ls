//! Text document request handlers for bridge connections.
//!
//! This module provides LSP text document request functionality (hover, completion, etc.)
//! for downstream language servers via the bridge architecture.
//!
//! The structure mirrors `lsp_impl/text_document/` for consistency.

mod completion;
mod declaration;
mod definition;
mod did_change;
mod did_close;
mod document_color;
mod document_highlight;
mod document_link;
mod document_symbol;
mod hover;
mod implementation;
mod inlay_hint;
mod references;
mod rename;
mod signature_help;
mod type_definition;
