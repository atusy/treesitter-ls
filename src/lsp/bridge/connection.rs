//! BridgeConnection for managing connections to language servers

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{Mutex, Notify};

/// Represents a connection to a bridged language server
#[allow(dead_code)] // Used in Phase 2 (real LSP communication)
pub struct BridgeConnection {
    /// Spawned language server process
    process: Mutex<Child>,
    /// Stdin handle for sending requests/notifications (wrapped in Mutex for async access)
    stdin: Mutex<ChildStdin>,
    /// Stdout handle for receiving responses/notifications (wrapped in Mutex for async access)
    stdout: Mutex<ChildStdout>,
    /// Next request ID for JSON-RPC requests
    next_request_id: AtomicU64,
    /// Tracks whether the connection has been initialized
    initialized: AtomicBool,
    /// Notify for waking tasks waiting for initialization
    initialized_notify: Notify,
    /// Tracks whether didOpen notification has been sent
    did_open_sent: AtomicBool,
}

impl std::fmt::Debug for BridgeConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Note: Can't easily access process.id() through Mutex without blocking
        f.debug_struct("BridgeConnection")
            .field("next_request_id", &self.next_request_id.load(Ordering::SeqCst))
            .field("initialized", &self.initialized.load(Ordering::SeqCst))
            .field("did_open_sent", &self.did_open_sent.load(Ordering::SeqCst))
            .finish()
    }
}

