//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.

use std::io;
use std::process::Stdio;
use std::sync::Arc;

use dashmap::DashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;

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
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to capture stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to capture stdout"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Get the process ID of the child process.
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
        serde_json::from_slice(&body)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Read a raw LSP message from the child process stdout.
    ///
    /// Returns the full message including header and body as a string.
    /// Used primarily for testing to verify message format.
    #[cfg(test)]
    async fn read_raw_message(&mut self) -> io::Result<String> {
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

/// Request ID type for JSON-RPC messages.
///
/// LSP spec allows either integer or string IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RequestId {
    Int(i64),
    String(String),
}

impl RequestId {
    /// Extract request ID from a JSON-RPC message.
    fn from_json(value: &serde_json::Value) -> Option<Self> {
        match &value["id"] {
            serde_json::Value::Number(n) => n.as_i64().map(RequestId::Int),
            serde_json::Value::String(s) => Some(RequestId::String(s.clone())),
            _ => None,
        }
    }
}

/// Tracks pending requests waiting for responses.
///
/// Uses `DashMap` for concurrent access from writer and reader tasks.
/// Each pending request is associated with a `oneshot::Sender` to deliver
/// the response back to the caller.
#[derive(Clone)]
pub(crate) struct PendingRequests {
    inner: Arc<DashMap<RequestId, oneshot::Sender<serde_json::Value>>>,
}

impl PendingRequests {
    /// Create a new pending request tracker.
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Register a pending request and return a receiver for the response.
    ///
    /// Returns a tuple of (response_receiver, request_id).
    pub(crate) fn register(&self, id: i64) -> (oneshot::Receiver<serde_json::Value>, RequestId) {
        let request_id = RequestId::Int(id);
        let (tx, rx) = oneshot::channel();
        self.inner.insert(request_id.clone(), tx);
        (rx, request_id)
    }

    /// Complete a pending request by routing the response to its sender.
    ///
    /// Extracts the request ID from the response and sends it to the
    /// corresponding pending request, if one exists.
    pub(crate) fn complete(&self, response: &serde_json::Value) {
        if let Some(id) = RequestId::from_json(response) {
            if let Some((_, sender)) = self.inner.remove(&id) {
                // Ignore send error - receiver may have been dropped
                let _ = sender.send(response.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_exports_async_bridge_connection_type() {
        // Verify the type exists and is accessible
        fn assert_type_exists<T>() {}
        assert_type_exists::<AsyncBridgeConnection>();
    }

    #[tokio::test]
    async fn spawn_creates_child_process_with_stdio() {
        // Use `cat` as a simple test process that echoes stdin to stdout
        let cmd = vec!["cat".to_string()];
        let conn = AsyncBridgeConnection::spawn(cmd).await.expect("spawn should succeed");

        // The connection should have a child process ID
        assert!(conn.child_id().is_some(), "child process should have an ID");
    }

    /// RED: Test that send_request writes JSON-RPC message with Content-Length header
    #[tokio::test]
    async fn send_request_writes_json_rpc_with_content_length() {
        use serde_json::json;

        // Use `cat` to echo what we write to stdin back to stdout
        let cmd = vec!["cat".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd).await.expect("spawn should succeed");

        // Send a simple JSON-RPC request
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        conn.write_message(&request).await.expect("write should succeed");

        // Read back what was written to verify the format
        let output = conn.read_raw_message().await.expect("read should succeed");

        // Verify Content-Length header is present and correct
        assert!(
            output.starts_with("Content-Length: "),
            "message should start with Content-Length header"
        );
        assert!(
            output.contains("\r\n\r\n"),
            "header should be separated from body by CRLF CRLF"
        );
        assert!(
            output.contains("\"jsonrpc\":\"2.0\""),
            "body should contain JSON-RPC content"
        );
    }

    /// RED: Test that read_message parses Content-Length header and reads JSON body
    #[tokio::test]
    async fn read_message_parses_content_length_and_body() {
        use serde_json::json;

        // Use `cat` to echo what we write back to us
        let cmd = vec!["cat".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd).await.expect("spawn should succeed");

        // Write a JSON-RPC response message
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {}
            }
        });

        conn.write_message(&response).await.expect("write should succeed");

        // Read it back using the reader task's parsing logic
        let parsed = conn.read_message().await.expect("read should succeed");

        // Verify the parsed message matches what we sent
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert!(parsed["result"].is_object());
    }

    /// RED: Test that response is routed to correct pending request via request ID
    #[tokio::test]
    async fn response_routed_to_pending_request_by_id() {
        use serde_json::json;
        use std::sync::Arc;

        // Use `cat` to echo what we write back
        let cmd = vec!["cat".to_string()];
        let conn = AsyncBridgeConnection::spawn(cmd).await.expect("spawn should succeed");

        // Wrap in Arc for sharing between reader task and main task
        let conn = Arc::new(tokio::sync::Mutex::new(conn));

        // Create a pending request tracker
        let pending = PendingRequests::new();

        // Register a pending request with ID 42
        let (response_rx, _request_id) = pending.register(42);

        // Spawn a "reader task" that reads a response and routes it
        let conn_clone = Arc::clone(&conn);
        let pending_clone = pending.clone();
        let reader_task = tokio::spawn(async move {
            let mut conn = conn_clone.lock().await;
            let response = conn.read_message().await.expect("read should succeed");
            pending_clone.complete(&response);
        });

        // Write a response with matching ID (simulate language server response)
        {
            let mut conn = conn.lock().await;
            let response = json!({
                "jsonrpc": "2.0",
                "id": 42,
                "result": { "value": "hello" }
            });
            conn.write_message(&response).await.expect("write should succeed");
        }

        // Wait for reader task
        reader_task.await.expect("reader task should complete");

        // The pending request should receive the response
        let result = response_rx.await.expect("should receive response");
        assert_eq!(result["id"], 42);
        assert_eq!(result["result"]["value"], "hello");
    }
}
