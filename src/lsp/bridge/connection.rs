//! Async connection to downstream language server processes.
//!
//! This module provides the core connection type for communicating with
//! language servers via stdio using async I/O.

use std::io;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Connection state for downstream language server.
///
/// Tracks the lifecycle of a connection per ADR-0015:
/// - `Initializing`: Connection spawned, LSP handshake in progress
/// - `Ready`: Initialization completed, requests can be processed
/// - `Failed`: Initialization failed or connection error occurred
///
/// State transitions:
/// - `Initializing` -> `Ready` (on successful initialize response)
/// - `Initializing` -> `Failed` (on timeout or error)
/// - `Ready` -> `Failed` (on crash or panic)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)] // Used in subsequent subtasks
pub(crate) enum ConnectionState {
    /// Connection spawned, LSP initialize/initialized handshake in progress.
    #[default]
    Initializing,
    /// Initialization completed successfully, ready to process requests.
    Ready,
    /// Initialization failed or connection encountered an error.
    Failed,
}

impl ConnectionState {
    /// Convert to u8 for atomic storage.
    fn to_u8(self) -> u8 {
        match self {
            ConnectionState::Initializing => 0,
            ConnectionState::Ready => 1,
            ConnectionState::Failed => 2,
        }
    }

    /// Convert from u8 from atomic storage.
    fn from_u8(value: u8) -> Self {
        match value {
            0 => ConnectionState::Initializing,
            1 => ConnectionState::Ready,
            _ => ConnectionState::Failed,
        }
    }
}

/// LSP JSON-RPC error code for REQUEST_FAILED.
///
/// Per ADR-0015: Used when requests cannot be processed due to connection state.
/// Value -32803 is from LSP spec reserved range for implementation errors.
const REQUEST_FAILED_CODE: i32 = -32803;

/// Bridge-specific error type for state-gated request handling.
///
/// Per ADR-0015, requests during non-Ready states return REQUEST_FAILED (-32803)
/// with state-specific messages.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct BridgeError {
    code: i32,
    message: String,
}

impl BridgeError {
    /// Create a REQUEST_FAILED error with the given message.
    #[allow(dead_code)]
    pub(crate) fn request_failed(message: &str) -> Self {
        Self {
            code: REQUEST_FAILED_CODE,
            message: message.to_string(),
        }
    }

    /// Get the error code.
    #[allow(dead_code)]
    pub(crate) fn code(&self) -> i32 {
        self.code
    }

    /// Get the error message.
    #[allow(dead_code)]
    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    /// Create an error for the given connection state, or None if state allows requests.
    ///
    /// Per ADR-0015:
    /// - `Initializing` -> REQUEST_FAILED ("bridge: downstream server initializing")
    /// - `Ready` -> None (requests allowed)
    /// - `Failed` -> REQUEST_FAILED ("bridge: downstream server failed")
    #[allow(dead_code)]
    pub(crate) fn for_state(state: ConnectionState) -> Option<Self> {
        match state {
            ConnectionState::Initializing => Some(Self::request_failed(
                "bridge: downstream server initializing",
            )),
            ConnectionState::Ready => None,
            ConnectionState::Failed => {
                Some(Self::request_failed("bridge: downstream server failed"))
            }
        }
    }

    /// Convert to a JSON-RPC error response.
    #[allow(dead_code)]
    pub(crate) fn to_json_response(&self, id: i64) -> serde_json::Value {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": self.code,
                "message": self.message
            }
        })
    }
}

/// Thread-safe connection state holder.
///
/// Wraps connection state in an atomic for safe access across async tasks.
/// Used per ADR-0015 for operation gating based on connection state.
#[derive(Clone)]
pub(crate) struct StatefulBridgeConnection {
    state: Arc<AtomicU8>,
}

impl StatefulBridgeConnection {
    /// Create a new state holder in Initializing state.
    #[allow(dead_code)]
    pub(crate) fn new_initializing() -> Self {
        Self {
            state: Arc::new(AtomicU8::new(ConnectionState::Initializing.to_u8())),
        }
    }

    /// Get the current connection state.
    #[allow(dead_code)]
    pub(crate) fn state(&self) -> ConnectionState {
        ConnectionState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Transition to Ready state.
    #[allow(dead_code)]
    pub(crate) fn set_ready(&self) {
        self.state
            .store(ConnectionState::Ready.to_u8(), Ordering::Release);
    }

    /// Transition to Failed state.
    #[allow(dead_code)]
    pub(crate) fn set_failed(&self) {
        self.state
            .store(ConnectionState::Failed.to_u8(), Ordering::Release);
    }
}

/// Handle to a bridge connection with state tracking.
///
/// Combines connection state tracking (via atomic) with connection access.
/// Used by BridgeManager to provide non-blocking initialization per ADR-0015.
///
/// The handle starts in Initializing state and transitions to:
/// - Ready: After successful LSP handshake (Subtask 5)
/// - Failed: On timeout or initialization error (Subtask 6)
#[derive(Clone)]
pub(crate) struct BridgeConnectionHandle {
    state: StatefulBridgeConnection,
}

impl BridgeConnectionHandle {
    /// Create a new handle in Initializing state.
    ///
    /// Per ADR-0015, connections start in Initializing state.
    /// Requests during this state will receive REQUEST_FAILED error.
    #[allow(dead_code)]
    pub(crate) fn new_initializing() -> Self {
        Self {
            state: StatefulBridgeConnection::new_initializing(),
        }
    }

