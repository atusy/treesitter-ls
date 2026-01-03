//! Text document bridge types.
//!
//! This module contains types for bridging textDocument/* LSP requests
//! to external language servers for injection regions.

mod completion;
mod definition;
mod hover;
mod signature_help;

pub use completion::CompletionWithNotifications;
pub use definition::GotoDefinitionWithNotifications;
pub use hover::HoverWithNotifications;
pub use signature_help::SignatureHelpWithNotifications;
