//! Completion types for language server bridge.
//!
//! This module contains types for bridging completion requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::CompletionResponse;

/// Result of `completion_with_notifications` containing
/// the completion response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct CompletionWithNotifications {
    /// The completion response (None if no result or error)
    pub response: Option<CompletionResponse>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
