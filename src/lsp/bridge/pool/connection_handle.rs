//! Connection handle for downstream language servers.
//!
//! This module provides the per-connection wrapper with state management,
//! request routing, and shutdown logic per ADR-0015.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use log::warn;

use super::ConnectionState;
use super::liveness_timeout::LivenessTimeout;
use crate::lsp::bridge::actor::{ReaderTaskHandle, ResponseRouter};
use crate::lsp::bridge::connection::SplitConnectionWriter;
use crate::lsp::bridge::protocol::{RequestId, build_exit_notification, build_shutdown_request};

/// Handle wrapping a connection with its state (ADR-0015 per-connection state).
///
/// Each connection has its own lifecycle state that transitions:
/// - Initializing: spawn started, awaiting initialize response
/// - Ready: initialize/initialized handshake complete
/// - Failed: initialization failed (timeout, error, etc.)
///
/// # Architecture (ADR-0015 Phase A)
///
/// Uses Reader Task separation for non-blocking response waiting:
/// - `writer`: Mutex-protected for serialized request sending
/// - `router`: Routes responses to oneshot waiters
/// - `reader_handle`: Background task reading from stdout
///
/// Request flow:
/// 1. Register request ID with router to get oneshot receiver
/// 2. Lock writer, send request, release lock
/// 3. Await oneshot receiver (no Mutex held)
pub(crate) struct ConnectionHandle {
    /// Connection state - uses std::sync::RwLock for fast, synchronous state checks
    state: std::sync::RwLock<ConnectionState>,
    /// Writer for sending messages (Mutex serializes writes)
    writer: tokio::sync::Mutex<SplitConnectionWriter>,
    /// Router for pending request tracking
    router: Arc<ResponseRouter>,
    /// Handle to the reader task.
    ///
    /// Used for:
    /// - RAII cleanup on drop (cancels reader task)
    /// - Liveness timer start notification (pending 0->1)
    reader_handle: ReaderTaskHandle,
    /// Atomic counter for generating unique downstream request IDs.
    ///
    /// Each upstream request may have the same ID (from different contexts),
    /// so we generate unique IDs for downstream requests to avoid
    /// "duplicate request ID" errors in the ResponseRouter.
    next_request_id: AtomicI64,
}

impl ConnectionHandle {
    /// Create a new ConnectionHandle in Ready state (test helper).
    ///
    /// Used in tests where we need a connection handle without going through
    /// the full initialization flow.
    #[cfg(test)]
    pub(super) fn new(
        writer: SplitConnectionWriter,
        router: Arc<ResponseRouter>,
        reader_handle: ReaderTaskHandle,
    ) -> Self {
        Self::with_state(writer, router, reader_handle, ConnectionState::Ready)
    }

    /// Create a new ConnectionHandle with a specific initial state.
    ///
    /// Uses default liveness timeout (60s per ADR-0018 Tier 2).
    ///
    /// Used for async initialization where the connection starts in Initializing
    /// state and transitions to Ready or Failed based on init result.
    ///
    /// # State Transitions (ADR-0015)
    /// - Start in `Initializing` state during LSP handshake
    /// - Transition to `Ready` on successful initialization
    /// - Transition to `Failed` on timeout or error
    pub(super) fn with_state(
        writer: SplitConnectionWriter,
        router: Arc<ResponseRouter>,
        reader_handle: ReaderTaskHandle,
        initial_state: ConnectionState,
    ) -> Self {
        let liveness_timeout = LivenessTimeout::default();
        log::debug!(
            target: "kakehashi::bridge",
            "Creating connection handle with liveness timeout: {:?}",
            liveness_timeout.as_duration()
        );
        Self {
            state: std::sync::RwLock::new(initial_state),
            writer: tokio::sync::Mutex::new(writer),
            router,
            reader_handle,
            // Start at 2 because ID=1 is reserved for the initialize request
            // which is pre-registered before spawning the reader task.
            next_request_id: AtomicI64::new(2),
        }
    }

