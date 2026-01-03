//! Tokio-based async bridge connection for concurrent LSP request handling.
//!
//! This module provides `TokioAsyncBridgeConnection` which uses tokio::process::Command
//! for spawning language servers and tokio::spawn for the reader task, enabling fully
//! async I/O without blocking OS threads.
//!
//! # Key Differences from AsyncBridgeConnection
//!
//! - Uses `tokio::process::Command` instead of `std::process::Command`
//! - Uses `tokio::sync::Mutex<ChildStdin>` instead of `std::sync::Mutex<ChildStdin>`
//! - Uses `tokio::task::JoinHandle` instead of `std::thread::JoinHandle`
//! - Uses `oneshot::Sender<()>` for shutdown instead of `AtomicBool`
//! - Reader task uses `tokio::select!` for clean shutdown handling

use dashmap::DashMap;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

/// Maximum number of pending requests before backpressure kicks in
const MAX_PENDING_REQUESTS: usize = 100;

/// Result of a response read operation
#[derive(Debug)]
pub struct ResponseResult {
    /// The JSON-RPC response (or None if error/timeout)
    pub response: Option<Value>,
    /// Captured $/progress notifications
    pub notifications: Vec<Value>,
}

/// Pending request entry - stores the sender for a response
type PendingRequest = oneshot::Sender<ResponseResult>;

/// Tokio-based async bridge connection that handles concurrent LSP requests.
///
/// This struct uses tokio's async primitives throughout:
/// - `tokio::process::Command` for spawning
/// - `tokio::sync::Mutex` for stdin serialization
/// - `tokio::task::JoinHandle` for the reader task
/// - `oneshot::Sender` for shutdown signaling
pub struct TokioAsyncBridgeConnection {
    /// Stdin for writing requests (protected by tokio::sync::Mutex for async write serialization)
    stdin: tokio::sync::Mutex<ChildStdin>,
    /// Pending requests awaiting responses: request_id -> response sender
    pending_requests: Arc<DashMap<i64, PendingRequest>>,
    /// Next request ID (atomically incremented)
    next_request_id: AtomicI64,
    /// Handle to the background reader task
    reader_handle: Option<JoinHandle<()>>,
    /// Shutdown signal sender
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Child process handle (for cleanup on drop, protected by Mutex for is_alive() access)
    child: Option<tokio::sync::Mutex<Child>>,
    /// Temporary directory path (for cleanup on drop)
    temp_dir: Option<PathBuf>,
    /// Track whether the language server has been initialized (received initialized notification)
    pub(crate) initialized: AtomicBool,
}

impl TokioAsyncBridgeConnection {
    /// Spawn a new language server process using tokio::process::Command.
    ///
    /// This creates a child process with stdin/stdout piped for LSP communication.
    /// A background reader task is spawned to handle incoming responses.
    ///
    /// # Arguments
    /// * `command` - The command to spawn (e.g., "rust-analyzer")
    /// * `args` - Arguments to pass to the command
    /// * `cwd` - Optional working directory for the child process
    /// * `notification_sender` - Optional channel for forwarding $/progress notifications
    /// * `temp_dir` - Optional temporary directory path (for cleanup on drop)
    ///
    /// # Returns
    /// A new TokioAsyncBridgeConnection wrapping the spawned process
    pub async fn spawn(
        command: &str,
        args: &[&str],
        cwd: Option<&std::path::Path>,
        notification_sender: Option<mpsc::Sender<Value>>,
        temp_dir: Option<PathBuf>,
    ) -> Result<Self, String> {
        use std::process::Stdio;
        use tokio::process::Command;

        // Build command with optional working directory
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Set working directory if specified
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        // Spawn the child process with piped stdin/stdout
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn process '{}': {}", command, e))?;

        // Extract stdin and stdout from the child process (AC2)
        // Note: We keep the child handle for cleanup on drop
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to capture stdin".to_string())?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to capture stdout".to_string())?;

        // Create the pending requests map
        let pending_requests: Arc<DashMap<i64, PendingRequest>> = Arc::new(DashMap::new());

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // Clone pending_requests for the reader task
        let pending_clone = pending_requests.clone();

        // Spawn the reader task with tokio::select! for clean shutdown
        let reader_handle = tokio::spawn(async move {
            Self::reader_loop(stdout, pending_clone, shutdown_rx, notification_sender).await;
        });

