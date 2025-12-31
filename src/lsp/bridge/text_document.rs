//! Text document bridge types.
//!
//! This module contains types for bridging textDocument/* LSP requests
//! to external language servers for injection regions.

mod call_hierarchy;
mod code_action;
mod completion;
mod declaration;
mod definition;
mod document_highlight;
mod document_link;
mod folding_range;
mod formatting;
mod hover;
mod implementation;
mod inlay_hint;
mod references;
mod rename;
mod signature_help;
mod type_definition;
mod type_hierarchy;

pub use call_hierarchy::{
    IncomingCallsWithNotifications, OutgoingCallsWithNotifications,
    PrepareCallHierarchyWithNotifications,
};
pub use code_action::CodeActionWithNotifications;
pub use completion::CompletionWithNotifications;
pub use declaration::DeclarationWithNotifications;
pub use definition::GotoDefinitionWithNotifications;
pub use document_highlight::DocumentHighlightWithNotifications;
pub use document_link::DocumentLinkWithNotifications;
pub use folding_range::FoldingRangeWithNotifications;
pub use formatting::FormattingWithNotifications;
pub use hover::HoverWithNotifications;
pub use implementation::ImplementationWithNotifications;
pub use inlay_hint::InlayHintWithNotifications;
pub use references::ReferencesWithNotifications;
pub use rename::RenameWithNotifications;
pub use signature_help::SignatureHelpWithNotifications;
pub use type_definition::TypeDefinitionWithNotifications;
pub use type_hierarchy::{
    PrepareTypeHierarchyWithNotifications, SubtypesWithNotifications, SupertypesWithNotifications,
};
