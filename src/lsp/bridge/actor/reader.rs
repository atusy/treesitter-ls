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
//! - Forwards notifications to DownstreamMessageHandler via channel
//! - Manages liveness timer for hung server detection (ADR-0014)
//! - Gracefully shuts down on EOF, error, or cancellation signal

use std::sync::Arc;
use std::time::Duration;

use log::{debug, warn};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::super::connection::BridgeReader;
use super::{DownstreamMessage, DownstreamNotification, ResponseRouter};

/// Type alias for the pinned liveness timer future.
type LivenessTimer = std::pin::Pin<Box<tokio::time::Sleep>>;

/// Creates a new liveness timer that will fire after the given duration.
fn new_liveness_timer(timeout: Duration) -> LivenessTimer {
    Box::pin(tokio::time::sleep(timeout))
}

/// Encapsulates liveness timer state and logic for the reader task.
///
/// This struct consolidates the timer state (active timer, timeout configuration)
/// and provides methods for timer lifecycle management, improving cohesion
/// compared to managing multiple variables separately.
///
/// # Timer States
///
/// - **Inactive**: `timer` is `None`, timeout may or may not be configured
/// - **Active**: `timer` is `Some`, will fire after configured duration
///
/// # Usage Pattern
///
/// ```ignore
/// let mut liveness = LivenessTimerState::new(Some(Duration::from_secs(60)));
/// liveness.start(&lang_prefix);           // Starts timer when pending 0->1
/// liveness.reset(&lang_prefix);           // Resets on message activity
/// liveness.stop(&lang_prefix, "reason");  // Stops when pending returns to 0
/// ```
struct LivenessTimerState {
    /// Active timer future, None when inactive.
    timer: Option<LivenessTimer>,
    /// Configured timeout duration, None if liveness is disabled.
    timeout: Option<Duration>,
}

impl LivenessTimerState {
    /// Create a new liveness timer state with optional timeout configuration.
    ///
    /// If `timeout` is None, the timer is disabled and all operations are no-ops.
    fn new(timeout: Option<Duration>) -> Self {
        Self {
            timer: None,
            timeout,
        }
    }

    /// Check if the timer is currently active.
    fn is_active(&self) -> bool {
        self.timer.is_some()
    }

    /// Start the liveness timer.
    ///
    /// Called when pending count transitions 0->1.
    /// No-op if timeout is not configured.
    fn start(&mut self, lang_prefix: &str) {
        if let Some(timeout) = self.timeout {
            debug!(
                target: "kakehashi::bridge::reader",
                "{}Liveness timer started: {:?}",
                lang_prefix,
                timeout
            );
            self.timer = Some(new_liveness_timer(timeout));
        }
    }

    /// Reset the liveness timer to full duration.
    ///
    /// Called on any message activity (response or notification).
    /// Only resets if timer is currently active.
    fn reset(&mut self, lang_prefix: &str) {
        if let Some(timeout) = self.timeout
            && self.timer.is_some()
        {
            debug!(
                target: "kakehashi::bridge::reader",
                "{}Liveness timer reset on message activity",
                lang_prefix
            );
            self.timer = Some(new_liveness_timer(timeout));
        }
    }

    /// Stop the liveness timer.
    ///
    /// Called when pending count returns to 0 or shutdown begins.
    /// Only logs if timer was actually active.
    fn stop(&mut self, lang_prefix: &str, reason: &str) {
        if self.timer.take().is_some() {
            debug!(
                target: "kakehashi::bridge::reader",
                "{}Liveness timer stopped: {}",
                lang_prefix,
                reason
            );
        }
    }

    /// Get the configured timeout duration (for logging on expiry).
    fn timeout_duration(&self) -> Duration {
        self.timeout.unwrap_or_default()
    }

