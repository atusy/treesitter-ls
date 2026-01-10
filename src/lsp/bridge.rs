//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.
//!
//! NOTE: Walking skeleton (PBI-301) - types are currently only used in tests.
//! Dead code warnings are suppressed until integration with the main LSP server.

// Walking skeleton: suppress dead_code warnings until fully integrated
#![allow(dead_code)]

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

/// Virtual document URI for injection regions.
///
/// Encodes host URI + injection language + region ID into a unique URI scheme
/// that downstream language servers can use to identify virtual documents.
///
/// Format: `tsls-virtual://{language}/{region_id}?host={url_encoded_host_uri}`
///
/// Example: `tsls-virtual://lua/region-0?host=file%3A%2F%2F%2Fproject%2Fdoc.md`
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtualDocumentUri {
    host_uri: tower_lsp::lsp_types::Url,
    language: String,
    region_id: String,
}

impl VirtualDocumentUri {
    /// Create a new virtual document URI for an injection region.
    ///
    /// # Arguments
    /// * `host_uri` - The URI of the host document (e.g., markdown file)
    /// * `language` - The injection language (e.g., "lua", "python")
    /// * `region_id` - Unique identifier for this injection region within the host
    pub(crate) fn new(
        host_uri: &tower_lsp::lsp_types::Url,
        language: &str,
        region_id: &str,
    ) -> Self {
        Self {
            host_uri: host_uri.clone(),
            language: language.to_string(),
            region_id: region_id.to_string(),
        }
    }

    /// Parse a virtual document URI from a URI string.
    ///
    /// Returns None if the URI is not a valid tsls-virtual:// URI.
    pub(crate) fn parse(uri_str: &str) -> Option<Self> {
        use tower_lsp::lsp_types::Url;
        use percent_encoding::percent_decode_str;

        let url = Url::parse(uri_str).ok()?;

        // Check scheme
        if url.scheme() != "tsls-virtual" {
            return None;
        }

        // Extract language from host (authority) part
        let language = url.host_str()?.to_string();

        // Extract region_id from path (strip leading /)
        let region_id = url.path().strip_prefix('/')?.to_string();

        // Extract host URI from query parameter
        let query = url.query()?;
        let host_encoded = query.strip_prefix("host=")?;
        let host_decoded = percent_decode_str(host_encoded)
            .decode_utf8()
            .ok()?
            .to_string();
        let host_uri = Url::parse(&host_decoded).ok()?;

        Some(Self {
            host_uri,
            language,
            region_id,
        })
    }

    /// Get the host document URI.
    pub(crate) fn host_uri(&self) -> &tower_lsp::lsp_types::Url {
        &self.host_uri
    }

    /// Get the injection language.
    pub(crate) fn language(&self) -> &str {
        &self.language
    }

    /// Get the region ID.
    pub(crate) fn region_id(&self) -> &str {
        &self.region_id
    }

    /// Convert to a URI string.
    ///
    /// Format: `tsls-virtual://{language}/{region_id}?host={url_encoded_host_uri}`
    pub(crate) fn to_uri_string(&self) -> String {
        use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

        // Encode all characters that are not safe in query strings
        const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
            .add(b' ')
            .add(b'"')
            .add(b'#')
            .add(b'<')
            .add(b'>')
            .add(b'`')
            .add(b'?')
            .add(b'{')
            .add(b'}')
            .add(b'/')
            .add(b':')
            .add(b'@')
            .add(b'%');

        let host_encoded = utf8_percent_encode(self.host_uri.as_str(), QUERY_ENCODE_SET);
        format!(
            "tsls-virtual://{}/{}?host={}",
            self.language, self.region_id, host_encoded
        )
    }
}

/// Build a JSON-RPC hover request for a downstream language server.
///
/// Transforms the host document position and URI to virtual document coordinates
/// for the injection region.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_position` - The position in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_hover_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    // Translate position from host to virtual coordinates
    let virtual_position = tower_lsp::lsp_types::Position {
        line: host_position.line - region_start_line,
        character: host_position.character,
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "textDocument/hover",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            },
            "position": {
                "line": virtual_position.line,
                "character": virtual_position.character
            }
        }
    })
}

