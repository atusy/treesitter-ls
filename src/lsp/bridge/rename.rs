//! Rename types for language server bridge.
//!
//! This module contains types for bridging rename requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::WorkspaceEdit;

/// Result of `rename_with_notifications` containing
/// the rename response (WorkspaceEdit) and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct RenameWithNotifications {
    /// The rename response (None if no result or error)
    pub response: Option<WorkspaceEdit>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
