//! Request tracking for semantic token operations to support cancellation and debouncing.
//!
//! # Cancellation Support
//!
//! Tower-lsp v0.20 handles `$/cancelRequest` notifications internally at the JSON-RPC layer
//! but doesn't expose them to user handlers. This module provides infrastructure for:
//!
//! 1. **Supersession-based cancellation** (fully supported): When a new semantic token request
//!    arrives for a URI, it supersedes any in-flight request for that URI. Handlers check
//!    `is_active()` at strategic points and exit early if superseded.
//!
//! 2. **Explicit cancellation** (infrastructure ready): The `cancel_by_lsp_request_id()` method
//!    is implemented and tested, ready to use when/if tower-lsp exposes `$/cancelRequest` to handlers.
//!    For now, callers can use `cancel_all_for_uri()` to cancel all requests for a document.
//!
//! 3. **Debouncing**: Prevents unbounded queue buildup during rapid edits by ensuring only the
//!    latest request per URI remains active.
//!
//! # Practical Impact
//!
//! The supersession mechanism addresses the primary use case: users typing rapidly generate
//! many semantic token requests, and we want to skip computation for obsolete requests.
//! The missing explicit `$/cancelRequest` support is a minor gap that affects only the edge
//! case where a client explicitly cancels a request without sending a replacement.

use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use url::Url;

/// Monotonically increasing request ID for tracking
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Generates a unique request ID
fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// Information about an active semantic token request
#[derive(Debug, Clone)]
struct RequestInfo {
    id: u64,
    #[allow(dead_code)] // Reserved for cleanup_stale() timing checks
    started_at: Instant,
}

/// Tracks active semantic token requests to support cancellation
#[derive(Debug, Clone)]
pub struct SemanticRequestTracker {
    /// Maps URI to the most recent active request ID
    active_requests: Arc<DashMap<Url, RequestInfo>>,
    /// Maps LSP request ID (from client) to (URI, internal_request_id) for $/cancelRequest support
    lsp_request_map: Arc<DashMap<i64, (Url, u64)>>,
}

impl Default for SemanticRequestTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticRequestTracker {
    /// Creates a new request tracker
    pub fn new() -> Self {
        Self {
            active_requests: Arc::new(DashMap::new()),
            lsp_request_map: Arc::new(DashMap::new()),
        }
    }

    /// Starts tracking a new request for the given URI.
    /// Returns a request ID that should be passed to subsequent operations.
    /// Automatically supersedes any previous request for the same URI.
    pub fn start_request(&self, uri: &Url) -> u64 {
        let request_id = next_request_id();
        let info = RequestInfo {
            id: request_id,
            started_at: Instant::now(),
        };
        self.active_requests.insert(uri.clone(), info);
        request_id
    }

    /// Checks if a request is still active (not superseded by a newer one).
    /// Returns true if the request should continue, false if it should abort.
    pub fn is_active(&self, uri: &Url, request_id: u64) -> bool {
        self.active_requests
            .get(uri)
            .map(|entry| entry.id == request_id)
            .unwrap_or(false)
    }

    /// Finishes a request, removing it from tracking if it's still the active one.
    /// This prevents memory leaks from completed requests.
    pub fn finish_request(&self, uri: &Url, request_id: u64) {
        self.active_requests
            .remove_if(uri, |_, info| info.id == request_id);

        // Clean up LSP request ID mapping
        // We don't know the LSP request ID here, so we need to scan and remove
        self.lsp_request_map.retain(|_, (u, rid)| {
            !(u == uri && *rid == request_id)
        });
    }

    /// Registers an LSP request ID for a semantic token request.
    ///
    /// This allows `$/cancelRequest` notifications to cancel the request by LSP ID.
    ///
    /// # Note
    ///
    /// Tower-lsp v0.20 doesn't expose `$/cancelRequest` to handlers, so this method
    /// is currently unused. It's implemented and tested for future compatibility
    /// if tower-lsp adds support or if we switch to a framework that exposes cancellation.
    ///
    /// # Arguments
    ///
    /// * `lsp_request_id` - The JSON-RPC request ID from the client
    /// * `uri` - The document URI for this request
    /// * `internal_request_id` - Our internal tracking ID from `start_request()`
    pub fn register_lsp_request_id(&self, lsp_request_id: i64, uri: &Url, internal_request_id: u64) {
        self.lsp_request_map.insert(lsp_request_id, (uri.clone(), internal_request_id));
    }

    /// Cancels a request by its LSP request ID (from `$/cancelRequest` notification).
    ///
    /// This marks the request as inactive so handlers can exit early at their next
    /// `is_active()` checkpoint.
    ///
    /// # Note
    ///
    /// Tower-lsp v0.20 doesn't expose `$/cancelRequest` to handlers, so this method
    /// cannot currently be called. It's implemented and tested for future compatibility.
    ///
    /// For immediate cancellation needs, use `cancel_all_for_uri()` instead.
    ///
    /// # Arguments
    ///
    /// * `lsp_request_id` - The JSON-RPC request ID to cancel
    pub fn cancel_by_lsp_request_id(&self, lsp_request_id: i64) {
        if let Some((_, (uri, internal_request_id))) = self.lsp_request_map.remove(&lsp_request_id) {
            // Mark the request as inactive by removing it from active_requests
            self.active_requests.remove_if(&uri, |_, info| info.id == internal_request_id);
        }
    }

