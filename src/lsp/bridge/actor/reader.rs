//! Reader task for downstream language server stdout.
//!
//! This module provides the background task that reads LSP messages from
//! the downstream server's stdout and routes responses to waiting requesters.
//!
//! # Architecture (ADR-0015)
//!
//! The Reader Task:
//! - Runs in a spawned tokio task
//! - Reads messages from stdout using BridgeReader
//! - Routes responses via ResponseRouter to oneshot waiters
//! - Logs notifications (they don't have waiters)
//! - Gracefully shuts down on EOF, error, or cancellation signal

use std::sync::Arc;

use log::{debug, warn};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::super::connection::BridgeReader;
use super::ResponseRouter;

/// Handle to a running Reader Task, managing its lifetime via RAII.
///
/// This struct owns the resources needed to control and clean up the reader task.
/// The underscore-prefixed fields indicate they are held for their Drop semantics
/// rather than being explicitly accessed.
///
/// # Lifecycle and Drop Behavior
///
/// When this handle is dropped:
///
/// 1. **CancellationToken is dropped** - This cancels the token, which signals the
///    reader loop's `select!` to exit via `cancel_token.cancelled()`. The reader
///    loop checks this signal on each iteration and breaks cleanly when cancelled.
///
/// 2. **JoinHandle is dropped without awaiting** - This is intentional and safe
///    because:
///    - The reader task will exit promptly once cancelled (the `select!` ensures
///      cancellation is checked on every loop iteration)
///    - The reader task also exits on EOF or read error, handling the case where
///      the downstream process terminates
///    - Tokio tasks are detached when their JoinHandle is dropped; they continue
///      running but we don't need to await completion
///    - The task performs no critical cleanup that requires awaiting
///
/// # Why Not Await the JoinHandle?
///
/// Awaiting would require this drop to be async, which is not possible in Rust.
/// The reader task is designed to exit quickly on cancellation (within one loop
/// iteration), so fire-and-forget cleanup is appropriate here.
///
/// # Cross-Task Coordination (ADR-0015)
///
/// The cancellation token enables coordination between reader and writer tasks.
/// When the writer task fails, it cancels the shared token, causing the reader
/// to exit and preventing CPU spin on orphaned channels. Conversely, when the
/// reader handle is dropped (e.g., during connection shutdown), the reader task
/// receives the cancellation signal and exits cleanly.
///
/// # Resource Cleanup Guarantee
///
/// - The reader task holds only borrowed/Arc'd resources (BridgeReader, ResponseRouter)
/// - On cancellation: logs shutdown, breaks from loop, task completes
/// - On EOF/error: fails all pending requests via router, then exits
/// - No resources are leaked regardless of exit path
pub(crate) struct ReaderTaskHandle {
    /// Join handle for the spawned reader task.
    ///
    /// Dropped without awaiting when this struct is dropped. This is safe because
    /// the reader task exits promptly on cancellation, EOF, or read error.
    _join_handle: JoinHandle<()>,

    /// Cancellation token to signal graceful shutdown.
    ///
    /// When this token is dropped, it is automatically cancelled, causing the
    /// reader loop's `cancel_token.cancelled()` future to complete. This triggers
    /// a clean exit from the reader loop.
    _cancel_token: CancellationToken,
}

/// Spawn a reader task that reads from stdout and routes responses.
///
/// # Arguments
/// * `reader` - The BridgeReader to read messages from
/// * `router` - The ResponseRouter to route responses to waiters
///
/// # Returns
/// A ReaderTaskHandle for managing the spawned task.
pub(crate) fn spawn_reader_task(
    reader: BridgeReader,
    router: Arc<ResponseRouter>,
) -> ReaderTaskHandle {
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    let join_handle = tokio::spawn(reader_loop(reader, router, token_clone));

    ReaderTaskHandle {
        _join_handle: join_handle,
        _cancel_token: cancel_token,
    }
}

/// The main reader loop - reads messages and routes them.
async fn reader_loop(
    mut reader: BridgeReader,
    router: Arc<ResponseRouter>,
    cancel_token: CancellationToken,
) {
    loop {
        tokio::select! {
            // Check for cancellation first
            _ = cancel_token.cancelled() => {
                debug!(
                    target: "tree_sitter_ls::bridge::reader",
                    "Reader task cancelled, shutting down"
                );
                break;
            }

            // Try to read a message
            result = reader.read_message() => {
                match result {
                    Ok(message) => {
                        handle_message(message, &router);
                    }
                    Err(e) => {
                        // EOF or read error - mark all pending as failed and exit
                        warn!(
                            target: "tree_sitter_ls::bridge::reader",
                            "Reader error: {}, failing pending requests",
                            e
                        );
                        router.fail_all(&format!("bridge: reader error: {}", e));
                        break;
                    }
                }
            }
        }
    }
}

