//! LSP Redirection for injection regions
//!
//! This module handles redirecting LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

use dashmap::DashMap;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read as _, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::*;

/// Default timeout for LSP requests (5 seconds)
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Manages a connection to a language server subprocess with a temporary workspace
pub struct LanguageServerConnection {
    process: Child,
    request_id: i64,
    stdout_reader: BufReader<ChildStdout>,
    /// Temporary directory for the workspace (cleaned up on drop)
    temp_dir: Option<PathBuf>,
    /// Track the version of the document currently open (None = not open yet)
    document_version: Option<i32>,
    /// Timeout duration for LSP requests
    timeout_duration: Duration,
}

impl LanguageServerConnection {
    /// Spawn rust-analyzer with a temporary Cargo project workspace.
    ///
    /// rust-analyzer requires a Cargo project context for go-to-definition to work.
    /// This creates a minimal temp project that gets cleaned up on drop.
    pub fn spawn_rust_analyzer() -> Option<Self> {
        // Create a temporary directory for the Cargo project
        let temp_dir =
            std::env::temp_dir().join(format!("treesitter-ls-ra-{}", std::process::id()));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(&src_dir).ok()?;

        // Write minimal Cargo.toml
        let cargo_toml = temp_dir.join("Cargo.toml");
        std::fs::write(
            &cargo_toml,
            "[package]\nname = \"virtual\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .ok()?;

        // Create empty main.rs (will be overwritten by did_open)
        let main_rs = src_dir.join("main.rs");
        std::fs::write(&main_rs, "").ok()?;

        let root_uri = format!("file://{}", temp_dir.display());

        let mut process = Command::new("rust-analyzer")
            .current_dir(&temp_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        // Take stdout and wrap in BufReader to maintain consistent buffering
        let stdout = process.stdout.take()?;
        let stdout_reader = BufReader::new(stdout);

        let mut conn = Self {
            process,
            request_id: 0,
            stdout_reader,
            temp_dir: Some(temp_dir),
            document_version: None,
            timeout_duration: DEFAULT_REQUEST_TIMEOUT,
        };

        // Send initialize request with workspace root
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "capabilities": {},
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": "virtual"}],
        });

        let init_id = conn.send_request("initialize", init_params)?;

        // Wait for initialize response
        conn.read_response_for_id(init_id)?;

        // Send initialized notification
        conn.send_notification("initialized", serde_json::json!({}));

        Some(conn)
    }

    /// Get the URI for the virtual main.rs file in the temp workspace
    pub fn main_rs_uri(&self) -> Option<String> {
        self.temp_dir
            .as_ref()
            .map(|dir| format!("file://{}/src/main.rs", dir.display()))
    }

    /// Send a JSON-RPC request, returns the request ID
    fn send_request(&mut self, method: &str, params: Value) -> Option<i64> {
        self.request_id += 1;
        let id = self.request_id;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.send_message(&request)?;
        Some(id)
    }