/// Transform a hover response from virtual to host document coordinates.
///
/// If the response contains a range, translates the line numbers from virtual
/// document coordinates back to host document coordinates by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_hover_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Check if response has a result with a range
    if let Some(result) = response.get_mut("result")
        && result.is_object()
        && let Some(range) = result.get_mut("range")
        && range.is_object()
    {
        // Transform start position
        if let Some(start) = range.get_mut("start")
            && let Some(line) = start.get_mut("line")
            && let Some(line_num) = line.as_u64()
        {
            *line = serde_json::json!(line_num + region_start_line as u64);
        }

        // Transform end position
        if let Some(end) = range.get_mut("end")
            && let Some(line) = end.get_mut("line")
            && let Some(line_num) = line.as_u64()
        {
            *line = serde_json::json!(line_num + region_start_line as u64);
        }
    }

    response
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
        if let Some(id) = RequestId::from_json(response)
            && let Some((_, sender)) = self.inner.remove(&id)
        {
            // Ignore send error - receiver may have been dropped
            let _ = sender.send(response.clone());
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
        let conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // The connection should have a child process ID
        assert!(conn.child_id().is_some(), "child process should have an ID");
    }

    /// RED: Test that send_request writes JSON-RPC message with Content-Length header
    #[tokio::test]
    async fn send_request_writes_json_rpc_with_content_length() {
        use serde_json::json;

        // Use `cat` to echo what we write to stdin back to stdout
        let cmd = vec!["cat".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // Send a simple JSON-RPC request
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        conn.write_message(&request)
            .await
            .expect("write should succeed");

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
        let mut conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // Write a JSON-RPC response message
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {}
            }
        });

        conn.write_message(&response)
            .await
            .expect("write should succeed");

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
        let conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

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
            conn.write_message(&response)
                .await
                .expect("write should succeed");
        }

        // Wait for reader task
        reader_task.await.expect("reader task should complete");

        // The pending request should receive the response
        let result = response_rx.await.expect("should receive response");
        assert_eq!(result["id"], 42);
        assert_eq!(result["result"]["value"], "hello");
    }

    /// Integration test: Initialize lua-language-server and verify response
    #[tokio::test]
    async fn initialize_lua_language_server_logs_success() {
        use serde_json::json;

        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        // Spawn lua-language-server
        let cmd = vec!["lua-language-server".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("should spawn lua-language-server");

        // Send initialize request
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {}
            }
        });

        conn.write_message(&init_request)
            .await
            .expect("should write initialize request");

        // Read initialize response (may need to skip notifications)
        let response = loop {
            let msg = conn.read_message().await.expect("should read message");
            // Skip notifications (messages without id that have a method)
            if msg.get("id").is_some() {
                break msg;
            }
            // It's a notification, continue reading
            log::debug!(
                target: "treesitter_ls::bridge::test",
                "Received notification: {:?}",
                msg.get("method")
            );
        };

        // Verify the response indicates successful initialization
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response["result"].is_object(), "should have result object");
        assert!(
            response["result"]["capabilities"].is_object(),
            "should have capabilities in result"
        );

        // Log successful initialization (as required by AC2)
        log::info!(
            target: "treesitter_ls::bridge",
            "lua-language-server initialized successfully"
        );

        // Send initialized notification
        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        conn.write_message(&initialized)
            .await
            .expect("should write initialized notification");
    }

    /// Integration test: Dropping connection terminates child process
    #[tokio::test]
    async fn drop_terminates_child_process() {
        // Spawn a long-running process that we can check is terminated
        // Using `sleep` as it will run indefinitely until killed
        let cmd = vec!["sleep".to_string(), "3600".to_string()];
        let conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("should spawn sleep process");

        let child_id = conn.child_id().expect("should have child ID");

        // Verify process is running before drop
        assert!(
            is_process_running(child_id),
            "child process should be running before drop"
        );

        // Drop the connection
        drop(conn);

        // Give the OS a moment to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify process is no longer running after drop
        assert!(
            !is_process_running(child_id),
            "child process should be terminated after drop"
        );
    }

    /// Check if a process with the given PID is still running
    fn is_process_running(pid: u32) -> bool {
        // Use kill -0 via Command to check if process exists
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// RED: Test VirtualDocumentUri creates scheme for injection region (PBI-302 Subtask 1)
    #[test]
    fn virtual_document_uri_creates_scheme_for_injection_region() {
        use tower_lsp::lsp_types::Url;

        // Create a virtual document URI for a Lua injection in a markdown file
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let injection_language = "lua";
        let region_id = "region-0";

        let virtual_uri = VirtualDocumentUri::new(&host_uri, injection_language, region_id);

        // The virtual URI should encode all three pieces of information
        assert_eq!(virtual_uri.host_uri(), &host_uri);
        assert_eq!(virtual_uri.language(), "lua");
        assert_eq!(virtual_uri.region_id(), "region-0");

        // The URI string should use a custom scheme
        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.starts_with("tsls-virtual://"),
            "URI should use tsls-virtual:// scheme: {}",
            uri_string
        );
        assert!(
            uri_string.contains("lua"),
            "URI should contain injection language: {}",
            uri_string
        );
    }

    /// RED: Test VirtualDocumentUri can be parsed back from URI string (PBI-302 Subtask 1)
    #[test]
    fn virtual_document_uri_roundtrip() {
        use tower_lsp::lsp_types::Url;

        let host_uri = Url::parse("file:///project/readme.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "python", "region-42");

        // Convert to URI string
        let uri_string = virtual_uri.to_uri_string();

        // Parse back
        let parsed = VirtualDocumentUri::parse(&uri_string).expect("should parse virtual URI");

        // Verify roundtrip preserves all data
        assert_eq!(parsed.host_uri(), &host_uri);
        assert_eq!(parsed.language(), "python");
        assert_eq!(parsed.region_id(), "region-42");
    }

    /// RED: Test bridge hover request uses virtual URI and mapped position (PBI-302 Subtask 4)
    #[test]
    fn bridge_hover_request_uses_virtual_uri_and_mapped_position() {
        use tower_lsp::lsp_types::{Position, Url};

        // Create a hover request builder for bridge
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_id = "region-0";
        let injection_language = "lua";

        // The region starts at line 3 in the host document
        let region_start_line = 3;

        // Build the hover request for downstream language server
        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            42, // request ID
        );

        // Verify the request structure
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/hover");

        // The params should use virtual URI
        let text_doc = &request["params"]["textDocument"];
        let uri_str = text_doc["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("tsls-virtual://lua/"),
            "Request should use virtual URI: {}",
            uri_str
        );

        // The position should be translated (line 5 - region_start 3 = line 2)
        let position = &request["params"]["position"];
        assert_eq!(
            position["line"], 2,
            "Position line should be translated to virtual coordinates"
        );
        assert_eq!(
            position["character"], 10,
            "Position character should remain unchanged"
        );
    }

    /// RED: Test bridge hover response transforms range to host coordinates (PBI-302 Subtask 5)
    #[test]
    fn bridge_hover_response_transforms_range_to_host_coordinates() {
        use serde_json::json;

        // Simulate a hover response from lua-language-server with a range
        // The range is in virtual document coordinates (starting at line 0)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": {
                    "kind": "markdown",
                    "value": "```lua\nfunction greet(name: string)\n```"
                },
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }
        });

        // The injection region starts at line 3 in the host document
        let region_start_line = 3;

        // Transform the response to host coordinates
        let transformed = transform_hover_response_to_host(response, region_start_line);

        // Verify the contents are unchanged
        assert_eq!(
            transformed["result"]["contents"]["kind"], "markdown",
            "Contents should be preserved"
        );

        // Verify the range is transformed to host coordinates
        let range = &transformed["result"]["range"];
        assert_eq!(
            range["start"]["line"], 3,
            "Start line should be translated to host (0 + 3 = 3)"
        );
        assert_eq!(
            range["start"]["character"], 9,
            "Start character should remain unchanged"
        );
        assert_eq!(
            range["end"]["line"], 3,
            "End line should be translated to host (0 + 3 = 3)"
        );
        assert_eq!(
            range["end"]["character"], 14,
            "End character should remain unchanged"
        );
    }

    /// Test hover response without range is passed through unchanged
    #[test]
    fn bridge_hover_response_without_range_unchanged() {
        use serde_json::json;

        // Hover response without a range (just contents)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": "Simple hover text"
            }
        });

        let region_start_line = 5;
        let transformed = transform_hover_response_to_host(response.clone(), region_start_line);

        // Response should be unchanged (no range to transform)
        assert_eq!(transformed, response);
    }

    /// Test hover response with null result is passed through unchanged
    #[test]
    fn bridge_hover_response_null_result_unchanged() {
        use serde_json::json;

        // Hover response with null result (no hover info available)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let region_start_line = 5;
        let transformed = transform_hover_response_to_host(response.clone(), region_start_line);

        // Response should be unchanged
        assert_eq!(transformed, response);
    }
}
