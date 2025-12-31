//! SignatureHelp types for language server bridge.
//!
//! This module contains types for bridging signatureHelp requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::SignatureHelp;

/// Result of `signature_help_with_notifications` containing
/// the signature help response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct SignatureHelpWithNotifications {
    /// The signature help response (None if no result or error)
    pub response: Option<SignatureHelp>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
