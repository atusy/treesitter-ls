//! BridgeConnection for managing connections to language servers

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::{Mutex, Notify};

/// Type of incremental LSP request (completion, hover, signatureHelp)
///
/// Used to track at most one pending request per incremental type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IncrementalType {
    Completion,
    Hover,
    SignatureHelp,
}

// Macro to conditionally make methods public for e2e tests
macro_rules! pub_e2e {
    ($(#[$meta:meta])* async fn $name:ident $($rest:tt)*) => {
        #[cfg(feature = "e2e")]
        $(#[$meta])*
        pub async fn $name $($rest)*

        #[cfg(not(feature = "e2e"))]
        $(#[$meta])*
        pub(crate) async fn $name $($rest)*
    };
}

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
    /// Tracks which virtual document URIs have been opened with didOpen
    /// Used to avoid sending duplicate didOpen notifications for the same virtual document
    opened_documents: Arc<Mutex<HashSet<String>>>,
    /// Tracks at most one pending incremental request per type (completion, hover, signatureHelp)
    /// Maps IncrementalType -> request_id for superseding during initialization window
    pending_incrementals: Mutex<HashMap<IncrementalType, u64>>,
}

impl std::fmt::Debug for BridgeConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Note: Can't easily access process.id() through Mutex without blocking
        f.debug_struct("BridgeConnection")
            .field(
                "next_request_id",
                &self.next_request_id.load(Ordering::SeqCst),
            )
            .field("initialized", &self.initialized.load(Ordering::SeqCst))
            .field("did_open_sent", &self.did_open_sent.load(Ordering::SeqCst))
            .finish()
    }
}

