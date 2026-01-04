//! LSP Bridge for injection regions
//!
//! This module handles bridging LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

mod cleanup;
mod connection;
mod error_types;
mod text_document;
mod tokio_async_pool;
mod tokio_connection;
mod workspace;

// Re-export public types and functions
pub use cleanup::{
    CleanupStats, DEFAULT_CLEANUP_MAX_AGE, TEMP_DIR_PREFIX, cleanup_stale_temp_dirs,
    startup_cleanup,
};
pub use connection::{ConnectionInfo, LanguageServerConnection, ResponseWithNotifications};
pub use error_types::{ErrorCodes, ResponseError};
pub use text_document::{
    CompletionWithNotifications, GotoDefinitionWithNotifications, HoverWithNotifications,
    SignatureHelpWithNotifications,
};
pub use tokio_async_pool::TokioAsyncLanguageServerPool;
pub use workspace::{setup_workspace, setup_workspace_with_option};
