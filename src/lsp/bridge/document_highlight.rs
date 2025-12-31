//! Document highlight types for language server bridge.
//!
//! This module contains types for bridging document highlight requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::DocumentHighlight;

/// Result of `document_highlight_with_notifications` containing
/// the document highlight response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct DocumentHighlightWithNotifications {
    /// The document highlight response (None if no result or error)
    pub response: Option<Vec<DocumentHighlight>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
