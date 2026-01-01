//! Go-to-declaration types for language server bridge.
//!
//! This module contains types for bridging go-to-declaration requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::GotoDefinitionResponse;

// GotoDeclarationResponse is an alias for GotoDefinitionResponse in tower-lsp

/// Result of `declaration_with_notifications` containing
/// the goto declaration response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct DeclarationWithNotifications {
    /// The goto declaration response (None if no result or error)
    pub response: Option<GotoDefinitionResponse>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
