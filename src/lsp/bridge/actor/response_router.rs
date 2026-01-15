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
}

impl ResponseRouter {
    /// Create a new empty ResponseRouter.
    pub(crate) fn new() -> Self {
        Self {
            pending: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Register a pending request and return a receiver for the response.
    ///
    /// Must be called before sending the request to ensure the response
    /// can be routed when it arrives.
    ///
    /// Returns `None` if a request with this ID is already pending (duplicate ID).
    pub(crate) fn register(&self, id: RequestId) -> Option<oneshot::Receiver<serde_json::Value>> {
        let (tx, rx) = oneshot::channel();
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());

        // Prevent duplicate registration
        if pending.contains_key(&id) {
            return None;
        }

        pending.insert(id, tx);
        Some(rx)
    }

    /// Route a response to its pending request.
    ///
    /// Extracts the request ID from the response and sends it to the
    /// corresponding waiter. If no waiter exists (response for unknown
    /// request), the response is dropped.
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

        match tx {
            Some(sender) => sender.send(response).is_ok(),
            None => false, // No waiter for this ID
        }
    }

    /// Get the number of pending requests.
    ///
    /// Primarily for testing and diagnostics.
    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        let pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        pending.len()
    }

    /// Fail all pending requests with an internal error response.
    ///
    /// Called when the connection fails (e.g., reader task panic) to ensure
    /// all waiters receive a response per LSP guarantee.
    pub(crate) fn fail_all(&self, error_message: &str) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        let entries: Vec<_> = pending.drain().collect();

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
}
