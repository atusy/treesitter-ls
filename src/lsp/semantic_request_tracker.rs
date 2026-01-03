//! Request tracking for semantic token operations to support cancellation and debouncing.
//!
//! # Cancellation Support
//!
//! When a new semantic token request arrives for a URI, it supersedes any in-flight request
//! for that URI. Handlers check `is_active()` at strategic points and exit early if superseded.
//!
//! This addresses the primary use case: users typing rapidly generate many semantic token
//! requests, and we want to skip computation for obsolete requests.

use dashmap::DashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use url::Url;

/// Monotonically increasing request ID for tracking
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Generates a unique request ID
fn next_request_id() -> u64 {
    NEXT_REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// Tracks active semantic token requests to support cancellation
#[derive(Debug, Clone)]
pub struct SemanticRequestTracker {
    /// Maps URI to the most recent active request ID
    active_requests: Arc<DashMap<Url, u64>>,
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
        }
    }

    /// Starts tracking a new request for the given URI.
    /// Returns a request ID that should be passed to subsequent operations.
    /// Automatically supersedes any previous request for the same URI.
    pub fn start_request(&self, uri: &Url) -> u64 {
        let request_id = next_request_id();
        self.active_requests.insert(uri.clone(), request_id);
        request_id
    }

    /// Checks if a request is still active (not superseded by a newer one).
    /// Returns true if the request should continue, false if it should abort.
    pub fn is_active(&self, uri: &Url, request_id: u64) -> bool {
        self.active_requests
            .get(uri)
            .map(|entry| *entry == request_id)
            .unwrap_or(false)
    }

    /// Finishes a request, removing it from tracking if it's still the active one.
    /// This prevents memory leaks from completed requests.
    pub fn finish_request(&self, uri: &Url, request_id: u64) {
        self.active_requests
            .remove_if(uri, |_, id| *id == request_id);
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
    fn test_cancel_all_for_uri() {
        let tracker = SemanticRequestTracker::new();
        let uri = Url::parse("file:///test.lua").unwrap();

        let req = tracker.start_request(&uri);
        tracker.cancel_all_for_uri(&uri);
        assert!(!tracker.is_active(&uri, req), "Request should be cancelled");
    }
}
