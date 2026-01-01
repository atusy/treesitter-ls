//! Formatting types for language server bridge.
//!
//! This module contains types for bridging formatting requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::TextEdit;

/// Result of `formatting_with_notifications` containing
/// the formatting response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct FormattingWithNotifications {
    /// The formatting response (None if no result or error)
    pub response: Option<Vec<TextEdit>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
