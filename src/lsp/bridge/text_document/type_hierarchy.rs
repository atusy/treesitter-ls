//! Type hierarchy types for language server bridge.
//!
//! This module contains types for bridging type hierarchy requests
//! to external language servers. It supports three related methods:
//! - textDocument/prepareTypeHierarchy
//! - typeHierarchy/supertypes
//! - typeHierarchy/subtypes

use serde_json::Value;
use tower_lsp::lsp_types::TypeHierarchyItem;

/// Result of `prepare_type_hierarchy_with_notifications` containing
/// the prepare type hierarchy response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct PrepareTypeHierarchyWithNotifications {
    /// The prepare type hierarchy response (None if no result or error)
    pub response: Option<Vec<TypeHierarchyItem>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}

/// Result of `supertypes_with_notifications` containing
/// the supertypes response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct SupertypesWithNotifications {
    /// The supertypes response (None if no result or error)
    pub response: Option<Vec<TypeHierarchyItem>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}

/// Result of `subtypes_with_notifications` containing
/// the subtypes response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct SubtypesWithNotifications {
    /// The subtypes response (None if no result or error)
    pub response: Option<Vec<TypeHierarchyItem>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