    /// Get the current connection state.
    #[allow(dead_code)]
    pub(crate) fn state(&self) -> ConnectionState {
        self.state.state()
    }

    /// Transition to Ready state after successful initialization.
    #[allow(dead_code)]
    pub(crate) fn set_ready(&self) {
        self.state.set_ready();
    }

    /// Transition to Failed state on initialization error or timeout.
    #[allow(dead_code)]
    pub(crate) fn set_failed(&self) {
        self.state.set_failed();
    }

    /// Check if the connection is ready for requests.
    ///
    /// Returns None if Ready, or BridgeError if not ready.
    /// Per ADR-0015: Requests during non-Ready states return REQUEST_FAILED.
    #[allow(dead_code)]
    pub(crate) fn check_ready(&self) -> Option<BridgeError> {
        BridgeError::for_state(self.state())
    }
}

/// Async connection to a downstream language server process.
///
/// Manages the lifecycle of a child process running a language server,
/// providing async I/O for LSP JSON-RPC communication over stdio.
///
/// # Architecture (ADR-0014)
///
/// - Uses `tokio::process::Command` for process spawning
/// - Dedicated async reader task with `select!` for cancellation
/// - Pending request tracking via `DashMap<RequestId, oneshot::Sender>`
pub(crate) struct AsyncBridgeConnection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl AsyncBridgeConnection {
    /// Spawn a new language server process.
    ///
    /// # Arguments
    /// * `cmd` - Command and arguments to spawn (e.g., `["lua-language-server"]`)
    ///
    /// # Returns
    /// A new `AsyncBridgeConnection` with stdio pipes connected to the child process.
    pub(crate) async fn spawn(cmd: Vec<String>) -> io::Result<Self> {
        let (program, args) = cmd.split_first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "command must not be empty")
        })?;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stdout"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Get the process ID of the child process.
    #[allow(dead_code)]
    pub(crate) fn child_id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Write a JSON-RPC message to the child process stdin.
    ///
    /// Formats the message with LSP Content-Length header:
    /// `Content-Length: <length>\r\n\r\n<json>`
    pub(crate) async fn write_message(&mut self, message: &serde_json::Value) -> io::Result<()> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;

        Ok(())
    }

    /// Read the raw bytes of an LSP message body from stdout.
    ///
    /// Parses the Content-Length header, reads the separator, and returns the body bytes.
    async fn read_message_bytes(&mut self) -> io::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;

        // Read header line
        let mut header_line = String::new();
        self.stdout.read_line(&mut header_line).await?;

        // Parse content length
        let content_length: usize = header_line
            .strip_prefix("Content-Length: ")
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length"))?;

        // Read empty line (CRLF separator)
        let mut empty_line = String::new();
        self.stdout.read_line(&mut empty_line).await?;

        // Read body
        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).await?;

        Ok(body)
    }

    /// Read and parse a JSON-RPC message from the child process stdout.
    ///
    /// Parses the Content-Length header and reads the JSON body.
    pub(crate) async fn read_message(&mut self) -> io::Result<serde_json::Value> {
        let body = self.read_message_bytes().await?;
        serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Read a raw LSP message from the child process stdout.
    ///
    /// Returns the full message including header and body as a string.
    /// Used primarily for testing to verify message format.
    #[cfg(test)]
    pub(crate) async fn read_raw_message(&mut self) -> io::Result<String> {
        use tokio::io::AsyncReadExt;

        // Read header line
        let mut header_line = String::new();
        self.stdout.read_line(&mut header_line).await?;

        // Parse content length
        let content_length: usize = header_line
            .strip_prefix("Content-Length: ")
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length"))?;

        // Read empty line (CRLF separator)
        let mut empty_line = String::new();
        self.stdout.read_line(&mut empty_line).await?;

        // Read body
        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).await?;

        // Reconstruct the full message (including headers for verification)
        let full_message = format!(
            "{}{}{}",
            header_line,
            empty_line,
            String::from_utf8_lossy(&body)
        );

        Ok(full_message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_has_required_variants() {
        // Test that ConnectionState enum has the required variants per ADR-0015
        let init = ConnectionState::Initializing;
        let ready = ConnectionState::Ready;
        let failed = ConnectionState::Failed;

        // Verify each variant is distinct
        assert_ne!(init, ready);
        assert_ne!(ready, failed);
        assert_ne!(init, failed);
    }

    #[test]
    fn connection_state_default_is_initializing() {
        // New connections should start in Initializing state
        let state = ConnectionState::default();
        assert_eq!(state, ConnectionState::Initializing);
    }

    #[test]
    fn stateful_connection_exposes_state() {
        // StatefulBridgeConnection should expose current state via state() method
        let state_holder = StatefulBridgeConnection::new_initializing();
        assert_eq!(state_holder.state(), ConnectionState::Initializing);
    }

    #[test]
    fn stateful_connection_can_transition_to_ready() {
        let state_holder = StatefulBridgeConnection::new_initializing();
        state_holder.set_ready();
        assert_eq!(state_holder.state(), ConnectionState::Ready);
    }

    #[test]
    fn stateful_connection_can_transition_to_failed() {
        let state_holder = StatefulBridgeConnection::new_initializing();
        state_holder.set_failed();
        assert_eq!(state_holder.state(), ConnectionState::Failed);
    }

    #[test]
    fn bridge_error_request_failed_has_correct_code() {
        // Per ADR-0015: REQUEST_FAILED error code is -32803
        let error = BridgeError::request_failed("bridge: downstream server initializing");
        assert_eq!(error.code(), -32803);
        assert_eq!(error.message(), "bridge: downstream server initializing");
    }

    #[test]
    fn bridge_error_for_initializing_state() {
        // Per ADR-0015: Requests during Initializing state return REQUEST_FAILED
        let state = ConnectionState::Initializing;
        let error = BridgeError::for_state(state);
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.code(), -32803);
        assert!(error.message().contains("initializing"));
    }

    #[test]
    fn bridge_error_for_ready_state_returns_none() {
        // Ready state allows requests - no error
        let state = ConnectionState::Ready;
        let error = BridgeError::for_state(state);
        assert!(error.is_none());
    }

    #[test]
    fn bridge_error_for_failed_state() {
        // Per ADR-0015: Requests during Failed state return REQUEST_FAILED
        let state = ConnectionState::Failed;
        let error = BridgeError::for_state(state);
        assert!(error.is_some());
        let error = error.unwrap();
        assert_eq!(error.code(), -32803);
        assert!(error.message().contains("failed"));
    }

    // --- Subtask 4, 5, 6: BridgeConnectionHandle tests ---

    #[test]
    fn bridge_connection_handle_starts_in_initializing_state() {
        // BridgeConnectionHandle should start in Initializing state
        let handle = BridgeConnectionHandle::new_initializing();
        assert_eq!(handle.state(), ConnectionState::Initializing);
    }

    #[test]
    fn bridge_connection_handle_can_transition_to_ready() {
        let handle = BridgeConnectionHandle::new_initializing();
        handle.set_ready();
        assert_eq!(handle.state(), ConnectionState::Ready);
    }

    #[test]
    fn bridge_connection_handle_can_transition_to_failed() {
        let handle = BridgeConnectionHandle::new_initializing();
        handle.set_failed();
        assert_eq!(handle.state(), ConnectionState::Failed);
    }

    #[test]
    fn bridge_connection_handle_check_ready_returns_none_for_ready() {
        let handle = BridgeConnectionHandle::new_initializing();
        handle.set_ready();
        let error = handle.check_ready();
        assert!(error.is_none(), "Ready state should allow requests");
    }

    #[test]
    fn bridge_connection_handle_check_ready_returns_error_for_initializing() {
        let handle = BridgeConnectionHandle::new_initializing();
        let error = handle.check_ready();
        assert!(error.is_some(), "Initializing state should reject requests");
        let error = error.unwrap();
        assert_eq!(error.code(), -32803);
    }

    #[test]
    fn bridge_connection_handle_check_ready_returns_error_for_failed() {
        let handle = BridgeConnectionHandle::new_initializing();
        handle.set_failed();
        let error = handle.check_ready();
        assert!(error.is_some(), "Failed state should reject requests");
        let error = error.unwrap();
        assert_eq!(error.code(), -32803);
    }
}

impl Drop for AsyncBridgeConnection {
    fn drop(&mut self) {
        // Kill the child process to prevent orphans (AC3)
        // start_kill() is non-blocking and signals the process to terminate
        if let Err(e) = self.child.start_kill() {
            log::warn!(
                target: "treesitter_ls::bridge",
                "Failed to kill child process: {}",
                e
            );
        } else {
            log::debug!(
                target: "treesitter_ls::bridge",
                "Killed child process {:?}",
                self.child.id()
            );
        }
    }
}
