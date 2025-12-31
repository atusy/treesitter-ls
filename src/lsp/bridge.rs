//! LSP Bridge for injection regions
//!
//! This module handles bridging LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

mod cleanup;
mod code_action;
mod completion;
mod connection;
mod definition;
mod formatting;
mod hover;
mod implementation;
mod pool;
mod references;
mod rename;
mod signature_help;
mod type_definition;
mod workspace;

// Re-export public types and functions
pub use cleanup::{
    CleanupStats, DEFAULT_CLEANUP_MAX_AGE, TEMP_DIR_PREFIX, cleanup_stale_temp_dirs,
    startup_cleanup,
};
pub use code_action::CodeActionWithNotifications;
pub use completion::CompletionWithNotifications;
pub use connection::{ConnectionInfo, LanguageServerConnection, ResponseWithNotifications};
pub use definition::GotoDefinitionWithNotifications;
pub use formatting::FormattingWithNotifications;
pub use hover::HoverWithNotifications;
pub use implementation::ImplementationWithNotifications;
pub use pool::LanguageServerPool;
pub use references::ReferencesWithNotifications;
pub use rename::RenameWithNotifications;
pub use signature_help::SignatureHelpWithNotifications;
pub use type_definition::TypeDefinitionWithNotifications;
pub use workspace::{setup_workspace, setup_workspace_with_option};
