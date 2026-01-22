//! Response routing for pending LSP requests.
//!
//! This module provides the ResponseRouter which tracks pending requests
//! and routes incoming responses to their corresponding waiters via oneshot channels.
//!
//! # Architecture (ADR-0015)
//!
//! The ResponseRouter enables non-blocking response waiting:
//! - Before sending a request, register it via `register(id)` to get a oneshot Receiver
//! - The Reader Task calls `route(response)` when a response arrives
//! - The original requester awaits on the Receiver without holding any Mutex

use std::collections::HashMap;

use tokio::sync::oneshot;

use super::super::protocol::RequestId;

/// Routes responses to pending requests via oneshot channels.
///
/// Thread-safe router that tracks in-flight requests and delivers responses
/// to their waiters. Designed for use with a single Reader Task that calls
/// `route()` for each incoming response.
///
/// # Cancel Forwarding Support
///
/// The router also maintains a mapping from upstream request IDs to downstream
/// request IDs via `cancel_map`. This enables $/cancelRequest forwarding:
/// - When a request is registered with an upstream ID, the mapping is stored
/// - When a cancel notification arrives, `lookup_downstream_id()` translates it
/// - When a request completes (via `route()` or `remove()`), the mapping is cleaned up
///
/// # Usage
///
/// ```ignore
/// let router = ResponseRouter::new();
/// let rx = router.register(request_id);  // Before sending request
/// // ... send request to downstream server ...
/// let response = rx.await?;  // Wait without holding Mutex
/// ```
pub(crate) struct ResponseRouter {
    pending: std::sync::Mutex<HashMap<RequestId, oneshot::Sender<serde_json::Value>>>,
    /// Maps upstream request ID (from client) to downstream request ID (to LS).
    ///
    /// Used for $/cancelRequest forwarding: when the client cancels request 42,
    /// we look up that 42 maps to downstream ID 7, and forward the cancel to LS.
    cancel_map: std::sync::Mutex<HashMap<i64, RequestId>>,
}

impl ResponseRouter {
    /// Create a new empty ResponseRouter.
    pub(crate) fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
            cancel_map: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending request and return a receiver for the response.
    ///
    /// Must be called before sending the request to ensure the response
    /// can be routed when it arrives.
    ///
    /// Returns `None` if a request with this ID is already pending (duplicate ID).
    pub(crate) fn register(&self, id: RequestId) -> Option<oneshot::Receiver<serde_json::Value>> {
        self.register_with_upstream(id, None)
    }