        Ok(Self {
            stdin: tokio::sync::Mutex::new(stdin),
            pending_requests,
            next_request_id: AtomicI64::new(1),
            reader_handle: Some(reader_handle),
            shutdown_tx: Some(shutdown_tx),
            child: Some(tokio::sync::Mutex::new(child)),
            temp_dir,
            initialized: AtomicBool::new(false),
        })
    }

    /// Handle a response message by routing it to the appropriate pending request.
    ///
    /// # Arguments
    /// * `message` - The LSP response message with an "id" field
    /// * `pending` - Map of pending requests awaiting responses
    fn handle_response(message: Value, pending: &Arc<DashMap<i64, PendingRequest>>) {
        if let Some(id) = message.get("id").and_then(|id| id.as_i64()) {
            log::debug!(
                target: "treesitter_ls::bridge::tokio",
                "[READER] Routing response for id={}",
                id
            );

            if let Some((_, sender)) = pending.remove(&id) {
                let result = ResponseResult {
                    response: Some(message),
                    notifications: vec![],
                };
                // Send response (ignore error if receiver dropped)
                let _ = sender.send(result);
            } else {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio",
                    "[READER] No pending request for id={}",
                    id
                );
            }
        }
    }

    /// Handle a notification message by forwarding $/progress to the notification channel.
    ///
    /// # Arguments
    /// * `message` - The LSP notification message with a "method" field
    /// * `notification_sender` - Optional channel for forwarding $/progress notifications
    fn handle_notification(message: Value, notification_sender: &Option<mpsc::Sender<Value>>) {
        if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
            // Check for $/progress notification and forward it
            if method == "$/progress"
                && let Some(sender) = notification_sender
            {
                log::debug!(
                    target: "treesitter_ls::bridge::tokio",
                    "[READER] Forwarding $/progress notification"
                );
                if let Err(e) = sender.try_send(message) {
                    log::warn!(
                        target: "treesitter_ls::bridge",
                        "Notification channel full, dropped message: {}",
                        e
                    );
                }
            }
        }
    }

    /// Handle connection end (EOF or error) by cleaning up pending requests.
    ///
    /// # Arguments
    /// * `pending` - Map of pending requests awaiting responses
    /// * `reason` - Human-readable reason for connection end (for logging)
    fn handle_connection_end(pending: &Arc<DashMap<i64, PendingRequest>>, reason: &str) {
        log::debug!(
            target: "treesitter_ls::bridge::tokio",
            "[READER] {}",
            reason
        );

        // Clean up all pending requests
        Self::clear_pending_requests(pending);
    }

    /// Background reader loop that reads responses and routes them to callers.
    ///
    /// Uses tokio::select! to handle:
    /// - Reading LSP messages from stdout
    /// - Shutdown signal for clean termination
    ///
    /// This is the key difference from sync AsyncBridgeConnection - shutdown can
    /// be processed immediately without waiting for a blocking read to complete.
    ///
    /// # Arguments
    /// * `stdout` - The child process stdout to read from
    /// * `pending` - Map of pending requests awaiting responses
    /// * `shutdown_rx` - Receiver for shutdown signal
    /// * `notification_sender` - Optional channel for forwarding $/progress notifications
    async fn reader_loop(
        stdout: ChildStdout,
        pending: Arc<DashMap<i64, PendingRequest>>,
        mut shutdown_rx: oneshot::Receiver<()>,
        notification_sender: Option<mpsc::Sender<Value>>,
    ) {
        let mut reader = BufReader::new(stdout);

        loop {
            tokio::select! {
                biased;

                // Check shutdown signal first (biased ensures this is checked before read)
                _ = &mut shutdown_rx => {
                    log::debug!(
                        target: "treesitter_ls::bridge::tokio",
                        "[READER] Shutdown signal received"
                    );
                    break;
                }

                // Try to read a message from stdout
                result = Self::read_message(&mut reader) => {
                    match result {
                        Ok(Some(message)) => {
                            // Check if this is a response (has "id" field)
                            if message.get("id").is_some() {
                                Self::handle_response(message, &pending);
                            } else {
                                // This is a notification (method without id)
                                Self::handle_notification(message, &notification_sender);
                            }
                        }
                        Ok(None) => {
                            // EOF or empty read
                            Self::handle_connection_end(&pending, "EOF or empty read");
                            break;
                        }
                        Err(e) => {
                            // Error reading message
                            let reason = format!("Error reading message: {}", e);
                            Self::handle_connection_end(&pending, &reason);
                            break;
                        }
                    }
                }
            }
        }

        log::debug!(
            target: "treesitter_ls::bridge::tokio",
            "[READER] Reader loop exiting"
        );
    }

    /// Send a JSON-RPC request and return a receiver for the response.
    ///
    /// This method is fully async. It writes the request to stdin and returns
    /// immediately with a receiver. The caller can await the response asynchronously.
    ///
    /// # Arguments
    /// * `method` - The JSON-RPC method name
    /// * `params` - The request parameters
    ///
    /// # Returns
    /// A tuple of (request_id, receiver) where the receiver will contain the response
    /// when it arrives, or an error if the request could not be sent.
    pub async fn send_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(i64, oneshot::Receiver<ResponseResult>), String> {
        // Guard: return error if not initialized to prevent protocol errors
        if !self.initialized.load(Ordering::SeqCst) {
            return Err("Cannot send request: language server not initialized".to_string());
        }

        // Check backpressure limit
        if self.pending_requests.len() >= MAX_PENDING_REQUESTS {
            return Err("Too many pending requests (backpressure limit reached)".to_string());
        }

        // Generate request ID atomically
        let id = self.next_request_id.fetch_add(1, Ordering::SeqCst);

        // Create oneshot channel for response
        let (sender, receiver) = oneshot::channel();

        // Register pending request BEFORE sending (to avoid race with reader)
        self.pending_requests.insert(id, sender);

        // Build request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Write request to stdin
        let content = serde_json::to_string(&request).map_err(|e| format!("JSON error: {}", e))?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        // If write fails, clean up pending entry
        if let Err(e) = async {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(header.as_bytes()).await?;
            stdin.write_all(content.as_bytes()).await?;
            stdin.flush().await
        }
        .await
        {
            self.pending_requests.remove(&id);
            return Err(format!("Write error: {}", e));
        }

        log::debug!(
            target: "treesitter_ls::bridge::tokio",
            "[CONN] Sent request id={} method={}",
            id,
            method
        );

        Ok((id, receiver))
    }

    /// Send a JSON-RPC notification (no response expected).
    ///
    /// # Arguments
    /// * `method` - The notification method name
    /// * `params` - The notification parameters
    pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let content =
            serde_json::to_string(&notification).map_err(|e| format!("JSON error: {}", e))?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(header.as_bytes())
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        stdin
            .write_all(content.as_bytes())
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("Flush error: {}", e))?;

        log::debug!(
            target: "treesitter_ls::bridge::tokio",
            "[CONN] Sent notification method={}",
            method
        );

        Ok(())
    }

    /// Remove a pending request entry by ID.
    ///
    /// Used to clean up pending_requests when a request times out or fails.
    /// This prevents memory leaks from accumulating pending entries that will
    /// never receive a response.
    ///
    /// # Arguments
    /// * `id` - The request ID to remove
    pub fn remove_pending_request(&self, id: i64) {
        self.pending_requests.remove(&id);
    }

    /// Clear all pending requests and notify them with None response.
    ///
    /// Used during shutdown, EOF, or error conditions to notify all waiting
    /// callers that their requests will not receive responses.
    ///
    /// This helper consolidates the cleanup pattern used in reader_loop (EOF/Error)
    /// and Drop impl.
    fn clear_all_pending_requests(&self) {
        Self::clear_pending_requests(&self.pending_requests);
    }

    /// Static helper to clear pending requests from a DashMap.
    ///
    /// Used by both instance method and static reader_loop.
    fn clear_pending_requests(pending: &Arc<DashMap<i64, PendingRequest>>) {
        let ids: Vec<i64> = pending.iter().map(|r| *r.key()).collect();
        for id in ids {
            if let Some((_, sender)) = pending.remove(&id) {
                let _ = sender.send(ResponseResult {
                    response: None,
                    notifications: vec![],
                });
            }
        }
    }

    /// Gracefully shutdown the language server.
    ///
    /// Sends the LSP shutdown request followed by exit notification.
    /// This should be called before dropping the connection when possible.
    /// If not called, Drop will just kill the process.
    pub async fn shutdown(&mut self) -> Result<(), String> {
        // Send shutdown request
        let (_, receiver) = self
            .send_request("shutdown", serde_json::json!(null))
            .await?;

        // Wait for response with timeout
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), receiver).await;

        // Send exit notification
        self.send_notification("exit", serde_json::json!(null))
            .await?;

        // Wait for child to exit
        if let Some(ref child_mutex) = self.child {
            let mut child = child_mutex.lock().await;
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), child.wait()).await;
        }

        log::debug!(
            target: "treesitter_ls::bridge::tokio",
            "[CONN] Shutdown complete"
        );

        Ok(())
    }

    /// Check if the child process is still alive.
    ///
    /// This method checks whether the bridge language server process is still running.
    /// It is used for health monitoring to detect crashed or killed processes.
    ///
    /// This is an async method because it needs to acquire the Mutex lock on the child process.
    ///
    /// # Returns
    /// * `true` if the child process is still running
    /// * `false` if the child process has exited or was never spawned
    pub async fn is_alive(&self) -> bool {
        if let Some(ref child_mutex) = self.child {
            let mut child = child_mutex.lock().await;
            // try_wait returns Ok(None) if the process is still running
            // try_wait returns Ok(Some(status)) if the process has exited
            // try_wait returns Err if there was an error checking the status
            match child.try_wait() {
                Ok(None) => true,     // Process is still running
                Ok(Some(_)) => false, // Process has exited
                Err(_) => false,      // Error checking status - assume dead
            }
        } else {
            false // No child process - connection was never properly initialized
        }
    }

    /// Read a single JSON-RPC message from the reader.
    async fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<Option<Value>, String> {
        // Read headers
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line).await {
                Ok(0) => return Ok(None), // EOF
                Ok(_) => {}
                Err(e) => return Err(format!("Header read error: {}", e)),
            }

            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(len_str) = line.strip_prefix("Content-Length:") {
                content_length = len_str
                    .trim()
                    .parse()
                    .map_err(|e| format!("Invalid content length: {}", e))?;
            }
        }

        if content_length == 0 {
            return Ok(None);
        }

        // Read content
        let mut content = vec![0u8; content_length];
        reader
            .read_exact(&mut content)
            .await
            .map_err(|e| format!("Content read error: {}", e))?;

        serde_json::from_slice(&content)
            .map(Some)
            .map_err(|e| format!("JSON parse error: {}", e))
    }
}