    /// Send a JSON-RPC notification (no response expected)
    fn send_notification(&mut self, method: &str, params: Value) -> Option<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send_message(&notification)
    }

    /// Send a JSON-RPC message
    fn send_message(&mut self, message: &Value) -> Option<()> {
        let content = serde_json::to_string(message).ok()?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let stdin = self.process.stdin.as_mut()?;
        stdin.write_all(header.as_bytes()).ok()?;
        stdin.write_all(content.as_bytes()).ok()?;
        stdin.flush().ok()?;

        Some(())
    }

    /// Read a JSON-RPC response matching the given request ID
    /// Skips notifications and other responses until finding the matching one
    fn read_response_for_id(&mut self, expected_id: i64) -> Option<Value> {
        loop {
            // Read headers
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if self.stdout_reader.read_line(&mut line).ok()? == 0 {
                    return None; // EOF
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(len_str) = line.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse().ok()?;
                }
            }

            if content_length == 0 {
                return None;
            }

            // Read content
            let mut content = vec![0u8; content_length];
            std::io::Read::read_exact(&mut self.stdout_reader, &mut content).ok()?;

            let message: Value = serde_json::from_slice(&content).ok()?;

            // Check if this is the response we're looking for
            if let Some(id) = message.get("id")
                && id.as_i64() == Some(expected_id)
            {
                return Some(message);
            }
            // Otherwise it's a notification or different response - skip it
        }
    }

    /// Open or update a document in the language server and write it to the temp workspace.
    ///
    /// For rust-analyzer, we need to write the file to disk for proper indexing.
    /// The content is written to src/main.rs in the temp workspace.
    ///
    /// On first call, sends `textDocument/didOpen` and waits for indexing.
    /// On subsequent calls, sends `textDocument/didChange` (no wait needed).
    pub fn did_open(&mut self, _uri: &str, language_id: &str, content: &str) -> Option<()> {
        // Write content to the actual file on disk (rust-analyzer needs this)
        if let Some(temp_dir) = &self.temp_dir {
            let main_rs = temp_dir.join("src").join("main.rs");
            std::fs::write(&main_rs, content).ok()?;
        }

        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        if let Some(version) = self.document_version {
            // Document already open - send didChange instead
            let new_version = version + 1;
            let params = serde_json::json!({
                "textDocument": {
                    "uri": real_uri,
                    "version": new_version,
                },
                "contentChanges": [{ "text": content }]
            });
            self.send_notification("textDocument/didChange", params)?;
            self.document_version = Some(new_version);
        } else {
            // First time - send didOpen and wait for indexing
            let params = serde_json::json!({
                "textDocument": {
                    "uri": real_uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": content,
                }
            });
            self.send_notification("textDocument/didOpen", params)?;
            self.document_version = Some(1);

            // Wait for rust-analyzer to index the project.
            // rust-analyzer needs time to parse the file and build its index.
            // We wait for diagnostic notifications which indicate indexing is complete.
            self.wait_for_indexing();
        }

        Some(())
    }

    /// Wait for rust-analyzer to finish indexing by consuming messages until we see diagnostics
    fn wait_for_indexing(&mut self) {
        // Read messages until we get a publishDiagnostics notification
        // or timeout after consuming a few messages
        for _ in 0..50 {
            // Read headers
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if self.stdout_reader.read_line(&mut line).ok().unwrap_or(0) == 0 {
                    return; // EOF
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(len_str) = line.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse().unwrap_or(0);
                }
            }

            if content_length == 0 {
                return;
            }

            // Read content
            let mut content = vec![0u8; content_length];
            if std::io::Read::read_exact(&mut self.stdout_reader, &mut content).is_err() {
                return;
            }

            let Ok(message) = serde_json::from_slice::<Value>(&content) else {
                continue;
            };

            // Check if this is a publishDiagnostics notification
            if let Some(method) = message.get("method").and_then(|m| m.as_str())
                && method == "textDocument/publishDiagnostics"
            {
                // rust-analyzer has indexed enough to publish diagnostics
                return;
            }
        }
    }

    /// Request go-to-definition (synchronous version)
    ///
    /// Uses the actual file URI from the temp workspace, not the virtual URI.
    /// Note: This method may block indefinitely. For timeout support, use `goto_definition_async`.
    pub fn goto_definition(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let req_id = self.send_request("textDocument/definition", params)?;

        // Read response (skipping notifications until we get the matching response)
        let response = self.read_response_for_id(req_id)?;

        // Extract result
        let result = response.get("result")?;

        // Parse as GotoDefinitionResponse
        serde_json::from_value(result.clone()).ok()
    }

    /// Request go-to-definition with timeout support (async version)
    ///
    /// Returns None if the request times out or fails.
    /// Uses the configured timeout_duration for the request.
    pub async fn goto_definition_async(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let req_id = self.send_request("textDocument/definition", params)?;

        // Read response with timeout (uses poll-based timeout on Unix)
        let timeout = self.timeout_duration;
        let response = self.read_response_with_timeout_sync(req_id, timeout)?;

        // Extract result
        let result = response.get("result")?;

        // Parse as GotoDefinitionResponse
        serde_json::from_value(result.clone()).ok()
    }

    /// Read a single byte with timeout using poll() on Unix
    ///
    /// Returns Some(byte) if read succeeds, None on timeout or error.
    fn read_byte_with_timeout(&mut self, timeout: Duration) -> Option<u8> {
        // First, check if there's data already in the BufReader's buffer
        // If so, we can read immediately without polling
        if !self.stdout_reader.buffer().is_empty() {
            let mut buf = [0u8; 1];
            match self.stdout_reader.read(&mut buf) {
                Ok(1) => return Some(buf[0]),
                _ => return None,
            }
        }

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;

            // No buffered data, need to wait for new data from the fd
            let fd = self.stdout_reader.get_ref().as_raw_fd();

            // Use poll() to wait for data with timeout
            let timeout_ms = timeout.as_millis() as i32;
            let mut pollfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };

            let poll_result = unsafe { libc::poll(&mut pollfd, 1, timeout_ms) };

            if poll_result <= 0 {
                // Timeout (0) or error (-1)
                return None;
            }

            // Data is available, read through the BufReader to maintain buffering
            let mut buf = [0u8; 1];
            match self.stdout_reader.read(&mut buf) {
                Ok(1) => Some(buf[0]),
                _ => None,
            }
        }

        #[cfg(not(unix))]
        {
            // Fallback: just do blocking read (no timeout on non-Unix)
            let mut buf = [0u8; 1];
            match self.stdout_reader.read(&mut buf) {
                Ok(1) => Some(buf[0]),
                _ => None,
            }
        }
    }

    /// Read a line from the stdout reader with timeout
    fn read_line_with_timeout(&mut self, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        let mut line = String::new();

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None; // Timeout
            }

            match self.read_byte_with_timeout(remaining) {
                Some(b'\n') => {
                    return Some(line);
                }
                Some(b) => {
                    line.push(b as char);
                }
                None => {
                    // Timeout or error
                    return None;
                }
            }
        }
    }

    /// Read exact number of bytes with timeout
    fn read_exact_with_timeout(&mut self, buf: &mut [u8], timeout: Duration) -> Option<()> {
        let deadline = Instant::now() + timeout;

        for byte in buf.iter_mut() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None; // Timeout
            }

            *byte = self.read_byte_with_timeout(remaining)?;
        }
        Some(())
    }

    /// Read a JSON-RPC response with timeout
    ///
    /// Returns None if the request times out or fails.
    fn read_response_with_timeout_sync(
        &mut self,
        expected_id: i64,
        timeout: Duration,
    ) -> Option<Value> {
        let deadline = Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None; // Timeout
            }

            // Read headers
            let mut content_length = 0;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return None;
                }

                let line = self.read_line_with_timeout(remaining)?;
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(len_str) = line.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse().ok()?;
                }
            }

            if content_length == 0 {
                return None;
            }

            // Read content
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let mut content = vec![0u8; content_length];
            self.read_exact_with_timeout(&mut content, remaining)?;

            let message: Value = serde_json::from_slice(&content).ok()?;

            // Check if this is the response we're looking for
            if let Some(id) = message.get("id")
                && id.as_i64() == Some(expected_id)
            {
                return Some(message);
            }
            // Otherwise it's a notification or different response - skip it
        }
    }

    /// Request hover information (synchronous version)
    ///
    /// Uses the actual file URI from the temp workspace, not the virtual URI.
    /// Note: This method may block indefinitely. For timeout support, use `hover_async`.
    pub fn hover(&mut self, _uri: &str, position: Position) -> Option<Hover> {
        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let req_id = self.send_request("textDocument/hover", params)?;

        // Read response (skipping notifications until we get the matching response)
        let response = self.read_response_for_id(req_id)?;

        // Extract result
        let result = response.get("result")?;

        // Parse as Hover (result can be null if no hover info available)
        if result.is_null() {
            return None;
        }

        serde_json::from_value(result.clone()).ok()
    }

    /// Request hover information with timeout support (async version)
    ///
    /// Returns None if the request times out or fails.
    /// Uses the configured timeout_duration for the request.
    pub async fn hover_async(&mut self, _uri: &str, position: Position) -> Option<Hover> {
        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let req_id = self.send_request("textDocument/hover", params)?;

        // Read response with timeout (uses poll-based timeout on Unix)
        let timeout = self.timeout_duration;
        let response = self.read_response_with_timeout_sync(req_id, timeout)?;

        // Extract result
        let result = response.get("result")?;

        // Parse as Hover (result can be null if no hover info available)
        if result.is_null() {
            return None;
        }

        serde_json::from_value(result.clone()).ok()
    }

    /// Check if the language server process is still alive
    pub fn is_alive(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
    }

    /// Check if a document has already been opened in this connection
    pub fn is_document_open(&self) -> bool {
        self.document_version.is_some()
    }

    /// Set the timeout duration for LSP requests
    ///
    /// This timeout applies to goto_definition_async, hover_async, and other
    /// async request methods that support timeout.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout_duration = timeout;
    }

    /// Get the current timeout duration
    pub fn timeout(&self) -> Duration {
        self.timeout_duration
    }

    /// Shutdown the language server and clean up temp directory
    pub fn shutdown(&mut self) {
        if let Some(shutdown_id) = self.send_request("shutdown", serde_json::json!(null)) {
            let _ = self.read_response_for_id(shutdown_id);
        }
        let _ = self.send_notification("exit", serde_json::json!(null));
        let _ = self.process.wait();

        // Clean up temp directory
        if let Some(temp_dir) = self.temp_dir.take() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
    }
}

