//! LSP Bridge for injection regions
//!
//! This module handles bridging LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

mod cleanup;
mod completion;
mod connection;
mod definition;
mod hover;
mod pool;
mod references;
mod signature_help;
mod workspace;

// Re-export public types and functions
pub use cleanup::{
    CleanupStats, DEFAULT_CLEANUP_MAX_AGE, TEMP_DIR_PREFIX, cleanup_stale_temp_dirs,
    startup_cleanup,
};
pub use completion::CompletionWithNotifications;
pub use connection::{ConnectionInfo, LanguageServerConnection, ResponseWithNotifications};
pub use definition::GotoDefinitionWithNotifications;
pub use hover::HoverWithNotifications;
pub use pool::LanguageServerPool;
pub use references::ReferencesWithNotifications;
pub use signature_help::SignatureHelpWithNotifications;
pub use workspace::{setup_workspace, setup_workspace_with_option};
