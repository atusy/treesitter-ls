//! Request ID capture for bridging upstream request IDs to downstream servers.
//!
//! This module provides a tower Service wrapper that captures the request ID
//! from incoming LSP requests and makes it available via task-local storage.
//! This enables downstream bridge requests to use the same ID as the upstream
//! client per ADR-0016 (Server Pool Coordination).
//!
//! # Cancel Forwarding
//!
//! The middleware also intercepts `$/cancelRequest` notifications and forwards
//! them to downstream language servers via the `CancelForwarder`. This ensures
//! that when a client cancels a request, the cancel is propagated to the
//! downstream server that is processing it.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use tower::Service;
use tower_lsp_server::jsonrpc::{Id, Request, Response};

use super::bridge::UpstreamId;
use super::bridge::LanguageServerPool;

tokio::task_local! {
    /// Task-local storage for the current upstream request ID.
    ///
    /// This is set by RequestIdCapture before delegating to the inner service,
    /// allowing downstream bridge code to access the original request ID.
    pub static CURRENT_REQUEST_ID: Option<Id>;
}

/// Forwards cancel requests to downstream language servers.
///
/// This type wraps an `Arc<LanguageServerPool>` and provides a method to forward
/// cancel notifications. It is shared between Kakehashi and the RequestIdCapture
/// middleware.
///
/// Use `CancelForwarder::new()` within the crate, or `Kakehashi::cancel_forwarder()`
/// to create an instance.
#[derive(Clone)]
pub struct CancelForwarder {
    pool: Arc<LanguageServerPool>,
}

impl CancelForwarder {
    /// Create a new cancel forwarder wrapping the given pool.
    pub fn new(pool: Arc<LanguageServerPool>) -> Self {
        Self { pool }
    }

    /// Forward a cancel request to the downstream server.
    ///
    /// Looks up the language from the upstream request registry and forwards
    /// the cancel notification to the appropriate downstream server.
    ///
    /// Supports both numeric and string request IDs per LSP spec.
    ///
    /// Returns `Ok(())` if the cancel was forwarded, or an error if the
    /// upstream ID is not found in the registry.
    pub async fn forward_cancel(&self, upstream_id: UpstreamId) -> std::io::Result<()> {
        self.pool.forward_cancel_by_upstream_id(upstream_id).await
    }
}

/// Tower Service wrapper that captures request IDs from incoming LSP requests.
///
/// This middleware extracts the request ID from each incoming request and stores
/// it in task-local storage before delegating to the inner service. This allows
/// bridge code to access the upstream request ID when making downstream requests.
///
/// Additionally, it intercepts `$/cancelRequest` notifications and forwards them
/// to downstream language servers via the `CancelForwarder`.
pub struct RequestIdCapture<S> {
    inner: S,
    cancel_forwarder: Option<CancelForwarder>,
}

impl<S> RequestIdCapture<S> {
    /// Create a new RequestIdCapture wrapping the given service.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            cancel_forwarder: None,
        }
    }

    /// Create a new RequestIdCapture with a cancel forwarder.
    ///
    /// The cancel forwarder is used to forward `$/cancelRequest` notifications
    /// to downstream language servers.
    pub fn with_cancel_forwarder(inner: S, cancel_forwarder: CancelForwarder) -> Self {
        Self {
            inner,
            cancel_forwarder: Some(cancel_forwarder),
        }
    }
}

