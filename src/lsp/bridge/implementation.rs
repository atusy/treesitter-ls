//! Go-to-implementation types for language server bridge.
//!
//! This module contains types for bridging go-to-implementation requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::GotoDefinitionResponse;

// GotoImplementationResponse is an alias for GotoDefinitionResponse in tower-lsp

/// Result of `implementation_with_notifications` containing
/// the goto implementation response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct ImplementationWithNotifications {
    /// The goto implementation response (None if no result or error)
    pub response: Option<GotoDefinitionResponse>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
