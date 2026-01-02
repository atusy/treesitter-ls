//! End-to-end tests for hover using direct LSP communication with treesitter-ls binary.
//!
//! Migrates hover tests from tests/test_lsp_hover.lua to Rust for faster CI execution
//! and deterministic snapshot testing.
//!
//! Run with: `cargo test --test e2e_hover --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_polling::poll_until;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// LSP client for communicating with treesitter-ls binary.
/// TODO: Extract this to shared helpers module (subtask 6)
struct LspClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    request_id: i64,
}

impl LspClient {
    /// Spawn the treesitter-ls binary and create a new LSP client.
    fn new() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn treesitter-ls binary");

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
    fn send_request(&mut self, method: &str, params: Value) -> Value {
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
        self.receive_response_for_id(request_id)
    }

    /// Send an LSP notification (no response expected).
    fn send_notification(&mut self, method: &str, params: Value) {
        let mut notification = serde_json::Map::new();
        notification.insert("jsonrpc".to_string(), json!("2.0"));
        notification.insert("method".to_string(), json!(method));

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
    fn receive_response_for_id(&mut self, expected_id: i64) -> Value {
        loop {
            let message = self.receive_message();

            if let Some(id) = message.get("id") {
                if message.get("method").is_some() {
                    continue;
                }
                if id.as_i64() == Some(expected_id) {
                    return message;
                }
            }
        }
    }

    /// Receive a single LSP message with Content-Length framing.
    fn receive_message(&mut self) -> Value {
        let mut header = String::new();
        loop {
            header.clear();
            self.stdout
                .read_line(&mut header)
                .expect("Failed to read header line");

            if header == "\r\n" {
                continue;
            }

            if header.starts_with("Content-Length:") {
                let len: usize = header
                    .trim_start_matches("Content-Length:")
                    .trim()
                    .parse()
                    .expect("Invalid Content-Length");

                let mut empty = String::new();
                self.stdout
                    .read_line(&mut empty)
                    .expect("Failed to read empty line");

                let mut body = vec![0u8; len];
                std::io::Read::read_exact(&mut self.stdout, &mut body)
                    .expect("Failed to read body");

                return serde_json::from_slice(&body).expect("Failed to parse response");
            }
        }
    }

    /// Kill the server process.
    fn kill(&mut self) {
        let _ = self.child.kill();
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Create a temporary markdown file with Rust code block for hover testing.
/// Equivalent to test_lsp_hover.lua markdown fixture.
fn create_hover_test_markdown_file() -> (String, String, tempfile::NamedTempFile) {
    let content = r#"# Example

```rust
fn main() {
    println!("Hello, world!");
}
```
"#;

    let temp_file = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .expect("Failed to create temp file");

    std::fs::write(temp_file.path(), content).expect("Failed to write temp file");

    let uri = format!("file://{}", temp_file.path().display());

    (uri, content.to_string(), temp_file)
}

/// Test that hover returns content for Rust code in Markdown.
///
/// Migrates from tests/test_lsp_hover.lua:
/// - Cursor on 'main' in fn main() at line 4, column 4 (1-indexed: line 4, col 4)
/// - Expects hover content containing 'main' or 'fn' or indexing message (PBI-149)
///
/// This test verifies the async bridge path works for hover requests.
#[test]
fn test_hover_returns_content() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    let _init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {},
            "initializationOptions": {
                "languages": {
                    "markdown": {
                        "bridge": {
                            "rust": { "enabled": true }
                        }
                    }
                },
                "languageServers": {
                    "rust-analyzer": {
                        "cmd": ["rust-analyzer"],
                        "languages": ["rust"],
                        "workspaceType": "cargo"
                    }
                }
            }
        }),
    );
    client.send_notification("initialized", json!({}));

    // Create and open test file
    let (uri, content, _temp_file) = create_hover_test_markdown_file();
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri,
                "languageId": "markdown",
                "version": 1,
                "text": content
            }
        }),
    );

    // Request hover at position of "main" on line 3, column 3 (0-indexed)
    // In the Lua test: line 4, column 4 (1-indexed) - the 'm' of main
    // Retry to wait for rust-analyzer indexing
    let hover_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/hover",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 3,
                    "character": 3
                }
            }),
        );

        // Verify we got a response
        assert!(
            response.get("result").is_some(),
            "Hover response should have result: {:?}",
            response
        );

        let result = response.get("result").unwrap().clone();

        // Result can be Hover object or null
        if !result.is_null() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        hover_result.is_some(),
        "Hover should return content for valid position after retries"
    );

    let hover = hover_result.unwrap();

    // Verify hover has contents field
    assert!(
        hover.get("contents").is_some(),
        "Hover result should have contents: {:?}",
        hover
    );

    // Extract contents as string for validation
    let contents_value = hover.get("contents").unwrap();
    let contents_str = match contents_value {
        Value::String(s) => s.clone(),
        Value::Object(obj) => {
            // MarkedString or MarkupContent
            if let Some(value) = obj.get("value") {
                value.as_str().unwrap_or("").to_string()
            } else {
                format!("{:?}", obj)
            }
        }
        Value::Array(arr) => {
            // Array of MarkedString
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => format!("{:?}", contents_value),
    };

    // Verify contents contains expected information
    // Either real hover content ('main' or 'fn') or indexing message (PBI-149)
    let has_valid_content = contents_str.contains("main")
        || contents_str.contains("fn")
        || contents_str.contains("rust-analyzer")
        || contents_str.contains("indexing");

    assert!(
        has_valid_content,
        "Hover contents should contain function info or indexing status, got: {}",
        contents_str
    );
}
