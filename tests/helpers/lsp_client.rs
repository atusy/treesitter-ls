//! LSP client for E2E tests.
//!
//! Provides a simple LSP client that communicates with kakehashi binary
//! via stdin/stdout using JSON-RPC 2.0 protocol.

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
    request_id: i64,
}

impl LspClient {
    /// Spawn the kakehashi binary and create a new LSP client.
    pub fn new() -> Self {
        // `CARGO_BIN_EXE_kakehashi` is set by Cargo's test harness for integration tests
        // and points to the built `kakehashi` binary, so we don't hardcode its path here.
        let mut child = Command::new(env!("CARGO_BIN_EXE_kakehashi"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn kakehashi binary");

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = BufReader::new(child.stdout.take().expect("Failed to get stdout"));

        Self {
            child,
            stdin: Some(stdin),
            stdout,
            request_id: 0,
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

    /// Receive a server-initiated request with a specific method name.
    ///
    /// Server-initiated requests have both "id" and "method" fields.
    /// This method waits for such a request up to the given timeout.
    ///
    /// # Arguments
    /// * `method` - The expected method name (e.g., "workspace/semanticTokens/refresh")
    /// * `timeout` - Maximum time to wait for the request
    ///
    /// # Returns
    /// * `Some(Value)` - The full request JSON if received
    /// * `None` - If timeout elapsed without receiving the expected request
    ///
    /// # Note
    /// This method collects and discards notifications (no "id") and responses
    /// (no "method") while waiting for a server request.
    pub(crate) fn receive_server_request(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Option<Value> {
        let start_time = Instant::now();

        while start_time.elapsed() < timeout {
            // Use a short internal timeout to allow checking elapsed time
            let remaining = timeout.saturating_sub(start_time.elapsed());
            if remaining.is_zero() {
                return None;
            }

            // Try to receive a message with remaining timeout
            match self.receive_message_with_timeout(remaining.min(Duration::from_millis(100))) {
                Some(message) => {
                    // Server request has both "id" and "method"
                    if message.get("id").is_some() && message.get("method").is_some() {
                        if message["method"].as_str() == Some(method) {
                            return Some(message);
                        }
                        // Different server request - could queue it, but for now skip
                    }
                    // Otherwise it's a notification or response, continue waiting
                }
                None => {
                    // Timeout on this iteration, continue checking overall timeout
                    continue;
                }
            }
        }

        None
    }

    /// Receive a message with timeout. Returns None if timeout expires.
    ///
    /// Unlike receive_message() which blocks indefinitely, this version
    /// returns None when no message is available within the timeout.
    ///
    /// # Note
    /// Due to BufReader::read_line being blocking, this implementation
    /// uses a polling approach with short sleeps. For test purposes,
    /// this limitation is acceptable.
    fn receive_message_with_timeout(&mut self, timeout: Duration) -> Option<Value> {
        use std::io::ErrorKind;

        const MAX_HEADERS: u32 = 100;
        const MAX_MESSAGE_SIZE: usize = 100 * 1024 * 1024;

        let start_time = Instant::now();
        let mut header_count = 0u32;
        let mut header = String::new();

        loop {
            if start_time.elapsed() > timeout {
                return None;
            }

            if header_count >= MAX_HEADERS {
                return None;
            }

            header.clear();

            // Note: BufReader::read_line is blocking. For proper timeout support,
            // would need async I/O or platform-specific non-blocking reads.
            // For tests, we accept this limitation and use short timeouts.
            match self.stdout.read_line(&mut header) {
                Ok(0) => return None, // EOF
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => continue,
                Err(_) => return None,
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
                    .ok()?;

                if len > MAX_MESSAGE_SIZE || len == 0 {
                    return None;
                }

                // Read empty line
                let mut empty = String::new();
                if self.stdout.read_line(&mut empty).ok()? == 0 {
                    return None;
                }

                // Read body
                let mut body = vec![0u8; len];
                std::io::Read::read_exact(&mut self.stdout, &mut body).ok()?;

                return serde_json::from_slice(&body).ok();
            }
        }
    }

    /// Send a response to a server-initiated request.
    ///
    /// Per LSP JSON-RPC 2.0, when the server sends a request (message with "id" and "method"),
    /// the client MUST send a response. Failing to respond is a protocol violation.
    ///
    /// # Arguments
    /// * `request_id` - The "id" from the server's request (accepts any JSON value per JSON-RPC 2.0)
    /// * `result` - The result value (use `json!(null)` for void responses like refresh)
    ///
    /// # Example
    /// ```ignore
    /// let refresh = client.receive_server_request("workspace/semanticTokens/refresh", timeout);
    /// if let Some(request) = refresh {
    ///     let id = request["id"].clone();
    ///     client.respond_to_request(id, json!(null));  // void response
    /// }
    /// ```
    pub(crate) fn respond_to_request(&mut self, request_id: Value, result: Value) {
        let response = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": result
        });
        self.send_message(&response);
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
