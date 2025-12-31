//! References types for language server bridge.
//!
//! This module contains types for bridging references requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::Location;

/// Result of `references_with_notifications` containing
/// the references response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct ReferencesWithNotifications {
    /// The references response (None if no result or error)
    pub response: Option<Vec<Location>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
