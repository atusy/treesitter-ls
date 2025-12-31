//! Go-to-type-definition types for language server bridge.
//!
//! This module contains types for bridging go-to-type-definition requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::GotoDefinitionResponse;

// GotoTypeDefinitionResponse is an alias for GotoDefinitionResponse in tower-lsp

/// Result of `type_definition_with_notifications` containing
/// the goto type definition response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct TypeDefinitionWithNotifications {
    /// The goto type definition response (None if no result or error)
    pub response: Option<GotoDefinitionResponse>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