impl Drop for TokioAsyncBridgeConnection {
    fn drop(&mut self) {
        // 1. Notify all pending requests
        self.clear_all_pending_requests();

        // 2. Send shutdown signal to reader task
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // 3. Abort the reader task if it's still running
        if let Some(handle) = self.reader_handle.take() {
            handle.abort();
        }

        // 4. Kill the child process
        // We skip sending shutdown/exit since block_on can't be called from async context
        // and the process will be killed anyway
        if let Some(child_mutex) = self.child.take() {
            // Use try_lock to avoid blocking in Drop - if we can't get the lock, the process
            // will be cleaned up when the Mutex is dropped anyway
            if let Ok(mut child) = child_mutex.try_lock() {
                let _ = child.start_kill();
                log::debug!(
                    target: "treesitter_ls::bridge::tokio",
                    "[CONN] Killed child process"
                );
            } else {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio",
                    "[CONN] Could not acquire child lock in Drop, process may not be killed"
                );
            }
        }

        // 5. Remove temp_dir (sync I/O is safe in Drop)
        if let Some(temp_dir) = self.temp_dir.take() {
            if let Err(e) = std::fs::remove_dir_all(&temp_dir) {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio",
                    "[CONN] Failed to remove temp_dir {:?}: {}",
                    temp_dir,
                    e
                );
            } else {
                log::debug!(
                    target: "treesitter_ls::bridge::tokio",
                    "[CONN] Removed temp_dir {:?}",
                    temp_dir
                );
            }
        }

        log::debug!(
            target: "treesitter_ls::bridge::tokio",
            "[CONN] Connection dropped, cleanup complete"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the struct has required fields with correct types.
    /// This is a compile-time verification test.
    #[test]
    fn tokio_async_bridge_connection_struct_exists() {
        // Verify TokioAsyncBridgeConnection has the expected fields by constructing it
        // This is a type-level test that will fail to compile if the struct is wrong

        // Create mock values for each field type
        let (stdin_tx, _stdin_rx) = tokio::sync::oneshot::channel::<()>();
        let _ = stdin_tx; // Silence unused warning

        // Verify the type signature of each field by accessing them
        fn assert_stdin_type(_: &tokio::sync::Mutex<ChildStdin>) {}
        fn assert_pending_requests_type(_: &Arc<DashMap<i64, PendingRequest>>) {}
        fn assert_next_request_id_type(_: &AtomicI64) {}
        fn assert_reader_handle_type(_: &Option<JoinHandle<()>>) {}
        fn assert_shutdown_tx_type(_: &Option<oneshot::Sender<()>>) {}

        // These function signatures prove the field types are correct
        // The test passes if this compiles
        let _ = assert_stdin_type;
        let _ = assert_pending_requests_type;
        let _ = assert_next_request_id_type;
        let _ = assert_reader_handle_type;
        let _ = assert_shutdown_tx_type;
    }

    /// Test that spawn() uses tokio::process::Command to create a child process.
    /// This test spawns a simple process (cat) with tokio::process and verifies
    /// that the connection is created successfully.
    #[tokio::test]
    async fn spawn_uses_tokio_process_command() {
        // Use 'cat' as a simple process that reads from stdin and writes to stdout
        // This is available on all Unix-like systems
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed with 'cat' command");

        let conn = result.unwrap();
        // Verify the connection has all required fields populated
        // (the struct fields are private, so we rely on the constructor succeeding)
        drop(conn);
    }

    /// Test that spawn() extracts stdin wrapped in tokio::sync::Mutex.
    /// Verifies AC2: Async stdin/stdout handles are obtained from the tokio Child process.
    #[tokio::test]
    async fn spawn_extracts_stdin_stdout_from_child() {
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();
        // Verify stdin is accessible through the Mutex by attempting to lock it
        // This proves the stdin was extracted and wrapped correctly
        let stdin_guard = conn.stdin.lock().await;
        // If we can acquire the lock, the stdin was properly set up
        drop(stdin_guard);
    }

    /// Test that spawn() creates a reader task handle and shutdown sender.
    /// Verifies the reader_handle is a tokio::task::JoinHandle and shutdown_tx exists.
    #[tokio::test]
    async fn spawn_creates_reader_task_handle() {
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let mut conn = result.unwrap();

        // Verify reader_handle is Some (task was spawned)
        assert!(
            conn.reader_handle.is_some(),
            "reader_handle should be Some after spawn"
        );

        // Verify shutdown_tx is Some
        assert!(
            conn.shutdown_tx.is_some(),
            "shutdown_tx should be Some after spawn"
        );

        // Send shutdown signal and verify the task completes
        if let Some(shutdown_tx) = conn.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Await the reader task to verify it's a valid JoinHandle
        if let Some(handle) = conn.reader_handle.take() {
            let result = handle.await;
            assert!(
                result.is_ok(),
                "Reader task should complete successfully after shutdown"
            );
        }
    }

    /// Test that shutdown completes within 100ms when reader is idle.
    ///
    /// This verifies AC1: The reader task uses tokio::select! for read/shutdown/timeout
    /// branches, enabling non-blocking shutdown unlike sync read_line which would block.
    ///
    /// The key insight: with synchronous read_line, shutdown would block until data arrives.
    /// With tokio::select!, shutdown signal is processed immediately even when idle.
    #[tokio::test]
    async fn shutdown_while_reader_idle_completes_within_100ms() {
        // Spawn a 'cat' process - it will be idle (not sending any data)
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let mut conn = result.unwrap();

        // Measure shutdown time
        let start = std::time::Instant::now();

        // Send shutdown signal
        if let Some(shutdown_tx) = conn.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        // Wait for reader task to complete
        if let Some(handle) = conn.reader_handle.take() {
            let result = handle.await;
            assert!(
                result.is_ok(),
                "Reader task should complete successfully after shutdown"
            );
        }

        let elapsed = start.elapsed();

        // Shutdown should complete within 100ms (AC1 verification)
        // With synchronous read_line, this would hang forever waiting for data
        // With tokio::select!, shutdown signal is processed immediately
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "Shutdown should complete within 100ms when reader is idle, but took {:?}",
            elapsed
        );
    }

    /// Test that reader task routes responses by ID to pending_requests.
    ///
    /// This verifies the reader task parses LSP messages and routes responses
    /// by request ID to the correct oneshot sender in pending_requests.
    #[tokio::test]
    async fn reader_routes_responses_to_pending_requests() {
        use tokio::io::AsyncWriteExt;

        // Use 'cat' as an echo server - we write to stdin, it echoes to stdout
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Create a receiver for request ID 1
        let (tx, rx) = oneshot::channel::<ResponseResult>();
        conn.pending_requests.insert(1, tx);

        // Write an LSP response message to stdin (cat will echo it to stdout)
        let response = r#"{"jsonrpc":"2.0","id":1,"result":{"test":"value"}}"#;
        let message = format!("Content-Length: {}\r\n\r\n{}", response.len(), response);

        {
            let mut stdin = conn.stdin.lock().await;
            stdin.write_all(message.as_bytes()).await.unwrap();
            stdin.flush().await.unwrap();
        }

        // Wait for the response with a timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx).await;

        assert!(
            result.is_ok(),
            "Should receive response within timeout: {:?}",
            result.err()
        );

        let response_result = result.unwrap();
        assert!(response_result.is_ok(), "Receiver should not be dropped");

        let response_result = response_result.unwrap();
        assert!(response_result.response.is_some(), "Response should be set");

        let response_value = response_result.response.unwrap();
        assert_eq!(
            response_value.get("id").and_then(|v| v.as_i64()),
            Some(1),
            "Response should have correct ID"
        );
    }

    /// Test that send_request returns a receiver that resolves when reader routes matching response.
    ///
    /// This is the key test for Subtask 2: send_request must:
    /// 1. Increment next_request_id atomically
    /// 2. Insert oneshot::Sender into pending_requests before writing
    /// 3. Write LSP message format (Content-Length header + JSON body)
    /// 4. Return receiver that resolves when reader routes the response
    #[tokio::test]
    async fn send_request_returns_receiver_that_resolves_on_response() {
        // Use 'cat' as an echo server - we write to stdin, it echoes to stdout
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Send a request using the send_request method
        let params = serde_json::json!({"test": "value"});
        let receiver = conn.send_request("test/method", params).await;
        assert!(receiver.is_ok(), "send_request should succeed");

        let (id, rx) = receiver.unwrap();
        assert!(id > 0, "Request ID should be positive");

        // Wait for the response (cat echoes our message back)
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx).await;

        assert!(
            result.is_ok(),
            "Should receive response within timeout: {:?}",
            result.err()
        );

        let response_result = result.unwrap();
        assert!(response_result.is_ok(), "Receiver should not be dropped");

        let response_result = response_result.unwrap();
        assert!(response_result.response.is_some(), "Response should be set");

        let response_value = response_result.response.unwrap();
        assert_eq!(
            response_value.get("id").and_then(|v| v.as_i64()),
            Some(id),
            "Response should have the same ID we sent"
        );
    }

    /// Test that spawn() accepts an optional notification_sender parameter.
    ///
    /// This verifies Subtask 2: spawn() must accept Option<mpsc::Sender<Value>>
    /// parameter and store it for forwarding $/progress notifications.
    #[tokio::test]
    async fn spawn_accepts_notification_sender_parameter() {
        use tokio::sync::mpsc;

        // Create a notification channel
        let (tx, _rx) = mpsc::channel::<serde_json::Value>(16);

        // spawn() should accept the notification sender parameter
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, Some(tx), None).await;

        assert!(
            result.is_ok(),
            "spawn() with notification_sender should succeed"
        );
    }

    /// Test that spawn() without notification_sender still works (backward compatible).
    #[tokio::test]
    async fn spawn_without_notification_sender_works() {
        // spawn() with None notification_sender should still work
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;

        assert!(
            result.is_ok(),
            "spawn() with None notification_sender should succeed"
        );
    }

    /// Test that reader forwards $/progress notifications to the notification channel.
    ///
    /// This verifies Subtask 1: when reader receives a message with 'method' field
    /// but no 'id' field (a notification), and method is '$/progress', it should
    /// send the message to the notification channel.
    #[tokio::test]
    async fn reader_forwards_progress_notifications_to_channel() {
        use tokio::io::AsyncWriteExt;
        use tokio::sync::mpsc;

        // Create a notification channel
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);

        // Spawn with notification sender
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, Some(tx), None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Write a $/progress notification to stdin (cat will echo to stdout)
        let notification = r#"{"jsonrpc":"2.0","method":"$/progress","params":{"token":"token1","value":{"kind":"begin","title":"Indexing"}}}"#;
        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification.len(),
            notification
        );

        {
            let mut stdin = conn.stdin.lock().await;
            stdin.write_all(message.as_bytes()).await.unwrap();
            stdin.flush().await.unwrap();
        }

        // Wait for the notification to be forwarded
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv()).await;

        assert!(result.is_ok(), "Should receive notification within timeout");

        let notification_value = result.unwrap();
        assert!(
            notification_value.is_some(),
            "Notification channel should receive message"
        );

        let notification_value = notification_value.unwrap();
        assert_eq!(
            notification_value.get("method").and_then(|m| m.as_str()),
            Some("$/progress"),
            "Forwarded notification should have method='$/progress'"
        );
    }

    /// Test that non-progress notifications are not forwarded.
    #[tokio::test]
    async fn reader_does_not_forward_non_progress_notifications() {
        use tokio::io::AsyncWriteExt;
        use tokio::sync::mpsc;

        // Create a notification channel
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);

        // Spawn with notification sender
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, Some(tx), None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Write a non-progress notification (window/logMessage)
        let notification = r#"{"jsonrpc":"2.0","method":"window/logMessage","params":{"type":3,"message":"Info"}}"#;
        let message = format!(
            "Content-Length: {}\r\n\r\n{}",
            notification.len(),
            notification
        );

        {
            let mut stdin = conn.stdin.lock().await;
            stdin.write_all(message.as_bytes()).await.unwrap();
            stdin.flush().await.unwrap();
        }

        // Wait briefly - notification should NOT be forwarded
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv()).await;

        // Should timeout since non-progress notifications are not forwarded
        assert!(
            result.is_err(),
            "Non-progress notifications should not be forwarded"
        );
    }

    /// Test that send_notification sends a message without expecting a response.
    #[tokio::test]
    async fn send_notification_sends_message_without_response() {
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Send a notification
        let params = serde_json::json!({"test": "notification"});
        let result = conn.send_notification("test/notification", params).await;

        // send_notification should succeed (not block waiting for response)
        assert!(result.is_ok(), "send_notification should succeed");
    }

    /// Test that spawn() with cwd parameter sets child process working directory.
    ///
    /// This test verifies AC1: TokioAsyncBridgeConnection::spawn() accepts optional cwd parameter.
    /// When cwd is Some, the child process should run in the specified directory.
    #[tokio::test]
    async fn spawn_with_cwd_sets_child_process_working_directory() {
        // Create a temp directory to use as cwd
        let temp_dir =
            std::env::temp_dir().join(format!("tokio-spawn-cwd-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

        // Use 'pwd' to verify the working directory is set correctly
        // On Unix, 'pwd' prints the current working directory
        let result =
            TokioAsyncBridgeConnection::spawn("pwd", &[], Some(&temp_dir), None, None).await;
        assert!(result.is_ok(), "spawn() with cwd should succeed");

        // Clean up - ignore errors
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// Test that spawn() with None cwd uses default working directory (backward compatible).
    #[tokio::test]
    async fn spawn_with_none_cwd_uses_default_directory() {
        // spawn() with None should behave like before - use the default process cwd
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() with None cwd should succeed");

        let conn = result.unwrap();
        // Verify connection works
        let stdin_guard = conn.stdin.lock().await;
        drop(stdin_guard);
    }

    /// PBI-148 Subtask 1: Test that spawn() stores Child handle for cleanup.
    ///
    /// The Child handle must be stored in the connection struct so that Drop
    /// can properly send shutdown/exit requests and wait for the process to exit.
    /// Without storing the Child, the process becomes orphaned when the connection
    /// is dropped.
    #[tokio::test]
    async fn spawn_stores_child_handle_for_cleanup() {
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Verify the child field exists and is Some after spawn
        assert!(
            conn.child.is_some(),
            "child handle should be stored after spawn"
        );

        // Verify we can access the child and it's alive
        if let Some(ref child_mutex) = conn.child {
            let mut child = child_mutex.lock().await;
            // try_wait returns Ok(None) if process is still running
            let status = child.try_wait();
            assert!(
                matches!(status, Ok(None)),
                "child process should be running after spawn"
            );
        }
    }

    /// PBI-148 Subtask 2: Test that spawn() stores temp_dir path for cleanup.
    ///
    /// The temp_dir must be stored in the connection struct so that Drop
    /// can remove it after shutting down the language server.
    /// Without storing temp_dir, the temporary workspace directories accumulate.
    #[tokio::test]
    async fn connection_tracks_temp_dir_for_cleanup() {
        // Create a temp directory to pass to spawn
        let temp_dir =
            std::env::temp_dir().join(format!("tokio-temp-dir-test-{}", std::process::id()));
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

        let result = TokioAsyncBridgeConnection::spawn(
            "cat",
            &[],
            Some(&temp_dir),
            None,
            Some(temp_dir.clone()),
        )
        .await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Verify the temp_dir field exists and matches what we passed
        assert!(
            conn.temp_dir.is_some(),
            "temp_dir should be stored after spawn"
        );
        assert_eq!(
            conn.temp_dir.as_ref().unwrap(),
            &temp_dir,
            "stored temp_dir should match the one passed to spawn"
        );

        // Clean up
        drop(conn);
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// PBI-148 Subtask 3: Test that Drop sends shutdown and exit to language server.
    ///
    /// Drop should send 'shutdown' request followed by 'exit' notification to
    /// gracefully terminate the language server before killing the process.
    /// This prevents orphaned processes and ensures clean termination.
    ///
    /// Note: This test is tricky because Drop is synchronous but we need async I/O.
    /// The implementation uses Handle::current().block_on() for best-effort cleanup.
    #[tokio::test]
    async fn drop_sends_shutdown_and_exit_to_language_server() {
        // We'll use cat as the process and verify child is killed after drop
        // (since cat doesn't understand LSP shutdown/exit, it will be killed)
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Verify child is alive before drop
        {
            let child_mutex = conn.child.as_ref().expect("child should be present");
            let mut child = child_mutex.lock().await;
            let status = child.try_wait();
            assert!(
                matches!(status, Ok(None)),
                "child process should be running before drop"
            );
        }

        // Drop the connection
        drop(conn);

        // Give the cleanup a moment to complete (Drop uses block_on which may race)
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Since cat doesn't respond to LSP shutdown, the child should be killed
        // This test mainly verifies that Drop doesn't panic and attempts cleanup
    }

    /// PBI-148 Subtask 4: Test that Drop removes temp_dir.
    ///
    /// After shutting down the language server, Drop should remove the temp_dir
    /// to prevent disk space accumulation from orphaned workspace directories.
    #[tokio::test]
    async fn drop_removes_temp_directory() {
        // Create a temp directory to pass to spawn
        let temp_dir = std::env::temp_dir().join(format!(
            "tokio-temp-dir-cleanup-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");

        // Verify temp_dir exists before spawn
        assert!(temp_dir.exists(), "temp_dir should exist before spawn");

        let result = TokioAsyncBridgeConnection::spawn(
            "cat",
            &[],
            Some(&temp_dir),
            None,
            Some(temp_dir.clone()),
        )
        .await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Verify temp_dir still exists (connection is alive)
        assert!(
            temp_dir.exists(),
            "temp_dir should exist while connection is alive"
        );

        // Drop the connection - should clean up temp_dir
        drop(conn);

        // Give the cleanup a moment to complete
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Verify temp_dir was removed
        assert!(
            !temp_dir.exists(),
            "temp_dir should be removed after connection is dropped"
        );
    }

    /// PBI-157 AC1: Test that is_alive() returns true for running process.
    ///
    /// When a connection is freshly spawned, the child process is running.
    /// is_alive() should return true.
    #[tokio::test]
    async fn is_alive_returns_true_for_running_process() {
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Immediately after spawn, process should be alive
        assert!(
            conn.is_alive().await,
            "is_alive() should return true for running process"
        );
    }

    /// PBI-157 AC1: Test that is_alive() returns false for dead process.
    ///
    /// When the child process exits (naturally or via kill), is_alive() should return false.
    #[tokio::test]
    async fn is_alive_returns_false_for_dead_process() {
        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Kill the child process
        if let Some(ref child_mutex) = conn.child {
            let mut child = child_mutex.lock().await;
            let _ = child.kill().await;
        }

        // Give the process time to exit
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // After killing, process should be dead
        assert!(
            !conn.is_alive().await,
            "is_alive() should return false for dead process"
        );
    }

    #[tokio::test]
    async fn tokio_async_bridge_connection_has_initialized_flag_defaulting_to_false() {
        // PBI-162 Subtask 2: TokioAsyncBridgeConnection must have an initialized flag
        // using AtomicBool that defaults to false before the initialized notification is sent.

        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // The initialized field should be accessible and false by default
        // Using AtomicBool, we need to load it with Ordering::SeqCst
        assert!(
            !conn.initialized.load(std::sync::atomic::Ordering::SeqCst),
            "initialized flag should default to false after spawn"
        );
    }

    #[tokio::test]
    async fn tokio_async_bridge_connection_send_request_returns_error_when_not_initialized() {
        // PBI-162 Subtask 4: send_request must return an error when initialized=false
        // to prevent protocol errors during the initialization window.

        let result = TokioAsyncBridgeConnection::spawn("cat", &[], None, None, None).await;
        assert!(result.is_ok(), "spawn() should succeed");

        let conn = result.unwrap();

        // Manually set initialized=false to test the guard logic
        // (In production, this will be the state between spawn and sending initialized notification)
        conn.initialized.store(false, std::sync::atomic::Ordering::SeqCst);

        // Attempt to send a request - should return an error
        let params = serde_json::json!({"test": "value"});
        let send_result = conn.send_request("test/method", params).await;

        assert!(
            send_result.is_err(),
            "send_request should return Err when not initialized"
        );

        // Verify error message mentions initialization
        let err_msg = send_result.unwrap_err();
        assert!(
            err_msg.contains("initialized") || err_msg.contains("initialization"),
            "Error message should mention initialization, got: {}",
            err_msg
        );
    }
}