impl Drop for LanguageServerConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Pool of rust-analyzer connections for reuse across go-to-definition requests
/// Thread-safe via DashMap. Each connection is keyed by a unique name.
pub struct RustAnalyzerPool {
    connections: DashMap<String, (LanguageServerConnection, Instant)>,
}

impl RustAnalyzerPool {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Get or create a rust-analyzer connection for the given key.
    /// Returns None if spawn fails.
    /// The connection is removed from the pool during use and must be returned via `return_connection`.
    pub fn take_connection(&self, key: &str) -> Option<LanguageServerConnection> {
        // Try to take existing connection
        if let Some((_, (mut conn, _))) = self.connections.remove(key) {
            // Check if connection is still alive; if dead, spawn a new one
            if conn.is_alive() {
                return Some(conn);
            }
            // Connection is dead, drop it and spawn a new one
        }
        // Spawn new one
        LanguageServerConnection::spawn_rust_analyzer()
    }

    /// Return a connection to the pool for reuse
    pub fn return_connection(&self, key: &str, conn: LanguageServerConnection) {
        self.connections
            .insert(key.to_string(), (conn, Instant::now()));
    }

    /// Check if the pool has a connection for the given key
    pub fn has_connection(&self, key: &str) -> bool {
        self.connections.contains_key(key)
    }
}

