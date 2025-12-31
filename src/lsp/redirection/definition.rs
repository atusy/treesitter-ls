//! Go-to-definition types for language server bridge.
//!
//! This module contains types for bridging go-to-definition requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::GotoDefinitionResponse;

/// Result of `goto_definition_with_notifications` containing
/// the goto definition response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct GotoDefinitionWithNotifications {
    /// The goto definition response (None if no result or error)
    pub response: Option<GotoDefinitionResponse>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
