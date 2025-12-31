//! Hover types for language server bridge.
//!
//! This module contains types for bridging hover requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::Hover;

/// Result of `hover_with_notifications` containing
/// the hover response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct HoverWithNotifications {
    /// The hover response (None if no result or error)
    pub response: Option<Hover>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