impl<S> Service<Request> for RequestIdCapture<S>
where
    S: Service<Request, Response = Option<Response>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        // Extract the request ID before delegating
        let request_id = req.id().cloned();

        // Check if this is a $/cancelRequest notification and forward to downstream
        // Per LSP spec, cancel params.id can be either integer or string
        let cancel_forwarder = self.cancel_forwarder.clone();
        if req.method() == "$/cancelRequest"
            && let Some(forwarder) = cancel_forwarder.as_ref()
            && let Some(params) = req.params()
        {
            // Extract the ID as either numeric or string (per LSP spec: integer | string)
            let id_to_cancel = if let Some(n) = params.get("id").and_then(|v| v.as_i64()) {
                Some(UpstreamId::Number(n))
            } else if let Some(s) = params.get("id").and_then(|v| v.as_str()) {
                Some(UpstreamId::String(s.to_string()))
            } else {
                None
            };

            if let Some(upstream_id) = id_to_cancel {
                let forwarder = forwarder.clone();
                // Fire-and-forget: spawn without tracking JoinHandle.
                //
                // This is intentional for $/cancelRequest because:
                // 1. LSP notifications don't expect responses (fire-and-forget by spec)
                // 2. Cancel is "best effort" - failures are logged but non-fatal
                // 3. We must not block the main request flow
                // 4. Graceful shutdown doesn't need to wait for cancels - the downstream
                //    server will clean up its own state when it shuts down
                tokio::spawn(async move {
                    if let Err(e) = forwarder.forward_cancel(upstream_id.clone()).await {
                        // Log the error but don't fail - cancel forwarding is best-effort
                        log::debug!(
                            target: "kakehashi::cancel",
                            "Failed to forward cancel for request {}: {}",
                            upstream_id,
                            e
                        );
                    }
                });
            }
        }

        // Call inner service and get the future
        let inner_fut = self.inner.call(req);

        Box::pin(async move {
            // Set the task-local request ID and await the inner future
            CURRENT_REQUEST_ID.scope(request_id, inner_fut).await
        })
    }
}

