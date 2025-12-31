//! LSP Redirection for injection regions
//!
//! This module handles redirecting LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

mod cleanup;
mod connection;
mod definition;
mod hover;
mod pool;
mod workspace;

// Re-export public types and functions
pub use cleanup::{
    CleanupStats, DEFAULT_CLEANUP_MAX_AGE, TEMP_DIR_PREFIX, cleanup_stale_temp_dirs,
    startup_cleanup,
};
pub use connection::{ConnectionInfo, LanguageServerConnection, ResponseWithNotifications};
pub use definition::GotoDefinitionWithNotifications;
pub use hover::HoverWithNotifications;
pub use pool::LanguageServerPool;
pub use workspace::{setup_workspace, setup_workspace_with_option};
