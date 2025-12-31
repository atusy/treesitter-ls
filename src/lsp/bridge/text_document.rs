//! Text document bridge types.
//!
//! This module contains types for bridging textDocument/* LSP requests
//! to external language servers for injection regions.

mod code_action;
mod completion;
mod definition;
mod document_highlight;
mod formatting;
mod hover;
mod implementation;
mod references;
mod rename;
mod signature_help;
mod type_definition;

pub use code_action::CodeActionWithNotifications;
pub use completion::CompletionWithNotifications;
pub use definition::GotoDefinitionWithNotifications;
pub use document_highlight::DocumentHighlightWithNotifications;
pub use formatting::FormattingWithNotifications;
pub use hover::HoverWithNotifications;
pub use implementation::ImplementationWithNotifications;
pub use references::ReferencesWithNotifications;
pub use rename::RenameWithNotifications;
pub use signature_help::SignatureHelpWithNotifications;
pub use type_definition::TypeDefinitionWithNotifications;
