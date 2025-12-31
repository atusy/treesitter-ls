//! Folding range types for language server bridge.
//!
//! This module contains types for bridging folding range requests
//! to external language servers.

use serde_json::Value;
use tower_lsp::lsp_types::FoldingRange;

/// Result of `folding_range_with_notifications` containing
/// the folding range response and any $/progress notifications captured.
#[derive(Debug, Clone)]
pub struct FoldingRangeWithNotifications {
    /// The folding range response (None if no result or error)
    pub response: Option<Vec<FoldingRange>>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}