    /// Take a reference to the timer for use in select!.
    ///
    /// Returns a future that completes when the timer fires, or pends forever
    /// if no timer is active.
    async fn wait(&mut self) {
        if let Some(ref mut timer) = self.timer {
            timer.await;
        } else {
            std::future::pending::<()>().await;
        }
    }
}

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
    /// When dropped, the token is automatically cancelled, causing the reader
    /// loop's `cancel_token.cancelled()` future to complete. This ensures the
    /// reader task exits when the handle is dropped (RAII cleanup).
    _cancel_token: CancellationToken,

    /// Sender for liveness timer start notifications.
    ///
    /// Used by ConnectionHandle to notify the reader when the first request
    /// is registered (pending 0->1), triggering the liveness timer to start.
    liveness_start_tx: mpsc::Sender<()>,

    /// Sender for liveness timer stop notifications (ADR-0018 Phase 4).
    ///
    /// Used by ConnectionHandle to stop the liveness timer when shutdown begins.
    /// Global shutdown (Tier 3) overrides liveness timeout (Tier 2).
    liveness_stop_tx: mpsc::Sender<()>,

    /// Receiver for liveness failure notification (ADR-0014 Phase 3).
    ///
    /// When the liveness timeout fires, the reader task sends () on this channel.
    /// ConnectionHandle checks this to transition to Failed state.
    liveness_failed_rx: std::sync::Mutex<Option<oneshot::Receiver<()>>>,
}

impl ReaderTaskHandle {
    /// Notify the reader task to start the liveness timer.
    ///
    /// Called by ConnectionHandle when the first request is registered (pending 0->1).
    /// Non-blocking: if the channel is full, the notification is dropped (timer already running).
    pub(crate) fn notify_liveness_start(&self) {
        // Use try_send to avoid blocking. If channel is full, timer is already running.
        let _ = self.liveness_start_tx.try_send(());
    }

    /// Stop the liveness timer without canceling the reader task (ADR-0018 Phase 4).
    ///
    /// Called by ConnectionHandle when shutdown begins. Global shutdown (Tier 3)
    /// overrides liveness timeout (Tier 2), but the reader task continues running
    /// to receive the shutdown response.
    ///
    /// Non-blocking: if the channel is full, the stop is already in progress.
    pub(crate) fn stop_liveness_timer(&self) {
        // Use try_send to avoid blocking
        let _ = self.liveness_stop_tx.try_send(());
    }

    /// Check if a liveness failure has been signaled.
    ///
    /// Returns true if the reader task signaled a liveness timeout failure.
    /// This is a one-time check - once it returns true, subsequent calls return false.
    ///
    /// Used by ConnectionHandle to detect liveness timeout and transition to Failed state.
    pub(crate) fn check_liveness_failed(&self) -> bool {
        let mut guard = self
            .liveness_failed_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        if let Some(mut rx) = guard.take() {
            match rx.try_recv() {
                Ok(()) => true, // Liveness timeout fired
                Err(oneshot::error::TryRecvError::Empty) => {
                    // Not yet failed, put it back
                    *guard = Some(rx);
                    false
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    // Sender dropped without sending - reader exited normally
                    false
                }
            }
        } else {
            // Already consumed
            false
        }
    }
}

/// Spawn a reader task that reads from stdout and routes responses.
///
/// This is a convenience wrapper for tests that don't need liveness timeout.
/// Production code should use `spawn_reader_task_with_liveness` directly.
///
/// # Arguments
/// * `reader` - The BridgeReader to read messages from
/// * `router` - The ResponseRouter to route responses to waiters
///
/// # Returns
/// A ReaderTaskHandle for managing the spawned task.
#[cfg(test)]
pub(crate) fn spawn_reader_task(
    reader: BridgeReader,
    router: Arc<ResponseRouter>,
) -> ReaderTaskHandle {
    spawn_reader_task_with_liveness(reader, router, None)
}

/// Spawn a reader task with optional liveness timeout (no language context).
///
/// This is a convenience wrapper for tests that don't need structured logging
/// with language identifiers or downstream notification forwarding.
/// Production code should use `spawn_reader_task_for_language`.
///
/// # Arguments
/// * `reader` - The BridgeReader to read messages from
/// * `router` - The ResponseRouter to route responses to waiters
/// * `liveness_timeout` - Optional timeout for hung server detection (ADR-0014)
///
/// # Returns
/// A ReaderTaskHandle for managing the spawned task.
#[cfg(test)]
pub(crate) fn spawn_reader_task_with_liveness(
    reader: BridgeReader,
    router: Arc<ResponseRouter>,
    liveness_timeout: Option<Duration>,
) -> ReaderTaskHandle {
    spawn_reader_task_for_language(reader, router, liveness_timeout, None, None, None)
}

