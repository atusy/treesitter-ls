//! LSP client for E2E tests.
//!
//! Provides a simple LSP client that communicates with kakehashi binary
//! via stdin/stdout using JSON-RPC 2.0 protocol.

// These methods are shared across multiple test binaries but not all tests use every method.
// Allow dead_code to suppress per-binary warnings.
#![allow(dead_code)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// LSP client for communicating with kakehashi binary.
///
/// Handles JSON-RPC 2.0 message framing with Content-Length headers,
/// request/response matching, and server-initiated notifications.
pub struct LspClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    request_id: i64,
}

impl LspClient {
    /// Spawn the kakehashi binary and create a new LSP client.
    pub fn new() -> Self {
        Self::with_debug(false)
    }

    /// Spawn the kakehashi binary with optional debug logging.
    pub fn with_debug(debug: bool) -> Self {
        // `CARGO_BIN_EXE_kakehashi` is set by Cargo's test harness for integration tests
        // and points to the built `kakehashi` binary, so we don't hardcode its path here.
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kakehashi"));
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if debug {
            cmd.env("RUST_LOG", "debug");
        }

        let mut child = cmd.spawn().expect("Failed to spawn kakehashi binary");

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = BufReader::new(child.stdout.take().expect("Failed to get stdout"));
        let stderr = child.stderr.take();

        Self {
            child,
            stdin: Some(stdin),
            stdout,
            stderr,
            request_id: 0,
        }
    }

    /// Read stderr output (single read, up to 4KB).
    ///
    /// Note: This may block if no data is available. Call after the server
    /// has had time to produce output. Useful for debugging server behavior.
    pub fn drain_stderr(&mut self) -> String {
        use std::io::Read;
        let Some(stderr) = self.stderr.as_mut() else {
            return String::new();
        };

        let mut buf = [0u8; 4096];
        match stderr.read(&mut buf) {
            Ok(0) => String::new(),
            Ok(n) => String::from_utf8_lossy(&buf[..n]).to_string(),
            Err(_) => String::new(),
        }
    }

    /// Send an LSP request and return the response.
    pub fn send_request(&mut self, method: &str, params: Value) -> Value {
        self.request_id += 1;
        let request_id = self.request_id;

        // Build request - some methods like "shutdown" don't take params
        let mut request = serde_json::Map::new();
        request.insert("jsonrpc".to_string(), json!("2.0"));
        request.insert("id".to_string(), json!(request_id));
        request.insert("method".to_string(), json!(method));

        // Only add params if it's not null
        if !params.is_null() {
            request.insert("params".to_string(), params);
        }

        self.send_message(&Value::Object(request));
        self.receive_response_for_id(request_id)
    }

    /// Send an LSP notification (no response expected).
    pub fn send_notification(&mut self, method: &str, params: Value) {
        // Build notification - some methods don't take params
        let mut notification = serde_json::Map::new();
        notification.insert("jsonrpc".to_string(), json!("2.0"));
        notification.insert("method".to_string(), json!(method));

        // Only add params if it's not null
        if !params.is_null() {
            notification.insert("params".to_string(), params);
        }

        self.send_message(&Value::Object(notification));
    }

    /// Send a JSON-RPC message with Content-Length header.
    fn send_message(&mut self, message: &Value) {
        let body = serde_json::to_string(message).expect("Failed to serialize message");
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin
            .write_all(header.as_bytes())
            .expect("Failed to write header");
        stdin
            .write_all(body.as_bytes())
            .expect("Failed to write body");
        stdin.flush().expect("Failed to flush stdin");
    }

    /// Receive an LSP response for a specific request id.
    /// Skips server-initiated notifications and requests until finding matching response.
    /// Times out after 30 seconds or 1000 messages to prevent indefinite hangs.
    fn receive_response_for_id(&mut self, expected_id: i64) -> Value {
        const MAX_MESSAGES: u32 = 1000;
        const TIMEOUT: Duration = Duration::from_secs(30);

        let start_time = Instant::now();
        let mut message_count = 0u32;

        loop {
            // Check timeout
            if start_time.elapsed() > TIMEOUT {
                panic!(
                    "Timeout waiting for response with id {}. Elapsed: {:?}",
                    expected_id,
                    start_time.elapsed()
                );
            }

            // Check message count threshold
            if message_count >= MAX_MESSAGES {
                panic!(
                    "Exceeded maximum message threshold ({}) waiting for response with id {}",
                    MAX_MESSAGES, expected_id
                );
            }

            let message = self.receive_message();
            message_count += 1;

            // Check if this is a response to our request
            if let Some(id) = message.get("id") {
                // Server-to-client requests have "method" field, skip them
                if message.get("method").is_some() {
                    continue;
                }
                // Response should match our request id
                if id.as_i64() == Some(expected_id) {
                    return message;
                }
            }
            // Otherwise it's a notification like window/logMessage, skip it
        }
    }