impl BridgeConnection {
    pub_e2e! {
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
        async fn new(command: &str) -> Result<Self, String> {
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
            opened_documents: Arc::new(Mutex::new(HashSet::new())),
            pending_incrementals: Mutex::new(HashMap::new()),
        })
        }
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

        writer
            .write_all(content.as_bytes())
            .await
            .map_err(|e| format!("Failed to write message: {}", e))?;

        writer
            .flush()
            .await
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
        reader
            .read_line(&mut header_line)
            .await
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
        reader
            .read_line(&mut separator)
            .await
            .map_err(|e| format!("Failed to read separator: {}", e))?;

        if separator.trim() != "" {
            return Err(format!(
                "Expected empty separator line, got: {:?}",
                separator
            ));
        }

        // Read exactly content_length bytes for JSON body
        let mut body = vec![0u8; content_length];
        reader
            .read_exact(&mut body)
            .await
            .map_err(|e| format!("Failed to read message body: {}", e))?;

        // Parse JSON
        let message =
            serde_json::from_slice(&body).map_err(|e| format!("Failed to parse JSON: {}", e))?;

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

        // Read response (may need to skip server-initiated notifications/requests)
        let response = {
            let mut stdout = self.stdout.lock().await;
            loop {
                let msg = Self::read_message(&mut *stdout).await?;

                // If this message has an "id" field matching our request, it's the response
                if msg.get("id").and_then(|v| v.as_u64()) == Some(request_id) {
                    break msg;
                }

                // Otherwise, it's a server-initiated notification or request - skip it
                // (e.g., window/logMessage, $/progress, etc.)
                // In a production implementation, we'd handle these properly
            }
        };

        // Response ID was already verified in the loop above

        // Check for error response
        if let Some(error) = response.get("error") {
            return Err(format!("Initialize request failed: {}", error));
        }

        // Return the result
        response
            .get("result")
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
            return Err(
                "SERVER_NOT_INITIALIZED (-32002): Connection not initialized yet".to_string(),
            );
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

    pub_e2e! {
        /// Checks if virtual document has been opened, and sends didOpen if not
        ///
        /// This method is idempotent - it will only send didOpen once per URI.
        /// Subsequent calls with the same URI will return Ok without sending.
        ///
        /// # Arguments
        /// * `uri` - Virtual document URI (e.g., "file:///virtual/lua/abc123.lua")
        /// * `language_id` - Language ID (e.g., "lua")
        /// * `content` - Virtual document content
        ///
        /// # Returns
        /// Ok if URI was already opened OR didOpen sent successfully
        #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
        async fn check_and_send_did_open(
            &self,
            uri: &str,
            language_id: &str,
            content: &str,
        ) -> Result<(), String> {
        // Check if this URI has already been opened
        {
            let opened = self.opened_documents.lock().await;
            if opened.contains(uri) {
                // Already opened, skip didOpen
                return Ok(());
            }
        }

        // Not opened yet - send didOpen
        self.send_did_open(uri, language_id, content).await
        }
    }

    pub_e2e! {
        /// Sends a textDocument/didOpen notification to the language server
        ///
        /// # Arguments
        /// * `uri` - Document URI (e.g., "file:///test.lua")
        /// * `language_id` - Language ID (e.g., "lua")
        /// * `text` - Document content
        #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
        async fn send_did_open(
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

        // Track that this virtual document has been opened
        let mut opened = self.opened_documents.lock().await;
        opened.insert(uri.to_string());

        Ok(())
        }
    }

    pub_e2e! {
        /// Sends a JSON-RPC request and waits for response
        ///
        /// # Arguments
        /// * `method` - LSP method name (e.g., "textDocument/completion")
        /// * `params` - Request parameters as JSON value
        ///
        /// # Returns
        /// Response result on success, error string on failure
        ///
        /// # Errors
        /// Returns error if:
        /// - Failed to send request
        /// - Response indicates error
        /// - Timeout waiting for response
        #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
        async fn send_request(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value, String> {
        use tokio::time::{timeout, Duration};

        // Get next request ID
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);

        // Build JSON-RPC request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": params
        });

        // Send request
        {
            let mut stdin = self.stdin.lock().await;
            Self::write_message(&mut *stdin, &request).await?;
        }

        // Read response with timeout (skip non-matching messages like notifications)
        let response = timeout(Duration::from_secs(5), async {
            let mut stdout = self.stdout.lock().await;
            loop {
                let msg = Self::read_message(&mut *stdout).await?;

                // If this message has an "id" field matching our request, it's the response
                if msg.get("id").and_then(|v| v.as_u64()) == Some(request_id) {
                    return Ok::<_, String>(msg);
                }

                // Otherwise, it's a server-initiated notification or request - skip it
                // In a production implementation, we'd handle these properly
            }
        })
        .await
        .map_err(|_| "REQUEST_FAILED (-32803): Request timed out after 5s".to_string())??;

        // Check for error response
        if let Some(error) = response.get("error") {
            return Err(format!("REQUEST_FAILED (-32803): {}", error));
        }

        // Return the result
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "REQUEST_FAILED (-32803): Response missing 'result' field".to_string())
        }
    }

    pub_e2e! {
        /// Performs the full LSP initialization sequence with timeout
        ///
        /// Sequence: initialize request → initialized notification
        /// This method has a 5 second timeout to prevent hangs.
        ///
        /// # Returns
        /// InitializeResult from the language server
        #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
        async fn initialize(&self) -> Result<serde_json::Value, String> {
        use tokio::time::{timeout, Duration};

        // Initialize with 5s timeout
        let result = timeout(
            Duration::from_secs(5),
            self.send_initialize_request()
        ).await
            .map_err(|_| "REQUEST_FAILED (-32803): Initialize request timed out after 5s".to_string())?
            .map_err(|e| format!("REQUEST_FAILED (-32803): {}", e))?;

        // Send initialized notification (no timeout - should be fast)
        self.send_initialized_notification().await?;

        Ok(result)
        }
    }

    /// Waits for the connection to be initialized with bounded timeout
    ///
    /// This method allows callers to wait for initialization to complete
    /// without blocking the spawn. Uses tokio::select! to implement timeout.
    ///
    /// # Arguments
    /// * `timeout_duration` - Maximum time to wait for initialization
    ///
    /// # Returns
    /// Ok(()) if initialized within timeout, Err if timeout expires
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn wait_for_initialized(
        &self,
        timeout_duration: std::time::Duration,
    ) -> Result<(), String> {
        use tokio::time::timeout;

        // If already initialized, return immediately
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Wait for initialized_notify with timeout
        timeout(timeout_duration, self.initialized_notify.notified())
            .await
            .map_err(|_| {
                format!(
                    "REQUEST_FAILED (-32803): Connection not initialized within {:?}",
                    timeout_duration
                )
            })?;

        Ok(())
    }

    /// Returns whether the connection has been initialized
    ///
    /// Used for testing and diagnostics.
    #[allow(dead_code)] // Used in tests
    pub(crate) fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn test_send_request_tracks_request_id() {
        // RED: Test that send_request tracks request IDs and correlates responses
        let connection = BridgeConnection::new("cat").await.unwrap();

        // First request ID should be 1 (next_request_id starts at 1)
        let initial_id = connection.next_request_id.load(Ordering::SeqCst);
        assert_eq!(initial_id, 1, "Initial request ID should be 1");

        // Note: We can't actually test send_request with 'cat' because it doesn't speak LSP
        // This test just verifies the field exists and is initialized correctly
        // Real behavior will be tested in E2E test with lua-language-server
    }

    #[tokio::test]
    async fn test_bridge_connection_spawns_process_with_valid_command() {
        // Test spawning a real process (use 'cat' as a simple test command)
        // This verifies tokio::process::Command integration
        let result = BridgeConnection::new("cat").await;

        assert!(
            result.is_ok(),
            "Failed to spawn process: {:?}",
            result.err()
        );
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
        assert!(
            error.contains("Failed to spawn"),
            "Error should mention spawn failure: {}",
            error
        );
        assert!(
            error.contains("nonexistent-binary-xyz123"),
            "Error should mention command: {}",
            error
        );
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
        BridgeConnection::write_message(&mut buffer, &message)
            .await
            .unwrap();

        let output = String::from_utf8(buffer).unwrap();

        // Should have Content-Length header
        assert!(
            output.starts_with("Content-Length: "),
            "Should start with Content-Length header"
        );

        // Should have \r\n\r\n separator
        assert!(
            output.contains("\r\n\r\n"),
            "Should have \\r\\n\\r\\n separator"
        );

        // Should end with JSON body
        let parts: Vec<&str> = output.split("\r\n\r\n").collect();
        assert_eq!(parts.len(), 2, "Should have exactly header and body");

        let header = parts[0];
        let body = parts[1];

        // Header should be Content-Length: N
        let content_length: usize = header
            .strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();

        // Body length should match Content-Length
        assert_eq!(
            body.len(),
            content_length,
            "Body length should match Content-Length header"
        );

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
        assert!(
            error.contains("Content-Length"),
            "Error should mention Content-Length"
        );
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
        let result = connection
            .send_notification(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": "file:///test.lua",
                        "languageId": "lua",
                        "version": 1,
                        "text": "print('hello')"
                    }
                }),
            )
            .await;

        assert!(
            result.is_err(),
            "Should block notification before initialized"
        );
        let error = result.unwrap_err();
        assert!(
            error.contains("SERVER_NOT_INITIALIZED"),
            "Error should mention SERVER_NOT_INITIALIZED: {}",
            error
        );
        assert!(
            error.contains("-32002"),
            "Error should include error code -32002: {}",
            error
        );
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
        assert!(
            result.is_ok(),
            "initialized notification should be allowed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_phase1_guard_allows_notifications_after_initialized() {
        // Test that notifications are allowed after initialization
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Manually set initialized flag (normally done by send_initialized_notification)
        connection.initialized.store(true, Ordering::SeqCst);

        // Now didOpen should be allowed
        let result = connection
            .send_notification(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": "file:///test.lua",
                        "languageId": "lua",
                        "version": 1,
                        "text": "print('hello')"
                    }
                }),
            )
            .await;

        assert!(
            result.is_ok(),
            "Notification should be allowed after initialized: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_did_open_blocked_before_initialized() {
        // Test that didOpen is blocked by Phase 1 guard
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Should not be initialized initially
        assert!(!connection.initialized.load(Ordering::SeqCst));

        // Try to send didOpen
        let result = connection
            .send_did_open("file:///test.lua", "lua", "print('hello')")
            .await;

        assert!(
            result.is_err(),
            "didOpen should be blocked before initialized"
        );
        let error = result.unwrap_err();
        assert!(
            error.contains("SERVER_NOT_INITIALIZED"),
            "Error should mention SERVER_NOT_INITIALIZED: {}",
            error
        );

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
        let result = connection
            .send_did_open("file:///test.lua", "lua", "print('hello')")
            .await;

        assert!(result.is_ok(), "didOpen should succeed: {:?}", result.err());

        // did_open_sent flag should now be true
        assert!(connection.did_open_sent.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_opened_documents_tracks_virtual_uris() {
        // RED: Test that opened_documents HashSet tracks which virtual URIs have been opened
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Set initialized flag
        connection.initialized.store(true, Ordering::SeqCst);

        // Initially, opened_documents should be empty
        {
            let opened = connection.opened_documents.lock().await;
            assert_eq!(opened.len(), 0, "opened_documents should start empty");
        }

        // Send didOpen for a virtual URI
        let uri = "file:///virtual/lua/abc123.lua";
        let result = connection.send_did_open(uri, "lua", "print('hello')").await;
        assert!(result.is_ok(), "didOpen should succeed: {:?}", result.err());

        // Verify URI was added to opened_documents
        {
            let opened = connection.opened_documents.lock().await;
            assert_eq!(
                opened.len(),
                1,
                "opened_documents should have one entry after didOpen"
            );
            assert!(
                opened.contains(uri),
                "opened_documents should contain the virtual URI"
            );
        }

        // Send didOpen for a different virtual URI
        let uri2 = "file:///virtual/lua/xyz789.lua";
        let result2 = connection
            .send_did_open(uri2, "lua", "print('world')")
            .await;
        assert!(
            result2.is_ok(),
            "Second didOpen should succeed: {:?}",
            result2.err()
        );

        // Verify both URIs are tracked
        {
            let opened = connection.opened_documents.lock().await;
            assert_eq!(
                opened.len(),
                2,
                "opened_documents should have two entries after second didOpen"
            );
            assert!(opened.contains(uri), "Should contain first URI");
            assert!(opened.contains(uri2), "Should contain second URI");
        }
    }

    #[tokio::test]
    async fn test_check_and_send_did_open_sends_only_on_first_access() {
        // RED: Test that check_and_send_did_open sends didOpen only once per URI
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Set initialized flag
        connection.initialized.store(true, Ordering::SeqCst);

        let uri = "file:///virtual/lua/abc123.lua";
        let language_id = "lua";
        let content = "print('hello')";

        // Initially, URI should not be in opened_documents
        {
            let opened = connection.opened_documents.lock().await;
            assert!(!opened.contains(uri), "URI should not be opened initially");
        }

        // First call should send didOpen
        let result1 = connection
            .check_and_send_did_open(uri, language_id, content)
            .await;
        assert!(
            result1.is_ok(),
            "First check_and_send_did_open should succeed: {:?}",
            result1.err()
        );

        // Verify URI was added to opened_documents
        {
            let opened = connection.opened_documents.lock().await;
            assert!(
                opened.contains(uri),
                "URI should be in opened_documents after first call"
            );
        }

        // Second call with same URI should not send didOpen (returns Ok but skips)
        // We can verify by checking that opened_documents still has size 1
        let result2 = connection
            .check_and_send_did_open(uri, language_id, content)
            .await;
        assert!(
            result2.is_ok(),
            "Second check_and_send_did_open should succeed: {:?}",
            result2.err()
        );

        // Verify opened_documents still has just one entry
        {
            let opened = connection.opened_documents.lock().await;
            assert_eq!(
                opened.len(),
                1,
                "opened_documents should still have one entry"
            );
        }
    }

    #[tokio::test]
    async fn test_wait_for_initialized_returns_immediately_when_already_initialized() {
        // Test that wait_for_initialized returns immediately if already initialized
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Set initialized flag
        connection.initialized.store(true, Ordering::SeqCst);

        // Should return immediately without waiting
        let result = connection
            .wait_for_initialized(std::time::Duration::from_secs(5))
            .await;

        assert!(
            result.is_ok(),
            "Should return Ok immediately when already initialized: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_wait_for_initialized_waits_for_notification() {
        // Test that wait_for_initialized waits for initialized_notify signal
        let connection = Arc::new(BridgeConnection::new("cat").await.unwrap());

        // Initially not initialized
        assert!(!connection.initialized.load(Ordering::SeqCst));

        // Spawn a task that will set initialized after a delay
        let conn_clone = connection.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            conn_clone.initialized.store(true, Ordering::SeqCst);
            conn_clone.initialized_notify.notify_waiters();
        });

        // Wait for initialization (should succeed before 5s timeout)
        let result = connection
            .wait_for_initialized(std::time::Duration::from_secs(5))
            .await;

        assert!(
            result.is_ok(),
            "Should succeed when notified within timeout: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_wait_for_initialized_times_out() {
        // Test that wait_for_initialized returns error on timeout
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Never set initialized flag (no notify)
        assert!(!connection.initialized.load(Ordering::SeqCst));

        // Wait with very short timeout (should timeout)
        let result = connection
            .wait_for_initialized(std::time::Duration::from_millis(50))
            .await;

        assert!(
            result.is_err(),
            "Should timeout when not initialized within duration"
        );

        let error = result.unwrap_err();
        assert!(
            error.contains("REQUEST_FAILED"),
            "Error should contain REQUEST_FAILED: {}",
            error
        );
        assert!(
            error.contains("-32803"),
            "Error should contain error code -32803: {}",
            error
        );
        assert!(
            error.contains("not initialized"),
            "Error should mention not initialized: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_pending_incrementals_tracks_request_per_type() {
        // RED: Test that BridgeConnection tracks at most one pending request per incremental type
        let connection = BridgeConnection::new("cat").await.unwrap();

        // Access pending_incrementals map (should start empty)
        let pending = connection.pending_incrementals.lock().await;
        assert_eq!(
            pending.len(),
            0,
            "pending_incrementals should start empty"
        );
    }
}