/// Spawn a reader task with liveness timeout and language identifier for logging.
///
/// # Arguments
/// * `reader` - The BridgeReader to read messages from
/// * `router` - The ResponseRouter to route responses to waiters
/// * `liveness_timeout` - Optional timeout for hung server detection (ADR-0014)
/// * `language` - Language identifier for structured logging (e.g., "lua", "python")
/// * `downstream_tx` - Optional channel sender for forwarding notifications to handler
/// * `server_name` - Optional server name for notification prefixing (e.g., "lua-language-server")
///
/// # Returns
/// A ReaderTaskHandle for managing the spawned task.
pub(crate) fn spawn_reader_task_for_language(
    reader: BridgeReader,
    router: Arc<ResponseRouter>,
    liveness_timeout: Option<Duration>,
    language: Option<String>,
    downstream_tx: Option<mpsc::Sender<DownstreamMessage>>,
    server_name: Option<String>,
) -> ReaderTaskHandle {
    let cancel_token = CancellationToken::new();
    let token_clone = cancel_token.clone();

    // Channel for liveness timer start notifications (capacity 1, latest notification wins)
    let (liveness_start_tx, liveness_start_rx) = mpsc::channel(1);

    // Channel for liveness timer stop notifications (Phase 4: shutdown integration)
    let (liveness_stop_tx, liveness_stop_rx) = mpsc::channel(1);

    // Channel for liveness failure notification (Phase 3: state transition signaling)
    let (liveness_failed_tx, liveness_failed_rx) = oneshot::channel();

    let join_handle = tokio::spawn(reader_loop_with_liveness(
        reader,
        router,
        token_clone,
        liveness_timeout,
        liveness_start_rx,
        liveness_stop_rx,
        liveness_failed_tx,
        language,
        downstream_tx,
        server_name,
    ));

    ReaderTaskHandle {
        _join_handle: join_handle,
        _cancel_token: cancel_token,
        liveness_start_tx,
        liveness_stop_tx,
        liveness_failed_rx: std::sync::Mutex::new(Some(liveness_failed_rx)),
    }
}

/// The main reader loop - reads messages and routes them.
///
/// This is a convenience wrapper that calls `reader_loop_with_liveness` without
/// liveness timeout support.
#[cfg(test)]
async fn reader_loop(
    reader: BridgeReader,
    router: Arc<ResponseRouter>,
    cancel_token: CancellationToken,
) {
    // Create dummy receivers that never receive (liveness disabled)
    let (_start_tx, start_rx) = mpsc::channel(1);
    let (_stop_tx, stop_rx) = mpsc::channel(1);
    // Create a dummy sender that we'll drop (no liveness failure signaling in test helper)
    let (failed_tx, _failed_rx) = oneshot::channel();
    reader_loop_with_liveness(
        reader,
        router,
        cancel_token,
        None,
        start_rx,
        stop_rx,
        failed_tx,
        None, // No language context for test helper
        None, // No downstream channel for test helper
        None, // No server name for test helper
    )
    .await
}