impl Default for RustAnalyzerPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to check if rust-analyzer is available for testing.
    /// Returns true if available, false if should skip test.
    fn check_rust_analyzer_available() -> bool {
        if std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok()
        {
            true
        } else {
            eprintln!("Skipping: rust-analyzer not installed");
            false
        }
    }

    #[test]
    fn language_server_connection_is_alive_returns_true_for_live_process() {
        if !check_rust_analyzer_available() {
            return;
        }

        let mut conn = LanguageServerConnection::spawn_rust_analyzer().unwrap();
        assert!(conn.is_alive());
    }

    #[test]
    fn language_server_connection_is_alive_returns_false_after_shutdown() {
        if !check_rust_analyzer_available() {
            return;
        }

        let mut conn = LanguageServerConnection::spawn_rust_analyzer().unwrap();
        conn.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!conn.is_alive());
    }

    #[test]
    fn rust_analyzer_pool_respawns_dead_connection() {
        if !check_rust_analyzer_available() {
            return;
        }

        let pool = RustAnalyzerPool::new();

        // First take spawns a new connection
        let mut conn = pool.take_connection("test-key").unwrap();
        assert!(conn.is_alive());

        // Kill the process to simulate a crash
        conn.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!conn.is_alive());

        // Return the dead connection to the pool
        pool.return_connection("test-key", conn);

        // Next take should detect the dead connection and respawn
        let mut conn2 = pool.take_connection("test-key").unwrap();
        assert!(
            conn2.is_alive(),
            "Pool should have respawned dead connection"
        );
    }

    #[test]
    fn language_server_connection_tracks_open_documents() {
        if !check_rust_analyzer_available() {
            return;
        }

        let mut conn = LanguageServerConnection::spawn_rust_analyzer().unwrap();

        // Initially, document should not be marked as open
        assert!(!conn.is_document_open());

        // After did_open, document should be marked as open
        conn.did_open("file:///test.rs", "rust", "fn main() {}");
        assert!(conn.is_document_open());
    }

    #[tokio::test]
    async fn goto_definition_returns_none_after_timeout() {
        use std::time::Duration;

        // Create a mock slow server using 'sleep' that never responds
        // We spawn a process that reads stdin but never writes anything back
        let mut process = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdout = process.stdout.take().unwrap();
        let stdout_reader = BufReader::new(stdout);

        let mut conn = LanguageServerConnection {
            process,
            request_id: 0,
            stdout_reader,
            temp_dir: None,
            document_version: None,
            timeout_duration: Duration::from_millis(100), // 100ms timeout for test
        };

        // goto_definition should timeout and return None (not hang forever)
        let result = conn
            .goto_definition_async("file:///test.rs", Position::new(0, 0))
            .await;
        assert!(
            result.is_none(),
            "Expected None due to timeout, but got a response"
        );
    }

    #[test]
    fn timeout_is_configurable_via_set_timeout() {
        use std::time::Duration;

        // Create a mock server
        let mut process = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdout = process.stdout.take().unwrap();
        let stdout_reader = BufReader::new(stdout);

        let mut conn = LanguageServerConnection {
            process,
            request_id: 0,
            stdout_reader,
            temp_dir: None,
            document_version: None,
            timeout_duration: DEFAULT_REQUEST_TIMEOUT,
        };

        // Default timeout should be 5 seconds
        assert_eq!(conn.timeout_duration, Duration::from_secs(5));

        // Set a custom timeout
        conn.set_timeout(Duration::from_secs(10));
        assert_eq!(conn.timeout_duration, Duration::from_secs(10));

        // Can set timeout to a shorter duration
        conn.set_timeout(Duration::from_millis(500));
        assert_eq!(conn.timeout_duration, Duration::from_millis(500));
    }

    #[tokio::test]
    async fn hover_returns_none_after_timeout() {
        use std::time::Duration;

        // Create a mock slow server that never responds
        let mut process = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        let stdout = process.stdout.take().unwrap();
        let stdout_reader = BufReader::new(stdout);

        let mut conn = LanguageServerConnection {
            process,
            request_id: 0,
            stdout_reader,
            temp_dir: None,
            document_version: None,
            timeout_duration: Duration::from_millis(100), // 100ms timeout for test
        };

        // hover_async should timeout and return None (not hang forever)
        let result = conn
            .hover_async("file:///test.rs", Position::new(0, 0))
            .await;
        assert!(
            result.is_none(),
            "Expected None due to timeout, but got a response"
        );
    }

    #[tokio::test]
    async fn timeout_returns_graceful_none_not_panic() {
        use std::time::Duration;

        // Create a mock server that immediately closes stdout (simulates crash/error)
        let mut process = Command::new("true") // 'true' exits immediately with status 0
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();

        // Wait for process to exit so stdout becomes EOF
        let _ = process.wait();

        let stdout = process.stdout.take().unwrap();
        let stdout_reader = BufReader::new(stdout);

        let mut conn = LanguageServerConnection {
            process: Command::new("cat") // Need a live process for struct
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
            request_id: 0,
            stdout_reader,
            temp_dir: None,
            document_version: None,
            timeout_duration: Duration::from_millis(100),
        };

        // Both methods should return None gracefully, not panic
        // Test goto_definition_async
        let goto_result = conn
            .goto_definition_async("file:///test.rs", Position::new(0, 0))
            .await;
        assert!(
            goto_result.is_none(),
            "goto_definition should return None on timeout/error"
        );

        // Test hover_async
        let hover_result = conn
            .hover_async("file:///test.rs", Position::new(0, 0))
            .await;
        assert!(
            hover_result.is_none(),
            "hover should return None on timeout/error"
        );

        // The test passing without panic proves the methods handle
        // timeout/error gracefully with Option<T> return type
    }
}
