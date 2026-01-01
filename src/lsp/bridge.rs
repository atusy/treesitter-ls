//! LSP Bridge for injection regions
//!
//! This module handles bridging LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

mod async_connection;
mod async_pool;
mod cleanup;
mod connection;
mod pool;
mod text_document;
mod workspace;

// Re-export public types and functions
pub use async_connection::{AsyncBridgeConnection, ResponseResult};
pub use async_pool::AsyncLanguageServerPool;
pub use cleanup::{
    CleanupStats, DEFAULT_CLEANUP_MAX_AGE, TEMP_DIR_PREFIX, cleanup_stale_temp_dirs,
    startup_cleanup,
};
pub use connection::{
    ConnectionInfo, DidOpenResult, LanguageServerConnection, ResponseWithNotifications,
};
pub use pool::LanguageServerPool;
pub use text_document::{
    CodeActionWithNotifications, CompletionWithNotifications, DocumentHighlightWithNotifications,
    DocumentLinkWithNotifications, FoldingRangeWithNotifications, FormattingWithNotifications,
    GotoDefinitionWithNotifications, HoverWithNotifications, ImplementationWithNotifications,
    IncomingCallsWithNotifications, InlayHintWithNotifications, OutgoingCallsWithNotifications,
    PrepareCallHierarchyWithNotifications, PrepareTypeHierarchyWithNotifications,
    ReferencesWithNotifications, RenameWithNotifications, SignatureHelpWithNotifications,
    SubtypesWithNotifications, SupertypesWithNotifications, TypeDefinitionWithNotifications,
};
// VirtualToHostRegistry is used for diagnostic forwarding (PBI-135)
#[allow(unused_imports)]
pub(crate) use text_document::VirtualToHostRegistry;
pub use workspace::{setup_workspace, setup_workspace_with_option};