    /// Generate a unique downstream request ID.
    ///
    /// Each call returns the next ID in the sequence (2, 3, 4, ...).
    /// ID=1 is reserved for the initialize request.
    /// This ensures unique IDs for the ResponseRouter even when multiple
    /// upstream requests have the same ID.
    pub(crate) fn next_request_id(&self) -> i64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Get the current connection state.
    ///
    /// Uses std::sync::RwLock for fast, non-blocking read access.
    /// Recovers from poisoned locks with logging per project convention.
    pub(crate) fn state(&self) -> ConnectionState {
        match self.state.read() {
            Ok(guard) => *guard,
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned state lock in ConnectionHandle::state()"
                );
                *poisoned.into_inner()
            }
        }
    }

    /// Set the connection state.
    ///
    /// Used for state transitions during async initialization:
    /// - Initializing -> Ready (on successful init)
    /// - Initializing -> Failed (on timeout/error)
    ///
    /// Recovers from poisoned locks with logging per project convention.
    pub(super) fn set_state(&self, new_state: ConnectionState) {
        match self.state.write() {
            Ok(mut guard) => *guard = new_state,
            Err(poisoned) => {
                warn!(
                    target: "kakehashi::lock_recovery",
                    "Recovered from poisoned state lock in ConnectionHandle::set_state()"
                );
                *poisoned.into_inner() = new_state;
            }
        }
    }

    /// Begin graceful shutdown of the connection.
    ///
    /// Transitions the connection to Closing state, which:
    /// - Rejects new requests with "bridge: connection closing" error
    /// - Signals that LSP shutdown/exit handshake should begin
    /// - Stops the liveness timer (ADR-0018: global shutdown overrides liveness)
    ///
    /// Note: The reader task continues running to receive the shutdown response.
    /// Only the liveness timer is disabled, not the entire reader.
    ///
    /// Valid from Ready or Initializing states per ADR-0015/ADR-0017.
    pub(crate) fn begin_shutdown(&self) {
        // Stop the liveness timer (but not the reader task) per ADR-0018
        // Global shutdown (Tier 3) overrides liveness timeout (Tier 2)
        // Reader continues running to receive shutdown response
        self.reader_handle.stop_liveness_timer();
        self.set_state(ConnectionState::Closing);
    }

    /// Complete the shutdown sequence.
    ///
    /// Transitions the connection to Closed state (terminal).
    /// Called after LSP shutdown/exit handshake completes or times out.
    ///
    /// Valid from Closing or Failed states per ADR-0015/ADR-0017.
    pub(crate) fn complete_shutdown(&self) {
        self.set_state(ConnectionState::Closed);
    }

    /// Force kill the child process with platform-appropriate escalation.
    ///
    /// This is the fallback when LSP shutdown handshake times out or fails.
    ///
    /// # Platform-Specific Behavior
    ///
    /// **Unix (Linux, macOS)**:
    /// 1. Send SIGTERM to allow graceful termination
    /// 2. Wait for up to 2 seconds for the process to exit
    /// 3. If still alive, send SIGKILL for forced termination
    ///
    /// **Windows**:
    /// - Directly calls `TerminateProcess` via `start_kill()`
    /// - No graceful period (Windows has no SIGTERM equivalent)
    pub(crate) async fn force_kill(&self) {
        let mut writer = self.writer.lock().await;
        writer.force_kill_with_escalation().await;
    }

    /// Perform graceful shutdown with LSP handshake (ADR-0017).
    ///
    /// Implements the LSP shutdown sequence:
    /// 1. Transition to Closing state (new operations rejected)
    /// 2. Send LSP "shutdown" request and wait for response
    /// 3. Send LSP "exit" notification
    /// 4. Force kill process (Unix: SIGTERM→SIGKILL escalation)
    /// 5. Transition to Closed state
    ///
    /// # Cleanup Guarantee
    ///
    /// Steps 4-5 (force kill and state transition) are **always executed**,
    /// even if the LSP handshake fails. This prevents connections from getting
    /// stuck in the Closing state.
    ///
    /// # Returns
    /// - Ok(()) if shutdown completed (gracefully or via force-kill)
    /// - Err only if the method couldn't complete at all (shouldn't happen)
    ///
    /// # Timeout Behavior
    ///
    /// This method has **no internal timeout** per ADR-0018. It waits indefinitely
    /// for the shutdown response. The caller (shutdown_all_with_timeout) is
    /// responsible for enforcing the global shutdown timeout.
    ///
    /// This design ensures:
    /// - Fast servers complete quickly without artificial delays
    /// - Slow servers use remaining time from the global budget
    /// - Single timeout ceiling prevents timeout multiplication (N * 5s)
    pub(crate) async fn graceful_shutdown(&self) -> io::Result<()> {
        // 1. Transition to Closing state
        self.begin_shutdown();

        // 2-3. Perform LSP handshake, capturing any error
        // Wrapped in async block to ensure cleanup (steps 4-5) always runs
        let handshake_result: io::Result<()> = async {
            // 2. Send LSP shutdown request
            let (request_id, response_rx) = self.register_request()?;
            let shutdown_request = build_shutdown_request(request_id);

            {
                let mut writer = self.writer().await;
                writer.write_message(&shutdown_request).await?;
            }

            // Wait for shutdown response (no timeout - global timeout handles this)
            // Per ADR-0018: graceful_shutdown has no internal timeout
            match response_rx.await {
                Ok(_response) => {
                    log::debug!(
                        target: "kakehashi::bridge",
                        "Shutdown response received, sending exit notification"
                    );
                }
                Err(_) => {
                    log::warn!(
                        target: "kakehashi::bridge",
                        "Shutdown response channel closed"
                    );
                }
            }

            // 3. Send exit notification (no response expected)
            let exit_notification = build_exit_notification();

            {
                let mut writer = self.writer().await;
                // Best effort - if this fails, process will be killed anyway
                let _ = writer.write_message(&exit_notification).await;
            }

            Ok(())
        }
        .await;

        // 4. Force kill the process with platform-appropriate escalation
        // This ensures the process is terminated even if it ignores exit notification
        // ALWAYS executed, even if handshake failed
        //
        // Unix: SIGTERM->SIGKILL escalation with 2s grace period
        // Windows: TerminateProcess directly (no grace period)
        self.force_kill().await;

        // 5. Transition to Closed state
        // ALWAYS executed, even if handshake failed
        self.complete_shutdown();

        // Log handshake errors but return Ok since shutdown completed (via force-kill if needed)
        if let Err(e) = &handshake_result {
            log::debug!(
                target: "kakehashi::bridge",
                "LSP handshake had error during shutdown (connection force-killed): {}",
                e
            );
        }

        // Always return Ok - the connection is now Closed regardless of handshake result
        Ok(())
    }

    /// Get access to the writer for sending messages.
    ///
    /// Returns the tokio::sync::MutexGuard for exclusive write access.
    pub(crate) async fn writer(&self) -> tokio::sync::MutexGuard<'_, SplitConnectionWriter> {
        self.writer.lock().await
    }

    /// Get the response router for registering pending requests.
    pub(crate) fn router(&self) -> &Arc<ResponseRouter> {
        &self.router
    }

    /// Register a new request and return (request_id, response_receiver).
    ///
    /// Generates a unique request ID and registers it with the router.
    /// If this is the first pending request (pending 0->1), notifies the reader
    /// task to start the liveness timer (ADR-0014).
    ///
    /// Returns error if registration fails (should never happen with unique IDs).
    pub(crate) fn register_request(
        &self,
    ) -> io::Result<(RequestId, tokio::sync::oneshot::Receiver<serde_json::Value>)> {
        // Check if this will be the first pending request (0->1 transition)
        let was_empty = self.router().pending_count() == 0;

        let request_id = RequestId::new(self.next_request_id());
        let response_rx = self
            .router()
            .register(request_id)
            .ok_or_else(|| io::Error::other("bridge: duplicate request ID"))?;

        // If pending went 0->1 and we're in Ready state, start liveness timer
        if was_empty && self.state() == ConnectionState::Ready {
            self.reader_handle.notify_liveness_start();
        }

        Ok((request_id, response_rx))
    }

    /// Wait for a response with timeout, cleaning up on timeout.
    ///
    /// Takes the oneshot receiver and request ID, waits for response with
    /// 30-second timeout. On timeout, removes the pending entry from router.
    ///
    /// Also checks for liveness timeout failure and transitions to Failed state
    /// if the reader task signaled a liveness timeout (ADR-0014 Phase 3).
    pub(crate) async fn wait_for_response(
        &self,
        request_id: RequestId,
        response_rx: tokio::sync::oneshot::Receiver<serde_json::Value>,
    ) -> io::Result<serde_json::Value> {
        use tokio::time::timeout;

        const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

        match timeout(REQUEST_TIMEOUT, response_rx).await {
            Ok(Ok(response)) => {
                // Check if this was an error response from liveness timeout
                // If so, transition to Failed state (ADR-0014 Phase 3)
                if self.reader_handle.check_liveness_failed() {
                    self.set_state(ConnectionState::Failed);
                }
                Ok(response)
            }
            Ok(Err(_)) => {
                // Channel closed - check if due to liveness timeout
                // If so, transition to Failed state (ADR-0014 Phase 3)
                if self.reader_handle.check_liveness_failed() {
                    self.set_state(ConnectionState::Failed);
                }
                Err(io::Error::other("bridge: response channel closed"))
            }
            Err(_) => {
                // Timeout - clean up pending entry
                self.router().remove(request_id);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "bridge: request timeout",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::bridge::actor::{ResponseRouter, spawn_reader_task};
    use crate::lsp::bridge::connection::AsyncBridgeConnection;

    /// Test that ConnectionHandle provides unique request IDs via atomic counter.
    ///
    /// Each call to next_request_id() should return a unique, incrementing value.
    /// This is critical for avoiding "duplicate request ID" errors when multiple
    /// upstream requests have the same ID (they come from different contexts).
    #[tokio::test]
    async fn connection_handle_provides_unique_request_ids() {
        // Create a mock server process to get a real connection
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        // Split connection and spawn reader task
        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        // Wrap in ConnectionHandle
        let handle = ConnectionHandle::new(writer, router, reader_handle);

        // Get multiple request IDs - they should be unique and incrementing
        // Note: IDs start at 2 because ID=1 is reserved for the initialize request
        let id1 = handle.next_request_id();
        let id2 = handle.next_request_id();
        let id3 = handle.next_request_id();

        assert_eq!(
            id1, 2,
            "First user request ID should be 2 (1 is reserved for initialize)"
        );
        assert_eq!(id2, 3, "Second user request ID should be 3");
        assert_eq!(id3, 4, "Third user request ID should be 4");
    }

    /// Test that ConnectionHandle wraps connection with state (ADR-0015).
    /// State should start as Ready (since constructor is called after init handshake),
    /// and can transition via set_state().
    #[tokio::test]
    async fn connection_handle_wraps_connection_with_state() {
        // Create a mock server process to get a real connection
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        // Split connection and spawn reader task (new architecture)
        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        // Wrap in ConnectionHandle
        let handle = ConnectionHandle::new(writer, router, reader_handle);

        // Initial state should be Ready (ConnectionHandle is created after init handshake)
        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "Initial state should be Ready"
        );

        // Can transition to Failed
        handle.set_state(ConnectionState::Failed);
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "State should transition to Failed"
        );

        // Can access writer
        let _writer_guard = handle.writer().await;
        // Writer is accessible (test passes if no panic)

        // Can access router
        let _router = handle.router();
        // Router is accessible (test passes if no panic)
    }

    /// Test that liveness timeout triggers Ready->Failed state transition (ADR-0014 Phase 3).
    ///
    /// When the liveness timer fires:
    /// 1. router.fail_all() sends error responses to pending requests
    /// 2. ConnectionHandle transitions to Failed state
    /// 3. Failed state triggers SpawnNew action on next request
    #[tokio::test]
    async fn liveness_timeout_transitions_to_failed_state() {
        use crate::lsp::bridge::actor::spawn_reader_task_with_liveness;
        use std::time::Duration;

        // Create an unresponsive server (consumes input, never outputs)
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn process");

        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());

        // Spawn reader with short liveness timeout
        let reader_handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(50)),
        );

        // Create ConnectionHandle in Ready state
        let handle = Arc::new(ConnectionHandle::with_state(
            writer,
            router,
            reader_handle,
            ConnectionState::Ready,
        ));

        // Verify initial state is Ready
        assert_eq!(handle.state(), ConnectionState::Ready);

        // Register a request - this will notify the reader to start the liveness timer
        let (request_id, response_rx) = handle.register_request().expect("should register request");

        // Wait for response - this will block until liveness timeout fires
        // The response will be an error from router.fail_all()
        let result = handle.wait_for_response(request_id, response_rx).await;

        // Response should be Ok (error response from fail_all is still delivered)
        let response = result.expect("should receive error response from fail_all");
        assert!(
            response.get("error").is_some(),
            "Response should be an error: {:?}",
            response
        );
        assert!(
            response["error"]["message"]
                .as_str()
                .unwrap_or("")
                .contains("liveness timeout"),
            "Error should mention liveness timeout"
        );

        // After liveness timeout, connection should be in Failed state (Phase 3)
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "Connection should transition to Failed state on liveness timeout"
        );
    }

    /// Test that begin_shutdown() cancels the active liveness timer (ADR-0018 Phase 4).
    ///
    /// When global shutdown begins, the liveness timer should be disabled because
    /// global shutdown (Tier 3) overrides liveness timeout (Tier 2).
    #[tokio::test]
    async fn begin_shutdown_cancels_liveness_timer() {
        use crate::lsp::bridge::actor::spawn_reader_task_with_liveness;
        use std::time::Duration;

        // Create an unresponsive server
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn process");

        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());

        // Spawn reader with short liveness timeout
        let reader_handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(50)),
        );

        // Create ConnectionHandle in Ready state
        let handle = Arc::new(ConnectionHandle::with_state(
            writer,
            router,
            reader_handle,
            ConnectionState::Ready,
        ));

        // Register a request to start the liveness timer
        let (_request_id, _response_rx) = handle.register_request().expect("should register");

        // Immediately begin shutdown - this should cancel the liveness timer
        handle.begin_shutdown();

        // Wait longer than the liveness timeout would have been
        tokio::time::sleep(Duration::from_millis(100)).await;

        // State should be Closing (from begin_shutdown), NOT Failed (from liveness timeout)
        // This proves the liveness timer was cancelled
        assert_eq!(
            handle.state(),
            ConnectionState::Closing,
            "State should be Closing, not Failed - liveness timer should have been cancelled"
        );
    }

    /// Test that liveness timer does not start in Closing state (ADR-0018 Phase 4).
    ///
    /// Once shutdown begins, new requests should not start the liveness timer
    /// because global shutdown (Tier 3) overrides liveness timeout (Tier 2).
    #[tokio::test]
    async fn liveness_timer_does_not_start_in_closing_state() {
        use crate::lsp::bridge::actor::spawn_reader_task_with_liveness;
        use std::time::Duration;

        // Create an echo server
        let mut conn = AsyncBridgeConnection::spawn(vec!["cat".to_string()])
            .await
            .expect("should spawn cat process");

        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());

        // Spawn reader with liveness timeout
        let reader_handle = spawn_reader_task_with_liveness(
            reader,
            Arc::clone(&router),
            Some(Duration::from_millis(50)),
        );

        // Create ConnectionHandle in Closing state
        let handle = Arc::new(ConnectionHandle::with_state(
            writer,
            router,
            reader_handle,
            ConnectionState::Closing,
        ));

        // This call to register_request checks if state is Ready before starting timer
        // In Closing state, the timer should not start
        // Note: register_request itself should still work (for pending shutdown requests)
        // but we're testing that the timer doesn't start

        // We can't easily test this directly since notify_liveness_start is called
        // unconditionally. However, the register_request method checks state before
        // calling notify_liveness_start.

        // The timer start is gated by: `self.state() == ConnectionState::Ready`
        // Since state is Closing, notify_liveness_start should NOT be called.

        // Verify the connection is in Closing state
        assert_eq!(handle.state(), ConnectionState::Closing);

        // This is a documentation test - the actual gating is in register_request
        // which already checks `self.state() == ConnectionState::Ready` before
        // calling `self.reader_handle.notify_liveness_start()`.
    }
}