    /// Register a pending request with upstream ID mapping for cancel forwarding.
    ///
    /// Like `register()`, but also stores a mapping from the upstream (client) request ID
    /// to the downstream (language server) request ID. This enables $/cancelRequest
    /// forwarding by translating upstream IDs to downstream IDs.
    ///
    /// # Arguments
    /// * `downstream_id` - The request ID used for the downstream language server
    /// * `upstream_id` - The original request ID from the upstream client (None for internal requests)
    ///
    /// Returns `None` if a request with this downstream ID is already pending.
    pub(crate) fn register_with_upstream(
        &self,
        downstream_id: RequestId,
        upstream_id: Option<i64>,
    ) -> Option<oneshot::Receiver<serde_json::Value>> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());

        // Prevent duplicate registration
        if pending.contains_key(&downstream_id) {
            return None;
        }

        pending.insert(downstream_id, tx);

        // Store the upstream->downstream mapping if upstream_id is provided
        if let Some(upstream) = upstream_id {
            let mut cancel_map = self.cancel_map.lock().unwrap_or_else(|e| e.into_inner());
            cancel_map.insert(upstream, downstream_id);
        }

        Some(rx)
    }

    /// Look up the downstream request ID for an upstream request ID.
    ///
    /// Used by $/cancelRequest forwarding to translate the client's request ID
    /// to the language server's request ID.
    ///
    /// Returns `None` if no mapping exists (request not found or already completed).
    pub(crate) fn lookup_downstream_id(&self, upstream_id: i64) -> Option<RequestId> {
        let cancel_map = self.cancel_map.lock().unwrap_or_else(|e| e.into_inner());
        cancel_map.get(&upstream_id).copied()
    }

    /// Route a response to its pending request.
    ///
    /// Extracts the request ID from the response and sends it to the
    /// corresponding waiter. If no waiter exists (response for unknown
    /// request), the response is dropped.
    ///
    /// Also cleans up the cancel map entry for this request ID.
    ///
    /// Returns `true` if the response was delivered, `false` otherwise.
    pub(crate) fn route(&self, response: serde_json::Value) -> bool {
        let id = match RequestId::from_json(&response) {
            Some(id) => id,
            None => return false, // Not a response (notification), skip
        };

        let tx = {
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            pending.remove(&id)
        };

        // Clean up cancel map entry (remove by downstream ID value)
        {
            let mut cancel_map = self.cancel_map.lock().unwrap_or_else(|e| e.into_inner());
            cancel_map.retain(|_, downstream_id| *downstream_id != id);
        }

        match tx {
            Some(sender) => sender.send(response).is_ok(),
            None => false, // No waiter for this ID
        }
    }

    /// Get the number of pending requests.
    ///
    /// Used for liveness timeout management (ADR-0014):
    /// - Timer starts when pending transitions 0 -> 1
    /// - Timer stops when pending transitions to 0
    pub(crate) fn pending_count(&self) -> usize {
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.len()
    }

    /// Remove a pending request without sending a response.
    ///
    /// Used for cleanup when a request fails before being sent to the downstream server.
    /// Also cleans up the cancel map entry for this request ID.
    ///
    /// Returns `true` if the request was removed, `false` if it wasn't pending.
    pub(crate) fn remove(&self, id: RequestId) -> bool {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let removed = pending.remove(&id).is_some();

        if removed {
            // Clean up cancel map entry (remove by downstream ID value)
            let mut cancel_map = self.cancel_map.lock().unwrap_or_else(|e| e.into_inner());
            cancel_map.retain(|_, downstream_id| *downstream_id != id);
        }

        removed
    }

    /// Fail all pending requests with an internal error response.
    ///
    /// Called when the connection fails (e.g., reader task panic) to ensure
    /// all waiters receive a response per LSP guarantee.
    ///
    /// Also clears the cancel map since all requests are being completed.
    pub(crate) fn fail_all(&self, error_message: &str) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let entries: Vec<_> = pending.drain().collect();

        // Clear the cancel map
        {
            let mut cancel_map = self.cancel_map.lock().unwrap_or_else(|e| e.into_inner());
            cancel_map.clear();
        }

        for (id, tx) in entries {
            let error_response = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id.as_i64(),
                "error": {
                    "code": -32603, // InternalError
                    "message": error_message
                }
            });
            let _ = tx.send(error_response);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn new_router_has_no_pending_requests() {
        let router = ResponseRouter::new();
        assert_eq!(router.pending_count(), 0);
    }

    #[test]
    fn register_returns_receiver_and_increments_pending() {
        let router = ResponseRouter::new();
        let id = RequestId::new(1);

        let rx = router.register(id);
        assert!(rx.is_some(), "register should return Some(receiver)");
        assert_eq!(router.pending_count(), 1);
    }

    #[test]
    fn register_duplicate_id_returns_none() {
        let router = ResponseRouter::new();
        let id = RequestId::new(1);

        let rx1 = router.register(id);
        assert!(rx1.is_some());

        let rx2 = router.register(id);
        assert!(rx2.is_none(), "duplicate ID should return None");
        assert_eq!(router.pending_count(), 1, "count should not increase");
    }

    #[tokio::test]
    async fn route_delivers_response_to_waiter() {
        let router = ResponseRouter::new();
        let id = RequestId::new(42);

        let rx = router.register(id).expect("register should succeed");

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": { "contents": "Hello" }
        });

        let delivered = router.route(response.clone());
        assert!(delivered, "route should return true when delivered");

        let received = rx.await.expect("receiver should get response");
        assert_eq!(received["id"], 42);
        assert_eq!(received["result"]["contents"], "Hello");
    }

    #[test]
    fn route_returns_false_for_unknown_id() {
        let router = ResponseRouter::new();

        let response = json!({
            "jsonrpc": "2.0",
            "id": 999,
            "result": null
        });

        let delivered = router.route(response);
        assert!(!delivered, "route should return false for unknown ID");
    }

    #[test]
    fn route_returns_false_for_notification() {
        let router = ResponseRouter::new();
        let id = RequestId::new(1);
        let _rx = router.register(id);

        // Notification has no "id" field
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {}
        });

        let delivered = router.route(notification);
        assert!(!delivered, "route should return false for notifications");
        assert_eq!(router.pending_count(), 1, "pending request should remain");
    }

    #[tokio::test]
    async fn fail_all_sends_error_to_all_waiters() {
        let router = ResponseRouter::new();

        let rx1 = router.register(RequestId::new(1)).unwrap();
        let rx2 = router.register(RequestId::new(2)).unwrap();

        router.fail_all("connection lost");

        assert_eq!(router.pending_count(), 0, "all pending should be cleared");

        let response1 = rx1.await.expect("should receive error response");
        assert_eq!(response1["error"]["code"], -32603);
        assert_eq!(response1["error"]["message"], "connection lost");
        assert_eq!(response1["id"], 1);

        let response2 = rx2.await.expect("should receive error response");
        assert_eq!(response2["error"]["code"], -32603);
        assert_eq!(response2["id"], 2);
    }

    #[tokio::test]
    async fn route_after_receiver_dropped_returns_false() {
        let router = ResponseRouter::new();
        let id = RequestId::new(1);

        let rx = router.register(id).unwrap();
        drop(rx); // Simulate requester giving up

        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": null
        });

        let delivered = router.route(response);
        assert!(
            !delivered,
            "route should return false when receiver dropped"
        );
        assert_eq!(router.pending_count(), 0, "pending should be cleared");
    }

    #[test]
    fn remove_clears_pending_request() {
        let router = ResponseRouter::new();
        let id = RequestId::new(1);

        let _rx = router.register(id).unwrap();
        assert_eq!(router.pending_count(), 1);

        let removed = router.remove(id);
        assert!(removed, "remove should return true for pending request");
        assert_eq!(router.pending_count(), 0, "pending should be cleared");
    }

    #[test]
    fn remove_returns_false_for_unknown_id() {
        let router = ResponseRouter::new();
        let id = RequestId::new(999);

        let removed = router.remove(id);
        assert!(!removed, "remove should return false for unknown ID");
    }

    // ========================================
    // CancelMap tests (ADR-0015 Cancel Forwarding)
    // ========================================

    /// Test that register_with_upstream stores upstream->downstream mapping.
    ///
    /// When registering a request with an upstream ID, the router should store
    /// the mapping so we can later look up the downstream ID for cancel forwarding.
    #[test]
    fn register_with_upstream_stores_mapping() {
        let router = ResponseRouter::new();
        let downstream_id = RequestId::new(42);
        let upstream_id = 100i64;

        let rx = router.register_with_upstream(downstream_id, Some(upstream_id));
        assert!(rx.is_some(), "register_with_upstream should succeed");
        assert_eq!(router.pending_count(), 1);

        // Should be able to look up downstream ID by upstream ID
        let looked_up = router.lookup_downstream_id(upstream_id);
        assert_eq!(
            looked_up,
            Some(downstream_id),
            "lookup should return downstream ID for upstream ID"
        );
    }

    /// Test that register_with_upstream with None upstream_id behaves like register.
    #[test]
    fn register_with_upstream_none_behaves_like_register() {
        let router = ResponseRouter::new();
        let downstream_id = RequestId::new(42);

        let rx = router.register_with_upstream(downstream_id, None);
        assert!(rx.is_some(), "register_with_upstream should succeed");
        assert_eq!(router.pending_count(), 1);
    }

    /// Test that lookup_downstream_id returns None for unknown upstream ID.
    #[test]
    fn lookup_downstream_id_returns_none_for_unknown() {
        let router = ResponseRouter::new();

        let result = router.lookup_downstream_id(999);
        assert_eq!(
            result, None,
            "lookup should return None for unknown upstream ID"
        );
    }

    /// Test that route() removes the cancel map entry.
    ///
    /// After routing a response, the cancel map entry should be removed
    /// because the request is complete and no longer needs cancel forwarding.
    #[tokio::test]
    async fn route_removes_cancel_map_entry() {
        let router = ResponseRouter::new();
        let downstream_id = RequestId::new(42);
        let upstream_id = 100i64;

        let rx = router
            .register_with_upstream(downstream_id, Some(upstream_id))
            .expect("should register");

        // Verify mapping exists before route
        assert_eq!(
            router.lookup_downstream_id(upstream_id),
            Some(downstream_id)
        );

        // Route the response
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });
        let delivered = router.route(response);
        assert!(delivered, "route should succeed");

        // Verify mapping is removed after route
        assert_eq!(
            router.lookup_downstream_id(upstream_id),
            None,
            "cancel map entry should be removed after route"
        );

        // Clean up receiver
        let _ = rx.await;
    }

    /// Test that remove() also removes the cancel map entry.
    #[test]
    fn remove_also_removes_cancel_map_entry() {
        let router = ResponseRouter::new();
        let downstream_id = RequestId::new(42);
        let upstream_id = 100i64;

        let _rx = router
            .register_with_upstream(downstream_id, Some(upstream_id))
            .expect("should register");

        // Verify mapping exists before remove
        assert_eq!(
            router.lookup_downstream_id(upstream_id),
            Some(downstream_id)
        );

        // Remove the pending request
        let removed = router.remove(downstream_id);
        assert!(removed, "remove should succeed");

        // Verify mapping is removed
        assert_eq!(
            router.lookup_downstream_id(upstream_id),
            None,
            "cancel map entry should be removed after remove"
        );
    }

    /// Test that fail_all() clears the cancel map.
    #[tokio::test]
    async fn fail_all_clears_cancel_map() {
        let router = ResponseRouter::new();

        let _rx1 = router
            .register_with_upstream(RequestId::new(1), Some(100))
            .unwrap();
        let _rx2 = router
            .register_with_upstream(RequestId::new(2), Some(200))
            .unwrap();

        // Verify mappings exist
        assert!(router.lookup_downstream_id(100).is_some());
        assert!(router.lookup_downstream_id(200).is_some());

        router.fail_all("connection lost");

        // Verify mappings are cleared
        assert_eq!(
            router.lookup_downstream_id(100),
            None,
            "cancel map should be cleared by fail_all"
        );
        assert_eq!(router.lookup_downstream_id(200), None);
    }
}