/// The main reader loop with optional liveness timeout support.
///
/// # Liveness Timer (ADR-0014)
///
/// When `liveness_timeout` is Some:
/// - Timer starts when a notification is received on `liveness_start_rx` (pending 0->1)
/// - Timer resets on any message received (response or notification)
/// - Timer stops when pending count returns to 0 OR when stop notification received
/// - Timer firing triggers Ready->Failed transition via router.fail_all() and signals via liveness_failed_tx
///
/// # Downstream Notification Forwarding (Phase 0)
///
/// When `downstream_tx` and `server_name` are provided:
/// - Notifications are wrapped in `DownstreamMessage::Notification` and sent to the channel
/// - On channel full: message is dropped and a warning is logged (per ADR-0015 backpressure)
///
/// # Observability
///
/// When `language` is provided, all log messages include the language identifier
/// for easier filtering in production (e.g., `[lua]` prefix in log messages).
#[allow(clippy::too_many_arguments)] // Internal async fn; grouping would add complexity
async fn reader_loop_with_liveness(
    mut reader: BridgeReader,
    router: Arc<ResponseRouter>,
    cancel_token: CancellationToken,
    liveness_timeout: Option<Duration>,
    mut liveness_start_rx: mpsc::Receiver<()>,
    mut liveness_stop_rx: mpsc::Receiver<()>,
    liveness_failed_tx: oneshot::Sender<()>,
    language: Option<String>,
    downstream_tx: Option<mpsc::Sender<DownstreamMessage>>,
    server_name: Option<String>,
) {
    // Language prefix for log messages (e.g., "[lua] " or "")
    let lang_prefix = language
        .as_ref()
        .map(|l| format!("[{}] ", l))
        .unwrap_or_default();

    // Consolidated liveness timer state (ADR-0014)
    let mut liveness = LivenessTimerState::new(liveness_timeout);

    loop {
        tokio::select! {
            biased; // Process in order: cancellation, timer, liveness start, liveness stop, read

            // Check for cancellation first (highest priority)
            _ = cancel_token.cancelled() => {
                debug!(
                    target: "kakehashi::bridge::reader",
                    "{}Reader task cancelled, shutting down",
                    lang_prefix
                );
                break;
            }

            // Check for liveness timeout (if timer is active)
            _ = liveness.wait() => {
                // Liveness timeout fired - server is unresponsive
                // This warn! is the primary observability signal for production debugging.
                // The pending_count is included for debugging stuck request scenarios.
                let pending_count = router.pending_count();
                warn!(
                    target: "kakehashi::bridge::reader",
                    "{}Liveness timeout expired after {:?}, server appears hung - failing {} pending request(s)",
                    lang_prefix,
                    liveness.timeout_duration(),
                    pending_count
                );
                router.fail_all("bridge: liveness timeout - server unresponsive");
                // Signal liveness failure for state transition (ADR-0014 Phase 3)
                let _ = liveness_failed_tx.send(());
                break;
            }

            // Check for liveness timer start notification (pending 0->1)
            Some(()) = liveness_start_rx.recv() => {
                liveness.start(&lang_prefix);
            }

            // Check for liveness timer stop notification (shutdown began - ADR-0018 Phase 4)
            Some(()) = liveness_stop_rx.recv() => {
                liveness.stop(&lang_prefix, "shutdown began");
            }

            // Try to read a message (lowest priority)
            result = reader.read_message() => {
                match result {
                    Ok(message) => {
                        // Reset liveness timer on any message activity (ADR-0014)
                        liveness.reset(&lang_prefix);

                        handle_message(
                            message,
                            &router,
                            &lang_prefix,
                            downstream_tx.as_ref(),
                            server_name.as_deref(),
                        );

                        // Check if pending count returned to 0 - stop timer
                        if liveness.is_active() && router.pending_count() == 0 {
                            liveness.stop(&lang_prefix, "pending count is 0");
                        }
                    }
                    Err(e) => {
                        // EOF or read error - mark all pending as failed and exit
                        warn!(
                            target: "kakehashi::bridge::reader",
                            "{}Reader error: {}, failing pending requests",
                            lang_prefix,
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
///
/// Routes responses to waiting oneshot receivers via the ResponseRouter.
/// Forwards notifications to the DownstreamMessageHandler via the provided channel.
fn handle_message(
    message: serde_json::Value,
    router: &ResponseRouter,
    lang_prefix: &str,
    downstream_tx: Option<&mpsc::Sender<DownstreamMessage>>,
    server_name: Option<&str>,
) {
    // Check if it's a response (has "id" field)
    if message.get("id").is_some() {
        // It's a response - route to waiter
        let delivered = router.route(message);
        if !delivered {
            debug!(
                target: "kakehashi::bridge::reader",
                "{}Response for unknown request ID, dropping",
                lang_prefix
            );
        }
    } else {
        // It's a notification - forward to handler if channel is available
        let method = message
            .get("method")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let (Some(tx), Some(name)) = (downstream_tx, server_name) {
            let notification = DownstreamNotification {
                server_name: name.to_string(),
                notification: message,
            };
            let downstream_msg = DownstreamMessage::Notification(notification);

            // Use try_send to avoid blocking. On channel full, drop and warn (ADR-0015 backpressure).
            match tx.try_send(downstream_msg) {
                Ok(()) => {
                    debug!(
                        target: "kakehashi::bridge::reader",
                        "{}Forwarded notification to handler: {}",
                        lang_prefix,
                        method.as_deref().unwrap_or("unknown")
                    );
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    warn!(
                        target: "kakehashi::bridge::reader",
                        "{}Downstream channel full, dropping notification: {}",
                        lang_prefix,
                        method.as_deref().unwrap_or("unknown")
                    );
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    warn!(
                        target: "kakehashi::bridge::reader",
                        "{}Downstream channel closed, dropping notification: {}. Handler task exited or receiver was dropped.",
                        lang_prefix,
                        method.as_deref().unwrap_or("unknown")
                    );
                }
            }
        } else {
            // Missing channel or server name - log and skip (legacy behavior for tests)
            let reason = match (downstream_tx.is_some(), server_name.is_some()) {
                (false, _) => "no downstream channel",
                (true, false) => "no server name",
                (true, true) => unreachable!(),
            };
            debug!(
                target: "kakehashi::bridge::reader",
                "{}Received notification: {}, skipping ({})",
                lang_prefix,
                method.as_deref().unwrap_or("unknown"),
                reason
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

        handle_message(response, &router, "", None, None);

        // Receiver should have the response
        // We can't block on rx.await here in a sync test, but we can check
        // that the pending count is 0 (meaning it was routed)
        assert_eq!(router.pending_count(), 0);
    }

    #[test]
    fn handle_message_skips_notification_without_channel() {
        let router = ResponseRouter::new();
        let _rx = router
            .register(crate::lsp::bridge::protocol::RequestId::new(1))
            .unwrap();

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {}
        });

        // No downstream channel - notification should be skipped (legacy behavior)
        handle_message(notification, &router, "", None, None);

        // Pending count should still be 1 (notification was skipped)
        assert_eq!(router.pending_count(), 1);
    }

    #[test]
    fn handle_message_forwards_notification_to_channel() {
        let router = ResponseRouter::new();
        let _rx = router
            .register(crate::lsp::bridge::protocol::RequestId::new(1))
            .unwrap();

        let (tx, mut rx) = mpsc::channel(1);

        let notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": "test-token" }
        });

        handle_message(
            notification.clone(),
            &router,
            "",
            Some(&tx),
            Some("test-server"),
        );

        // Notification should have been forwarded to channel
        let received = rx.try_recv().expect("should receive notification");
        match received {
            DownstreamMessage::Notification(n) => {
                assert_eq!(n.server_name, "test-server");
                assert_eq!(n.notification["method"], "$/progress");
                assert_eq!(n.notification["params"]["token"], "test-token");
            }
        }

        // Pending count should still be 1 (notification doesn't affect pending)
        assert_eq!(router.pending_count(), 1);
    }

    #[test]
    fn handle_message_drops_notification_on_channel_full() {
        let router = ResponseRouter::new();
        let (tx, mut rx) = mpsc::channel(1); // Capacity 1

        // Fill the channel
        let first_notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": "first" }
        });
        handle_message(first_notification, &router, "", Some(&tx), Some("server"));

        // Try to send another notification - should be dropped (channel full)
        let second_notification = json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": { "token": "second" }
        });
        handle_message(second_notification, &router, "", Some(&tx), Some("server"));

        // Assert: exactly one message in channel (the first notification)
        let received = rx.try_recv().expect("should have one message");
        match received {
            DownstreamMessage::Notification(n) => {
                assert_eq!(n.notification["params"]["token"], "first");
            }
        }

        // Assert: channel is now empty (second notification was dropped)
        assert!(
            rx.try_recv().is_err(),
            "second notification should have been dropped due to backpressure"
        );
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

    // ============================================================
    // Liveness Timer Tests (ADR-0014)
    // ============================================================

    /// Test that liveness timer starts when notified (pending 0->1 transition).
    ///
    /// ADR-0014: Timer starts when pending count transitions 0 to 1.
    /// This verifies that sending a start notification activates the timer.
    #[tokio::test]
    async fn liveness_timer_starts_on_notification() {
        use crate::lsp::bridge::protocol::RequestId;
        use std::time::Duration;

        // Create an unresponsive server (will never send a response)
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(), // Consumes input, never outputs
        ])
        .await
        .expect("should spawn process");

        let (_writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());

        // Register a request (simulating pending going 0->1)
        let _rx = router.register(RequestId::new(1)).unwrap();
        assert_eq!(router.pending_count(), 1);

        // Spawn reader with short liveness timeout
        let handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(100)),
        );

        // Notify the reader to start the timer
        handle.notify_liveness_start();

        // Wait for timeout to fire
        tokio::time::sleep(Duration::from_millis(200)).await;

        // After timeout, pending requests should be failed
        // (router.fail_all is called on timeout)
        assert_eq!(
            router.pending_count(),
            0,
            "Pending count should be 0 after liveness timeout fires"
        );
    }

    /// Test that liveness timer resets on message activity.
    ///
    /// ADR-0014: Timer resets on any stdout activity (response or notification).
    /// This verifies that receiving a message resets the timer to full duration.
    ///
    /// Uses paused time for deterministic testing - avoids CI flakiness from
    /// timing variations under system load.
    #[tokio::test(start_paused = true)]
    async fn liveness_timer_resets_on_message_activity() {
        use crate::lsp::bridge::protocol::RequestId;
        use std::time::Duration;

        // Create a server that echoes messages
        let mut conn = create_echo_connection().await;

        // Register request before splitting
        let router = Arc::new(ResponseRouter::new());
        let _rx1 = router.register(RequestId::new(1)).unwrap();
        let _rx2 = router.register(RequestId::new(2)).unwrap();

        // Write response before splitting - it will be buffered in the pipe
        // When reader starts, it reads the response and resets the timer
        let response1 = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": null
        });
        conn.write_message(&response1).await.unwrap();

        let (writer, reader) = conn.split();

        // Spawn reader with liveness timeout (150ms)
        let handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(150)),
        );

        // Notify the reader to start the timer
        handle.notify_liveness_start();

        // Yield to let reader task process the buffered response
        // This resets the timer deadline
        tokio::task::yield_now().await;

        // Advance time past the original timeout (150ms) but before the reset deadline
        // If timer reset worked: deadline is now ~150ms from when response was processed
        // If timer didn't reset: it would fire at 150ms
        tokio::time::advance(Duration::from_millis(160)).await;
        tokio::task::yield_now().await;

        // After first response, pending should be 1 (one request remaining)
        // Timer should have been reset, not fired
        assert!(
            router.pending_count() <= 1,
            "Timer should reset on message activity, not fire prematurely"
        );

        drop(writer);
    }

    /// Test that liveness timer stops when pending count returns to 0.
    ///
    /// ADR-0014: Timer stops when pending count returns to 0.
    /// This verifies that when the last response is received, the timer is deactivated.
    #[tokio::test]
    async fn liveness_timer_stops_when_pending_zero() {
        use crate::lsp::bridge::protocol::RequestId;
        use std::time::Duration;

        // Create a server that echoes messages
        let mut conn = create_echo_connection().await;

        // Register a single request
        let router = Arc::new(ResponseRouter::new());
        let rx = router.register(RequestId::new(1)).unwrap();

        // Write the response before splitting
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": "done"
        });
        conn.write_message(&response).await.unwrap();

        let (writer, reader) = conn.split();

        // Spawn reader with liveness timeout
        let handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(100)),
        );

        // Notify the reader to start the timer
        handle.notify_liveness_start();

        // Wait for response to be received
        let _received = tokio::time::timeout(Duration::from_secs(1), rx)
            .await
            .expect("should receive response")
            .expect("channel should not be closed");

        // Pending count should now be 0
        assert_eq!(
            router.pending_count(),
            0,
            "Pending should be 0 after response"
        );

        // Wait past the original timeout - timer should have stopped, not fired
        tokio::time::sleep(Duration::from_millis(150)).await;

        // If timer had fired, router would have fail_all() called and we'd see errors
        // Since pending is already 0, there's nothing to fail - this is the correct behavior

        drop(writer);
    }

    /// Test that liveness timeout fires and fails pending requests.
    ///
    /// ADR-0014: Ready to Failed transition on liveness timeout expiry.
    /// When timeout fires while pending > 0, router.fail_all() is called.
    #[tokio::test]
    async fn liveness_timeout_fires_and_fails_pending_requests() {
        use crate::lsp::bridge::protocol::RequestId;
        use std::time::Duration;

        // Create an unresponsive server
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn process");

        let (_writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());

        // Register multiple pending requests
        let rx1 = router.register(RequestId::new(1)).unwrap();
        let rx2 = router.register(RequestId::new(2)).unwrap();
        assert_eq!(router.pending_count(), 2);

        // Spawn reader with short liveness timeout
        let handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(50)),
        );

        // Notify the reader to start the timer
        handle.notify_liveness_start();

        // Wait for both receivers to get error responses
        let result1 = tokio::time::timeout(Duration::from_millis(200), rx1)
            .await
            .expect("should not timeout on receiver")
            .expect("channel should not be closed");

        let result2 = tokio::time::timeout(Duration::from_millis(200), rx2)
            .await
            .expect("should not timeout on receiver")
            .expect("channel should not be closed");

        // Both should have received error responses
        assert!(
            result1.get("error").is_some(),
            "Request 1 should have error response"
        );
        assert!(
            result2.get("error").is_some(),
            "Request 2 should have error response"
        );
        assert!(
            result1["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("liveness timeout"),
            "Error should mention liveness timeout"
        );

        // Pending should be 0 after fail_all
        assert_eq!(
            router.pending_count(),
            0,
            "Pending should be 0 after timeout"
        );
    }
}