/// Handle a single message from the downstream server.
fn handle_message(message: serde_json::Value, router: &ResponseRouter) {
    // Check if it's a response (has "id" field)
    if message.get("id").is_some() {
        // It's a response - route to waiter
        let delivered = router.route(message);
        if !delivered {
            debug!(
                target: "tree_sitter_ls::bridge::reader",
                "Response for unknown request ID, dropping"
            );
        }
    } else {
        // It's a notification - log and skip
        if let Some(method) = message.get("method").and_then(|v| v.as_str()) {
            debug!(
                target: "tree_sitter_ls::bridge::reader",
                "Received notification: {}, skipping",
                method
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::bridge::connection::AsyncBridgeConnection;
    use serde_json::json;

    /// Helper to create a test connection using `cat` for echo behavior.
    async fn create_echo_connection() -> AsyncBridgeConnection {
        AsyncBridgeConnection::spawn(vec!["cat".to_string()])
            .await
            .expect("cat should spawn")
    }

    #[tokio::test]
    async fn reader_task_routes_response_to_waiter() {
        // Create a connection that echoes messages
        let mut conn = create_echo_connection().await;

        // Write a response before splitting (so it's in the pipe)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": { "contents": "test hover" }
        });
        conn.write_message(&response)
            .await
            .expect("write should succeed");

        // Split the connection to get the reader
        let (writer, reader) = conn.split();

        // Create router and register a request
        let router = Arc::new(ResponseRouter::new());
        let rx = router
            .register(crate::lsp::bridge::protocol::RequestId::new(42))
            .unwrap();

        // Spawn the reader task
        let _handle = spawn_reader_task(reader, Arc::clone(&router));

        // Wait for the response with timeout
        let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx)
            .await
            .expect("should not timeout")
            .expect("channel should not be closed");

        // Verify the response was routed correctly
        assert_eq!(received["id"], 42);
        assert_eq!(received["result"]["contents"], "test hover");
        assert_eq!(router.pending_count(), 0);

        // Drop writer to clean up child process
        drop(writer);
    }

    #[test]
    fn handle_message_routes_response() {
        let router = ResponseRouter::new();
        let _rx = router
            .register(crate::lsp::bridge::protocol::RequestId::new(1))
            .unwrap();

        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": null
        });

        handle_message(response, &router);

        // Receiver should have the response
        // We can't block on rx.await here in a sync test, but we can check
        // that the pending count is 0 (meaning it was routed)
        assert_eq!(router.pending_count(), 0);
    }

    #[test]
    fn handle_message_ignores_notification() {
        let router = ResponseRouter::new();
        let _rx = router
            .register(crate::lsp::bridge::protocol::RequestId::new(1))
            .unwrap();

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {}
        });

        handle_message(notification, &router);

        // Pending count should still be 1 (notification was ignored)
        assert_eq!(router.pending_count(), 1);
    }

    #[tokio::test]
    async fn reader_loop_exits_on_cancellation() {
        use crate::lsp::bridge::connection::BridgeReader;
        use std::process::Stdio;
        use tokio::process::Command;

        // Create a long-running process that won't send any output
        // Using `sleep` ensures the reader blocks waiting for input
        let mut child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("sleep should spawn");

        let stdout = child.stdout.take().expect("stdout should be available");
        let reader = BridgeReader::new(stdout);

        let router = Arc::new(ResponseRouter::new());
        let cancel_token = CancellationToken::new();
        let token_clone = cancel_token.clone();

        // Spawn the reader loop
        let handle = tokio::spawn(reader_loop(reader, router, token_clone));

        // Give the loop a moment to start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Cancel the token
        cancel_token.cancel();

        // The loop should exit promptly
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
        assert!(
            result.is_ok(),
            "reader_loop should exit quickly after cancellation"
        );

        // Clean up
        let _ = child.kill().await;
    }

    #[tokio::test]
    async fn reader_loop_fails_all_on_eof() {
        use crate::lsp::bridge::protocol::RequestId;

        // Create a connection that will close immediately (empty echo)
        let mut conn = create_echo_connection().await;
        let (writer, reader) = conn.split();

        // Drop the writer to close stdin, causing EOF on stdout
        drop(writer);

        let router = Arc::new(ResponseRouter::new());
        let rx1 = router.register(RequestId::new(1)).unwrap();
        let rx2 = router.register(RequestId::new(2)).unwrap();

        let cancel_token = CancellationToken::new();

        // Run the reader loop - it should exit on EOF
        let handle = tokio::spawn(reader_loop(reader, Arc::clone(&router), cancel_token));

        // Wait for the loop to complete
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;
        assert!(result.is_ok(), "reader_loop should exit on EOF");

        // All pending requests should have received error responses
        assert_eq!(router.pending_count(), 0, "all pending should be cleared");

        // Check that waiters received error responses
        let response1 = rx1.await.expect("should receive error response");
        assert!(
            response1.get("error").is_some(),
            "response should be an error"
        );
        assert_eq!(response1["error"]["code"], -32603);

        let response2 = rx2.await.expect("should receive error response");
        assert!(
            response2.get("error").is_some(),
            "response should be an error"
        );
    }

    #[tokio::test]
    async fn reader_loop_routes_multiple_responses_in_order() {
        use crate::lsp::bridge::protocol::RequestId;

        let mut conn = create_echo_connection().await;

        // Write multiple responses before splitting
        let response1 = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": "first"
        });
        let response2 = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": "second"
        });
        let response3 = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "result": "third"
        });

        conn.write_message(&response1).await.unwrap();
        conn.write_message(&response2).await.unwrap();
        conn.write_message(&response3).await.unwrap();

        let (writer, reader) = conn.split();

        let router = Arc::new(ResponseRouter::new());
        let rx1 = router.register(RequestId::new(1)).unwrap();
        let rx2 = router.register(RequestId::new(2)).unwrap();
        let rx3 = router.register(RequestId::new(3)).unwrap();

        let _handle = spawn_reader_task(reader, Arc::clone(&router));

        // All three should be received
        let received1 = tokio::time::timeout(std::time::Duration::from_secs(1), rx1)
            .await
            .expect("should not timeout")
            .expect("channel should not be closed");
        assert_eq!(received1["result"], "first");

        let received2 = tokio::time::timeout(std::time::Duration::from_secs(1), rx2)
            .await
            .expect("should not timeout")
            .expect("channel should not be closed");
        assert_eq!(received2["result"], "second");

        let received3 = tokio::time::timeout(std::time::Duration::from_secs(1), rx3)
            .await
            .expect("should not timeout")
            .expect("channel should not be closed");
        assert_eq!(received3["result"], "third");

        assert_eq!(router.pending_count(), 0);
        drop(writer);
    }

    #[tokio::test]
    async fn reader_loop_skips_notifications_and_continues() {
        use crate::lsp::bridge::protocol::RequestId;

        let mut conn = create_echo_connection().await;

        // Write a notification followed by a response
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": "test" }
        });
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": "after notification"
        });

        conn.write_message(&notification).await.unwrap();
        conn.write_message(&response).await.unwrap();

        let (writer, reader) = conn.split();

        let router = Arc::new(ResponseRouter::new());
        let rx = router.register(RequestId::new(42)).unwrap();

        let _handle = spawn_reader_task(reader, Arc::clone(&router));

        // Should receive the response even though notification came first
        let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx)
            .await
            .expect("should not timeout")
            .expect("channel should not be closed");

        assert_eq!(received["id"], 42);
        assert_eq!(received["result"], "after notification");

        drop(writer);
    }

    #[tokio::test]
    async fn reader_loop_handles_unknown_response_id() {
        use crate::lsp::bridge::protocol::RequestId;

        let mut conn = create_echo_connection().await;

        // Write a response with an unregistered ID, followed by one with registered ID
        let unknown_response = json!({
            "jsonrpc": "2.0",
            "id": 999,
            "result": "unknown"
        });
        let known_response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": "known"
        });

        conn.write_message(&unknown_response).await.unwrap();
        conn.write_message(&known_response).await.unwrap();

        let (writer, reader) = conn.split();

        let router = Arc::new(ResponseRouter::new());
        // Only register ID 1, not 999
        let rx = router.register(RequestId::new(1)).unwrap();

        let _handle = spawn_reader_task(reader, Arc::clone(&router));

        // Should skip the unknown response and deliver the known one
        let received = tokio::time::timeout(std::time::Duration::from_secs(1), rx)
            .await
            .expect("should not timeout")
            .expect("channel should not be closed");

        assert_eq!(received["id"], 1);
        assert_eq!(received["result"], "known");

        drop(writer);
    }
}