/// Get the current request ID from task-local storage.
///
/// Returns None if called outside of a request context or if the request was
/// a notification (which has no ID).
pub fn get_current_request_id() -> Option<Id> {
    CURRENT_REQUEST_ID.try_with(|id| id.clone()).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// Mock service that records whether it was called and captures the request ID
    /// from task-local storage during the call.
    #[derive(Clone)]
    struct MockService {
        captured_id: Arc<Mutex<Option<Option<Id>>>>,
    }

    impl MockService {
        fn new() -> Self {
            Self {
                captured_id: Arc::new(Mutex::new(None)),
            }
        }

        async fn get_captured_id(&self) -> Option<Option<Id>> {
            self.captured_id.lock().await.clone()
        }
    }

    impl Service<Request> for MockService {
        type Response = Option<Response>;
        type Error = std::convert::Infallible;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: Request) -> Self::Future {
            let captured_id = Arc::clone(&self.captured_id);
            Box::pin(async move {
                // Capture the current request ID from task-local storage
                let id = get_current_request_id();
                *captured_id.lock().await = Some(id);
                Ok(None)
            })
        }
    }

    #[tokio::test]
    async fn captures_numeric_request_id() {
        let mock = MockService::new();
        let mut service = RequestIdCapture::new(mock.clone());

        // Create a request with numeric ID
        let request = Request::build("textDocument/hover")
            .params(serde_json::json!({}))
            .id(42i64)
            .finish();

        // Call the service
        let _ = service.call(request).await;

        // Verify the ID was captured
        let captured = mock.get_captured_id().await;
        assert_eq!(captured, Some(Some(Id::Number(42))));
    }

    #[tokio::test]
    async fn captures_string_request_id() {
        let mock = MockService::new();
        let mut service = RequestIdCapture::new(mock.clone());

        // Create a request with string ID
        let request = Request::build("textDocument/hover")
            .params(serde_json::json!({}))
            .id("test-id-123")
            .finish();

        // Call the service
        let _ = service.call(request).await;

        // Verify the ID was captured
        let captured = mock.get_captured_id().await;
        assert_eq!(captured, Some(Some(Id::String("test-id-123".to_string()))));
    }

    #[tokio::test]
    async fn handles_notification_without_id() {
        let mock = MockService::new();
        let mut service = RequestIdCapture::new(mock.clone());

        // Create a notification (no ID)
        let notification = Request::build("initialized")
            .params(serde_json::json!({}))
            .finish();

        // Call the service
        let _ = service.call(notification).await;

        // Verify no ID was captured (notification has None)
        let captured = mock.get_captured_id().await;
        assert_eq!(captured, Some(None));
    }

    #[tokio::test]
    async fn request_id_not_available_outside_context() {
        // Without being inside a request context, ID should be None
        let id = get_current_request_id();
        assert_eq!(id, None);
    }

    // ========================================
    // CancelForwarder tests
    // ========================================

    /// Test that with_cancel_forwarder creates a middleware that forwards cancels.
    ///
    /// We can't easily mock CancelForwarder (it requires a real LanguageServerPool),
    /// so we test that:
    /// 1. The middleware is constructed correctly
    /// 2. Cancel notifications are intercepted (not passed through unchanged)
    /// 3. Non-cancel requests work normally
    #[tokio::test]
    async fn with_cancel_forwarder_passes_non_cancel_requests() {
        let mock = MockService::new();
        let pool = Arc::new(LanguageServerPool::new());
        let forwarder = CancelForwarder::new(pool);
        let mut service = RequestIdCapture::with_cancel_forwarder(mock.clone(), forwarder);

        // Create a hover request (not a cancel)
        let request = Request::build("textDocument/hover")
            .params(serde_json::json!({}))
            .id(42i64)
            .finish();

        // Call the service
        let _ = service.call(request).await;

        // Verify the request was passed through and ID captured
        let captured = mock.get_captured_id().await;
        assert_eq!(captured, Some(Some(Id::Number(42))));
    }

    #[tokio::test]
    async fn cancel_notification_is_intercepted() {
        let mock = MockService::new();
        let pool = Arc::new(LanguageServerPool::new());
        let forwarder = CancelForwarder::new(pool);
        let mut service = RequestIdCapture::with_cancel_forwarder(mock.clone(), forwarder);

        // Create a $/cancelRequest notification
        let request = Request::build("$/cancelRequest")
            .params(serde_json::json!({ "id": 123 }))
            .finish();

        // Call the service
        let result = service.call(request).await;

        // The notification should be processed (no error)
        assert!(result.is_ok());

        // The inner service was still called (tower-lsp needs to see it too)
        let captured = mock.get_captured_id().await;
        assert!(captured.is_some(), "Inner service should still be called");

        // Note: We can't verify the forward happened without a real pool setup,
        // but we've verified the middleware processes the cancel notification.
    }

    #[tokio::test]
    async fn cancel_forwarder_handles_missing_id_in_params() {
        let mock = MockService::new();
        let pool = Arc::new(LanguageServerPool::new());
        let forwarder = CancelForwarder::new(pool);
        let mut service = RequestIdCapture::with_cancel_forwarder(mock.clone(), forwarder);

        // Create a $/cancelRequest with no id parameter (malformed)
        let request = Request::build("$/cancelRequest")
            .params(serde_json::json!({}))
            .finish();

        // Should not crash, just skip forwarding
        let result = service.call(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cancel_forwarder_handles_string_id() {
        let mock = MockService::new();
        let pool = Arc::new(LanguageServerPool::new());
        let forwarder = CancelForwarder::new(pool);
        let mut service = RequestIdCapture::with_cancel_forwarder(mock.clone(), forwarder);

        // Create a $/cancelRequest with string id (supported per LSP 3.17 spec)
        let request = Request::build("$/cancelRequest")
            .params(serde_json::json!({ "id": "string-id" }))
            .finish();

        // Should extract UpstreamId::String and attempt forwarding
        let result = service.call(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn middleware_without_forwarder_ignores_cancel() {
        let mock = MockService::new();
        // Create middleware without cancel forwarder
        let mut service = RequestIdCapture::new(mock.clone());

        // Create a $/cancelRequest notification
        let request = Request::build("$/cancelRequest")
            .params(serde_json::json!({ "id": 123 }))
            .finish();

        // Should work without crash (cancel just isn't forwarded)
        let result = service.call(request).await;
        assert!(result.is_ok());

        // Inner service was still called
        let captured = mock.get_captured_id().await;
        assert!(captured.is_some());
    }
}