    /// Receive a single LSP message with Content-Length framing.
    /// Times out after 30 seconds to prevent indefinite blocking on unresponsive servers.
    /// Validates Content-Length to prevent excessive memory allocation.
    fn receive_message(&mut self) -> Value {
        const MAX_HEADERS: u32 = 100;
        const TIMEOUT: Duration = Duration::from_secs(30);
        const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024; // 100MB limit

        let start_time = Instant::now();
        let mut header_count = 0u32;

        // Read Content-Length header
        let mut header = String::new();
        loop {
            // Check timeout
            if start_time.elapsed() > TIMEOUT {
                panic!(
                    "Timeout reading LSP message header. Elapsed: {:?}",
                    start_time.elapsed()
                );
            }

            // Check header count threshold
            if header_count >= MAX_HEADERS {
                panic!(
                    "Exceeded maximum header count ({}) - server may be sending malformed headers",
                    MAX_HEADERS
                );
            }

            header.clear();
            let bytes_read = self
                .stdout
                .read_line(&mut header)
                .expect("Failed to read header line");

            // EOF detection: if read_line returns 0 bytes, the connection closed
            if bytes_read == 0 {
                panic!("Server closed connection prematurely while reading header");
            }

            header_count += 1;

            if header == "\r\n" {
                continue;
            }

            if header.starts_with("Content-Length:") {
                let len: usize = header
                    .trim_start_matches("Content-Length:")
                    .trim()
                    .parse()
                    .expect("Invalid Content-Length value");

                // Validate Content-Length to prevent excessive allocations
                if len > MAX_MESSAGE_SIZE {
                    panic!(
                        "Content-Length {} exceeds maximum allowed size {}",
                        len, MAX_MESSAGE_SIZE
                    );
                }

                if len == 0 {
                    panic!("Invalid Content-Length: 0 - message body cannot be empty");
                }

                // Read empty line
                let mut empty = String::new();
                let bytes_read = self
                    .stdout
                    .read_line(&mut empty)
                    .expect("Failed to read empty line");

                if bytes_read == 0 {
                    panic!("Server closed connection prematurely after header");
                }

                // Read body
                let mut body = vec![0u8; len];
                std::io::Read::read_exact(&mut self.stdout, &mut body)
                    .expect("Failed to read body");

                return serde_json::from_slice(&body).expect("Failed to parse response");
            }
        }
    }

    /// Kill the server process.
    fn kill(&mut self) {
        // If the child has already exited, nothing to do.
        match self.child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => {
                // Child is still running; try to terminate it.
                let _ = self.child.kill();
            }
            Err(_) => {
                // If we can't query status, still attempt to kill as best effort.
                let _ = self.child.kill();
            }
        }

        // Reap the process to avoid leaving a zombie. Ignore errors in Drop path.
        let _ = self.child.wait();
    }

    /// Close stdin to signal EOF (for shutdown testing).
    pub(crate) fn close_stdin(&mut self) {
        self.stdin = None;
    }

    /// Get the current request ID counter (for cancel testing).
    pub(crate) fn current_request_id(&self) -> i64 {
        self.request_id
    }

    /// Send an LSP request without waiting for response.
    ///
    /// Returns the request ID used. Caller is responsible for receiving the response
    /// via `receive_response_for_id_public()`.
    pub(crate) fn send_request_async(&mut self, method: &str, params: Value) -> i64 {
        self.request_id += 1;
        let request_id = self.request_id;

        let mut request = serde_json::Map::new();
        request.insert("jsonrpc".to_string(), json!("2.0"));
        request.insert("id".to_string(), json!(request_id));
        request.insert("method".to_string(), json!(method));

        if !params.is_null() {
            request.insert("params".to_string(), params);
        }

        self.send_message(&Value::Object(request));
        request_id
    }

    /// Receive response for a specific request ID (public version).
    pub(crate) fn receive_response_for_id_public(&mut self, expected_id: i64) -> Value {
        self.receive_response_for_id(expected_id)
    }

    /// Check if the server process is still running.
    pub(crate) fn is_running(&mut self) -> bool {
        self.child
            .try_wait()
            .expect("Error checking child status")
            .is_none()
    }

    /// Wait for the process to exit with a timeout.
    /// Returns the exit status if the process exited, or None if timeout occurred.
    pub(crate) fn wait_for_exit(&mut self, timeout: Duration) -> Option<std::process::ExitStatus> {
        let start = Instant::now();
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_client_can_initialize() {
        let mut client = LspClient::new();

        let response = client.send_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {}
            }),
        );

        assert!(response.get("result").is_some());
        assert!(response["result"].get("capabilities").is_some());
    }
}