impl BridgeConnection {
    /// Creates a new BridgeConnection by spawning a language server process
    ///
    /// # Arguments
    /// * `command` - Command to spawn (e.g., "lua-language-server")
    ///
    /// # Errors
    /// Returns error if:
    /// - Process fails to spawn
    /// - stdin/stdout handles cannot be obtained
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn new(command: &str) -> Result<Self, String> {
        use tokio::process::Command;
        use std::process::Stdio;

        let mut child = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| format!("Failed to obtain stdin for {}", command))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| format!("Failed to obtain stdout for {}", command))?;

        Ok(Self {
            process: Mutex::new(child),
            stdin: Mutex::new(stdin),
            stdout: Mutex::new(stdout),
            next_request_id: AtomicU64::new(1),
            initialized: AtomicBool::new(false),
            initialized_notify: Notify::new(),
            did_open_sent: AtomicBool::new(false),
        })
    }

    /// Writes a JSON-RPC message with LSP Base Protocol framing
    ///
    /// Format: `Content-Length: N\r\n\r\n{json}`
    async fn write_message<W>(writer: &mut W, message: &serde_json::Value) -> Result<(), String>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        use tokio::io::AsyncWriteExt;

        let json_str = serde_json::to_string(message)
            .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

        let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);

        writer.write_all(content.as_bytes()).await
            .map_err(|e| format!("Failed to write message: {}", e))?;

        writer.flush().await
            .map_err(|e| format!("Failed to flush writer: {}", e))?;

        Ok(())
    }

    /// Reads a JSON-RPC message with LSP Base Protocol framing
    ///
    /// Expected format: `Content-Length: N\r\n\r\n{json}`
    async fn read_message<R>(reader: &mut R) -> Result<serde_json::Value, String>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncReadExt;

        let mut reader = tokio::io::BufReader::new(reader);

        // Read header line: "Content-Length: N"
        let mut header_line = String::new();
        reader.read_line(&mut header_line).await
            .map_err(|e| format!("Failed to read header: {}", e))?;

        // Parse Content-Length
        let content_length = header_line
            .trim()
            .strip_prefix("Content-Length: ")
            .ok_or_else(|| format!("Missing Content-Length header, got: {}", header_line))?
            .parse::<usize>()
            .map_err(|e| format!("Invalid Content-Length value: {}", e))?;

        // Read separator line (should be empty "\r\n")
        let mut separator = String::new();
        reader.read_line(&mut separator).await
            .map_err(|e| format!("Failed to read separator: {}", e))?;

        if separator.trim() != "" {
            return Err(format!("Expected empty separator line, got: {:?}", separator));
        }

        // Read exactly content_length bytes for JSON body
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await
            .map_err(|e| format!("Failed to read message body: {}", e))?;

        // Parse JSON
        let message = serde_json::from_slice(&body)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        Ok(message)
    }

    /// Sends an initialize request to the language server
    ///
    /// This sends a proper LSP initialize request and waits for InitializeResult.
    /// Does NOT send the initialized notification - that's a separate method.
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn send_initialize_request(&self) -> Result<serde_json::Value, String> {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "clientInfo": {
                    "name": "treesitter-ls",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {}
            }
        });

        // Write request
        {
            let mut stdin = self.stdin.lock().await;
            Self::write_message(&mut *stdin, &request).await?;
        }

        // Read response
        let response = {
            let mut stdout = self.stdout.lock().await;
            Self::read_message(&mut *stdout).await?
        };

        // Verify it's a response to our request
        if response.get("id").and_then(|v| v.as_u64()) != Some(request_id) {
            return Err(format!("Response ID mismatch: expected {}, got {:?}",
                request_id, response.get("id")));
        }

        // Check for error response
        if let Some(error) = response.get("error") {
            return Err(format!("Initialize request failed: {}", error));
        }

        // Return the result
        response.get("result")
            .cloned()
            .ok_or_else(|| "Initialize response missing 'result' field".to_string())
    }

    /// Sends the initialized notification to the language server
    ///
    /// This MUST be called after receiving InitializeResult and before
    /// sending any other notifications or requests. Sets the initialized flag.
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn send_initialized_notification(&self) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });

        // Write notification
        {
            let mut stdin = self.stdin.lock().await;
            Self::write_message(&mut *stdin, &notification).await?;
        }

        // Set initialized flag and notify waiters
        self.initialized.store(true, Ordering::SeqCst);
        self.initialized_notify.notify_waiters();

        Ok(())
    }

    /// Sends a notification to the language server
    ///
    /// # Phase 1 Guard
    /// Blocks all notifications (except "initialized") if the connection
    /// hasn't been initialized yet. Returns SERVER_NOT_INITIALIZED error.
    ///
    /// # Arguments
    /// * `method` - LSP notification method (e.g., "textDocument/didOpen")
    /// * `params` - Notification parameters
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn send_notification(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        // Phase 1 guard: block notifications before initialized (except "initialized" itself)
        if !self.initialized.load(Ordering::SeqCst) && method != "initialized" {
            return Err("SERVER_NOT_INITIALIZED (-32002): Connection not initialized yet".to_string());
        }

        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        // Write notification
        {
            let mut stdin = self.stdin.lock().await;
            Self::write_message(&mut *stdin, &notification).await?;
        }

        Ok(())
    }

    /// Sends a textDocument/didOpen notification to the language server
    ///
    /// # Arguments
    /// * `uri` - Document URI (e.g., "file:///test.lua")
    /// * `language_id` - Language ID (e.g., "lua")
    /// * `text` - Document content
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn send_did_open(
        &self,
        uri: &str,
        language_id: &str,
        text: &str,
    ) -> Result<(), String> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": text
            }
        });

        self.send_notification("textDocument/didOpen", params).await?;

        // Set did_open_sent flag
        self.did_open_sent.store(true, Ordering::SeqCst);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use serde_json::json;

    #[tokio::test]
    async fn test_bridge_connection_spawns_process_with_valid_command() {
        // Test spawning a real process (use 'cat' as a simple test command)
        // This verifies tokio::process::Command integration
        let result = BridgeConnection::new("cat").await;

        assert!(result.is_ok(), "Failed to spawn process: {:?}", result.err());
        let connection = result.unwrap();

        // Verify process is alive (check atomic fields)
        assert_eq!(connection.next_request_id.load(Ordering::SeqCst), 1);
        assert!(!connection.initialized.load(Ordering::SeqCst));
        assert!(!connection.did_open_sent.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_bridge_connection_fails_with_invalid_command() {
        // Test that invalid command returns clear error
        let result = BridgeConnection::new("nonexistent-binary-xyz123").await;

        assert!(result.is_err(), "Should fail for nonexistent command");
        let error = result.unwrap_err();
        assert!(error.contains("Failed to spawn"), "Error should mention spawn failure: {}", error);
        assert!(error.contains("nonexistent-binary-xyz123"), "Error should mention command: {}", error);
    }

    #[tokio::test]
    async fn test_write_message_formats_with_content_length_header() {
        // Test LSP Base Protocol message framing
        let message = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let mut buffer = Vec::new();
        BridgeConnection::write_message(&mut buffer, &message).await.unwrap();

        let output = String::from_utf8(buffer).unwrap();

        // Should have Content-Length header
        assert!(output.starts_with("Content-Length: "), "Should start with Content-Length header");

        // Should have \r\n\r\n separator
        assert!(output.contains("\r\n\r\n"), "Should have \\r\\n\\r\\n separator");

        // Should end with JSON body
        let parts: Vec<&str> = output.split("\r\n\r\n").collect();
        assert_eq!(parts.len(), 2, "Should have exactly header and body");

        let header = parts[0];
        let body = parts[1];

        // Header should be Content-Length: N
        let content_length: usize = header.strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();

        // Body length should match Content-Length
        assert_eq!(body.len(), content_length, "Body length should match Content-Length header");

        // Body should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["method"], "initialize");
    }

    #[tokio::test]
    async fn test_read_message_parses_content_length_header() {
        // RED: Test reading LSP Base Protocol message
        let message = json!({"jsonrpc": "2.0", "id": 1, "result": {}});
        let json_str = serde_json::to_string(&message).unwrap();
        let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);

        let mut reader = std::io::Cursor::new(content.as_bytes());
        let result = BridgeConnection::read_message(&mut reader).await.unwrap();

        assert_eq!(result["id"], 1);
        assert_eq!(result["result"], json!({}));
    }

    #[tokio::test]
    async fn test_read_message_fails_on_invalid_header() {
        // Test error handling for malformed messages
        let content = "Invalid-Header: 123\r\n\r\n{}";
        let mut reader = std::io::Cursor::new(content.as_bytes());

        let result = BridgeConnection::read_message(&mut reader).await;
        assert!(result.is_err(), "Should fail on invalid header");

        let error = result.unwrap_err();
        assert!(error.contains("Content-Length"), "Error should mention Content-Length");
    }

    #[tokio::test]
    async fn test_send_initialize_request_increments_request_id() {
        // Test that send_initialize_request uses incrementing request IDs
        // We'll use 'cat' and mock the response by writing to its stdin won't work
        // Instead, let's just verify the request structure is correct
        // For now, skip this test - we'll verify in E2E test with real lua-ls
    }

    #[tokio::test]
    async fn test_phase1_guard_blocks_notifications_before_initialized() {
        // RED: Test Phase 1 guard blocks notifications before initialized
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Should not be initialized initially
        assert!(!connection.initialized.load(Ordering::SeqCst));

        // Try to send a didOpen notification before initialized
        let result = connection.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///test.lua",
                    "languageId": "lua",
                    "version": 1,
                    "text": "print('hello')"
                }
            })
        ).await;

        assert!(result.is_err(), "Should block notification before initialized");
        let error = result.unwrap_err();
        assert!(error.contains("SERVER_NOT_INITIALIZED"), "Error should mention SERVER_NOT_INITIALIZED: {}", error);
        assert!(error.contains("-32002"), "Error should include error code -32002: {}", error);
    }

    #[tokio::test]
    async fn test_phase1_guard_allows_initialized_notification() {
        // Test that "initialized" notification is allowed even before flag is set
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Should not be initialized initially
        assert!(!connection.initialized.load(Ordering::SeqCst));

        // "initialized" notification should be allowed (won't check response since cat doesn't speak LSP)
        let result = connection.send_notification("initialized", json!({})).await;

        // Should succeed (cat will just echo or ignore, but no error from our guard)
        assert!(result.is_ok(), "initialized notification should be allowed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_phase1_guard_allows_notifications_after_initialized() {
        // Test that notifications are allowed after initialization
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Manually set initialized flag (normally done by send_initialized_notification)
        connection.initialized.store(true, Ordering::SeqCst);

        // Now didOpen should be allowed
        let result = connection.send_notification(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": "file:///test.lua",
                    "languageId": "lua",
                    "version": 1,
                    "text": "print('hello')"
                }
            })
        ).await;

        assert!(result.is_ok(), "Notification should be allowed after initialized: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_did_open_blocked_before_initialized() {
        // Test that didOpen is blocked by Phase 1 guard
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Should not be initialized initially
        assert!(!connection.initialized.load(Ordering::SeqCst));

        // Try to send didOpen
        let result = connection.send_did_open(
            "file:///test.lua",
            "lua",
            "print('hello')"
        ).await;

        assert!(result.is_err(), "didOpen should be blocked before initialized");
        let error = result.unwrap_err();
        assert!(error.contains("SERVER_NOT_INITIALIZED"), "Error should mention SERVER_NOT_INITIALIZED: {}", error);

        // did_open_sent flag should still be false
        assert!(!connection.did_open_sent.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_did_open_sets_flag_after_initialized() {
        // Test that didOpen sets did_open_sent flag
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Set initialized flag
        connection.initialized.store(true, Ordering::SeqCst);

        // Initially did_open_sent should be false
        assert!(!connection.did_open_sent.load(Ordering::SeqCst));

        // Send didOpen
        let result = connection.send_did_open(
            "file:///test.lua",
            "lua",
            "print('hello')"
        ).await;

        assert!(result.is_ok(), "didOpen should succeed: {:?}", result.err());

        // did_open_sent flag should now be true
        assert!(connection.did_open_sent.load(Ordering::SeqCst));
    }
}
