//! CodeAction types for language server bridge.
//!
//! This module contains types for bridging code action requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::CodeActionResponse;

/// Result of `code_action_with_notifications` containing
/// the code action response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct CodeActionWithNotifications {
    /// The code action response (None if no result or error)
    pub response: Option<CodeActionResponse>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
