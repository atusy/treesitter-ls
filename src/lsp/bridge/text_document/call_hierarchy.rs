//! Call hierarchy types for language server bridge.
//!
//! This module contains types for bridging call hierarchy requests
//! to external language servers. It supports three related methods:
//! - textDocument/prepareCallHierarchy
//! - callHierarchy/incomingCalls
//! - callHierarchy/outgoingCalls

use serde_json::Value;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall,
};

/// Result of `prepare_call_hierarchy_with_notifications` containing
/// the prepare call hierarchy response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct PrepareCallHierarchyWithNotifications {
    /// The prepare call hierarchy response (None if no result or error)
    pub response: Option<Vec<CallHierarchyItem>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}

/// Result of `incoming_calls_with_notifications` containing
/// the incoming calls response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct IncomingCallsWithNotifications {
    /// The incoming calls response (None if no result or error)
    pub response: Option<Vec<CallHierarchyIncomingCall>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}

/// Result of `outgoing_calls_with_notifications` containing
/// the outgoing calls response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct OutgoingCallsWithNotifications {
    /// The outgoing calls response (None if no result or error)
    pub response: Option<Vec<CallHierarchyOutgoingCall>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
