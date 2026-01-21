//! Request ID capture for bridging upstream request IDs to downstream servers.
//!
//! This module provides a tower Service wrapper that captures the request ID
//! from incoming LSP requests and makes it available via task-local storage.
//! This enables downstream bridge requests to use the same ID as the upstream
//! client per ADR-0016 (Server Pool Coordination).

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tower::Service;
use tower_lsp_server::jsonrpc::{Id, Request, Response};

tokio::task_local! {
    /// Task-local storage for the current upstream request ID.
    ///
    /// This is set by RequestIdCapture before delegating to the inner service,
    /// allowing downstream bridge code to access the original request ID.
    pub static CURRENT_REQUEST_ID: Option<Id>;
}

/// Tower Service wrapper that captures request IDs from incoming LSP requests.
///
/// This middleware extracts the request ID from each incoming request and stores
/// it in task-local storage before delegating to the inner service. This allows
/// bridge code to access the upstream request ID when making downstream requests.
pub struct RequestIdCapture<S> {
    inner: S,
}

impl<S> RequestIdCapture<S> {
    /// Create a new RequestIdCapture wrapping the given service.
    pub fn new(inner: S) -> Self {
        Self { inner }
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
}
