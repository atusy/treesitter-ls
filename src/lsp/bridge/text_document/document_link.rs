//! Document link types for language server bridge.
//!
//! This module contains types for bridging document link requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::DocumentLink;

/// Result of `document_link_with_notifications` containing
/// the document link response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct DocumentLinkWithNotifications {
    /// The document link response (None if no result or error)
    pub response: Option<Vec<DocumentLink>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