    /// Cleans up stale requests (older than the given duration).
    /// This is a safety mechanism to prevent memory leaks from abandoned requests.
    #[allow(dead_code)] // Reserved for future periodic cleanup task
    pub fn cleanup_stale(&self, max_age: Duration) {
        let now = Instant::now();
        self.active_requests
            .retain(|_, info| now.duration_since(info.started_at) < max_age);
    }

    /// Cancels all requests for a given URI.
    /// Useful when a document is closed.
    pub fn cancel_all_for_uri(&self, uri: &Url) {
        self.active_requests.remove(uri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_tracking_basic() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let req1 = tracker.start_request(&uri);
        assert!(tracker.is_active(&uri, req1), "Request should be active");

        tracker.finish_request(&uri, req1);
        assert!(!tracker.is_active(&uri, req1), "Request should be finished");
    }

    #[test]
    fn test_request_superseding() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let req1 = tracker.start_request(&uri);
        assert!(
            tracker.is_active(&uri, req1),
            "First request should be active"
        );

        // Start a new request - should supersede the first
        let req2 = tracker.start_request(&uri);
        assert!(
            !tracker.is_active(&uri, req1),
            "First request should be superseded"
        );
        assert!(
            tracker.is_active(&uri, req2),
            "Second request should be active"
        );
    }

    #[test]
    fn test_multiple_uris() {
        let tracker = SemanticRequestTracker::new();
        let uri1 = Url::parse("file:///test1.lua").unwrap();
        let uri2 = Url::parse("file:///test2.lua").unwrap();

        let req1 = tracker.start_request(&uri1);
        let req2 = tracker.start_request(&uri2);

        assert!(tracker.is_active(&uri1, req1), "Request 1 should be active");
        assert!(tracker.is_active(&uri2, req2), "Request 2 should be active");

        // Requests for different URIs don't interfere
        let req3 = tracker.start_request(&uri1);
        assert!(
            !tracker.is_active(&uri1, req1),
            "Request 1 should be superseded"
        );
        assert!(
            tracker.is_active(&uri2, req2),
            "Request 2 should still be active"
        );
        assert!(tracker.is_active(&uri1, req3), "Request 3 should be active");
    }

    #[test]
    fn test_cleanup_stale() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let req = tracker.start_request(&uri);

        // Immediate cleanup shouldn't remove fresh requests
        tracker.cleanup_stale(Duration::from_millis(100));
        assert!(
            tracker.is_active(&uri, req),
            "Fresh request should not be cleaned up"
        );

        // Sleep and cleanup with very short duration
        std::thread::sleep(Duration::from_millis(10));
        tracker.cleanup_stale(Duration::from_millis(5));
        assert!(
            !tracker.is_active(&uri, req),
            "Old request should be cleaned up"
        );
    }

    #[test]
    fn test_cancel_all_for_uri() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let req = tracker.start_request(&uri);
        tracker.cancel_all_for_uri(&uri);
        assert!(!tracker.is_active(&uri, req), "Request should be cancelled");
    }

    #[test]
    fn test_cancel_by_lsp_request_id() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        // Start a request and register its LSP request ID
        let internal_req_id = tracker.start_request(&uri);
        let lsp_req_id = 42;
        tracker.register_lsp_request_id(lsp_req_id, &uri, internal_req_id);

        // Verify request is active
        assert!(
            tracker.is_active(&uri, internal_req_id),
            "Request should be active before cancellation"
        );

        // Cancel by LSP request ID
        tracker.cancel_by_lsp_request_id(lsp_req_id);

        // Verify request is now cancelled
        assert!(
            !tracker.is_active(&uri, internal_req_id),
            "Request should be cancelled after cancel_by_lsp_request_id"
        );
    }

    #[test]
    fn test_cancel_by_lsp_request_id_unknown_id() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let internal_req_id = tracker.start_request(&uri);

        // Try to cancel with an unregistered LSP request ID (should be a no-op)
        tracker.cancel_by_lsp_request_id(999);

        // Request should still be active
        assert!(
            tracker.is_active(&uri, internal_req_id),
            "Request should still be active if LSP request ID is unknown"
        );
    }

    #[test]
    fn test_lsp_request_id_cleanup_on_finish() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let internal_req_id = tracker.start_request(&uri);
        let lsp_req_id = 42;
        tracker.register_lsp_request_id(lsp_req_id, &uri, internal_req_id);

        // Finish the request
        tracker.finish_request(&uri, internal_req_id);

        // Try to cancel by LSP request ID (should be a no-op since request is finished)
        tracker.cancel_by_lsp_request_id(lsp_req_id);

        // Request should not be active (already finished)
        assert!(
            !tracker.is_active(&uri, internal_req_id),
            "Request should not be active after finish"
        );
    }
}
