//! Inlay hint types for language server bridge.
//!
//! This module contains types for bridging inlay hint requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::InlayHint;

/// Result of `inlay_hint_with_notifications` containing
/// the inlay hint response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct InlayHintWithNotifications {
    /// The inlay hint response (None if no result or error)
    pub response: Option<Vec<InlayHint>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
