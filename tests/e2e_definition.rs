//! End-to-end tests for go-to-definition using direct LSP communication with treesitter-ls binary.
//!
//! These tests spawn the treesitter-ls binary and communicate via LSP protocol,
//! enabling faster and more reliable E2E testing without Neovim dependency.
//!
//! Run with: `cargo test --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lsp_polling::poll_until;
use serde_json::{Value, json};
use std::process::{Child, Command, Stdio};

/// Spawn the treesitter-ls binary as an LSP server.
fn spawn_lsp_server() -> Child {
    Command::new(env!("CARGO_BIN_EXE_treesitter-ls"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn treesitter-ls binary")
}

#[test]
fn test_spawn_binary_starts_process() {
    let mut child = spawn_lsp_server();

    // Process should be running (not immediately exited)
    // We check by trying to get exit status without waiting
    let status = child.try_wait().expect("Error checking child status");

    // Process should still be running (waiting for LSP messages on stdin)
    assert!(
        status.is_none(),
        "Process should not exit immediately - it should wait for LSP input"
    );

    // Clean up: kill the process
    child.kill().expect("Failed to kill child process");
}

#[test]
fn test_initialize_returns_capabilities() {
    let mut client = LspClient::new();

    // Send initialize request
    let response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {}
        }),
    );

    // Verify response structure
    assert!(
        response.get("result").is_some(),
        "Initialize response should have result: {:?}",
        response
    );

    let result = response.get("result").unwrap();

    // Verify capabilities exist
    assert!(
        result.get("capabilities").is_some(),
        "InitializeResult should have capabilities: {:?}",
        result
    );

    let capabilities = result.get("capabilities").unwrap();

    // Verify some expected capabilities for treesitter-ls
    assert!(
        capabilities.get("textDocumentSync").is_some(),
        "Server should support textDocumentSync: {:?}",
        capabilities
    );

    // treesitter-ls should support definition (for bridging)
    assert!(
        capabilities.get("definitionProvider").is_some(),
        "Server should support definitionProvider: {:?}",
        capabilities
    );
}

/// Create a temporary markdown file with Rust code block for testing.
/// Returns the file URI and the content for reference.
fn create_test_markdown_file() -> (String, String, tempfile::NamedTempFile) {
    let content = r#"Here is a function definition:

```rust
fn example() {
    println!("Hello, world!");
}

fn main() {
    example();
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

#[test]
fn test_did_open_after_initialize() {
    let mut client = LspClient::new();

    // Initialize handshake
    let _init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {}
        }),
    );

    // Send initialized notification (required by LSP protocol)
    client.send_notification("initialized", json!({}));

    // Create test file
    let (uri, content, _temp_file) = create_test_markdown_file();

    // Send didOpen notification
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

    // Give the server time to process (notifications don't have responses)
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Verify server is still running (didn't crash on didOpen)
    let status = client
        .child()
        .try_wait()
        .expect("Error checking child status");
    assert!(
        status.is_none(),
        "Server should still be running after didOpen"
    );
}

#[test]
fn test_definition_returns_location() {
    let mut client = LspClient::new();

    // Initialize handshake with bridge configuration
    // This matches the minimal_init.lua setup used by Neovim E2E tests
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
    let (uri, content, _temp_file) = create_test_markdown_file();
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

    // Request definition at position of "example()" call on line 8, column 4 (on 'e')
    // Line 8 (0-indexed): "    example();"
    // Column 4 is the 'e' of example
    //
    // Retry up to 20 times with 500ms delay (10 seconds total) to wait for
    // rust-analyzer to finish indexing. This mirrors the Neovim E2E test behavior.
    let result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/definition",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 8,
                    "character": 4
                }
            }),
        );

        // Verify we got a response
        assert!(
            response.get("result").is_some(),
            "Definition response should have result: {:?}",
            response
        );

        let result = response.get("result").unwrap().clone();

        // Result can be Location, Location[], LocationLink[], or null
        // treesitter-ls bridges to rust-analyzer which typically returns LocationLink[]
        if !result.is_null() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        result.is_some(),
        "Definition result should not be null for valid position after retries"
    );
}

/// Sanitize definition response by replacing temp file URIs with a stable placeholder.
fn sanitize_definition_response(result: &Value) -> Value {
    match result {
        Value::Array(locations) => {
            Value::Array(
                locations
                    .iter()
                    .map(|loc| {
                        let mut loc = loc.clone();
                        // For LocationLink, sanitize targetUri
                        if let Some(uri) = loc.get_mut("targetUri") {
                            *uri = Value::String("<TEST_FILE_URI>".to_string());
                        }
                        // For Location, sanitize uri
                        if let Some(uri) = loc.get_mut("uri") {
                            *uri = Value::String("<TEST_FILE_URI>".to_string());
                        }
                        loc
                    })
                    .collect(),
            )
        }
        Value::Object(loc) => {
            let mut loc = loc.clone();
            if let Some(uri) = loc.get_mut("targetUri") {
                *uri = Value::String("<TEST_FILE_URI>".to_string());
            }
            if let Some(uri) = loc.get_mut("uri") {
                *uri = Value::String("<TEST_FILE_URI>".to_string());
            }
            Value::Object(loc)
        }
        _ => result.clone(),
    }
}

#[test]
fn test_definition_snapshot() {
    let mut client = LspClient::new();

    // Initialize handshake with bridge configuration
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
    let (uri, content, _temp_file) = create_test_markdown_file();
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

    // Request definition at position of "example()" call on line 8, column 4
    // Retry until we get a non-null response
    let result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": 8, "character": 4 }
            }),
        );

        let result = response.get("result").cloned().unwrap_or(Value::Null);
        if !result.is_null() {
            Some(result)
        } else {
            None
        }
    });

    assert!(result.is_some(), "Expected non-null definition result");
    let result = result.unwrap();

    // Sanitize the result for snapshot comparison (replace temp file URI)
    let sanitized = sanitize_definition_response(&result);

    // Use insta snapshot testing
    insta::assert_json_snapshot!("definition_response", sanitized);
}

/// Test that verifies Rust E2E produces equivalent results to Neovim E2E.
///
/// The Neovim test (test_lsp_definition.lua) tests:
/// - Cursor on line 9 (1-indexed), column 5 - the `example()` call
/// - Expects jump to line 4 (1-indexed) - the `fn example()` definition
///
/// This test verifies the same behavior using 0-indexed positions:
/// - Cursor on line 8, column 4 - the `example()` call
/// - Expects definition at line 3 - the `fn example()` definition
#[test]
fn test_definition_matches_neovim_behavior() {
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

    // Create and open test file (same content structure as Neovim test)
    let (uri, content, _temp_file) = create_test_markdown_file();
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

    // Neovim test: cursor on line 9 (1-indexed), column 5 (on 'e' of example)
    // 0-indexed: line 8, column 4
    let cursor_line = 8; // 0-indexed (Neovim line 9)
    let cursor_col = 4; // 0-indexed (Neovim column 5)

    // Neovim test: expects jump to line 4 (1-indexed)
    // 0-indexed: line 3
    let expected_definition_line = 3; // 0-indexed (Neovim line 4)

    // Request definition with retry
    let definition_line = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": cursor_line, "character": cursor_col }
            }),
        );

        if let Some(result) = response.get("result") {
            if !result.is_null() {
                // Extract line from first location in array
                if let Some(locations) = result.as_array() {
                    if let Some(first) = locations.first() {
                        if let Some(range) = first.get("range") {
                            if let Some(start) = range.get("start") {
                                if let Some(line) = start.get("line").and_then(|l| l.as_u64()) {
                                    return Some(line);
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    });

    let actual_line = definition_line.expect("Should get definition response with line number");

    // Verify the definition jumps to the same line as Neovim E2E test
    assert_eq!(
        actual_line,
        expected_definition_line as u64,
        "Definition should jump to line {} (0-indexed) / line {} (1-indexed, Neovim), \
         matching Neovim E2E test behavior. Got line {} (0-indexed).",
        expected_definition_line,
        expected_definition_line + 1,
        actual_line
    );
}

#[test]
fn test_shutdown_terminates_cleanly() {
    let mut client = LspClient::new();

    // Initialize handshake
    let _init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {}
        }),
    );
    client.send_notification("initialized", json!({}));

    // Verify server is running
    let status = client
        .child()
        .try_wait()
        .expect("Error checking child status");
    assert!(status.is_none(), "Server should be running before shutdown");

    // Send shutdown request (server should acknowledge but stay running)
    // Note: LSP shutdown takes no params
    let shutdown_response = client.send_request("shutdown", json!(null));
    assert!(
        shutdown_response.get("result").is_some(),
        "Shutdown should return a result: {:?}",
        shutdown_response
    );

    // Server should still be running after shutdown (waiting for exit notification)
    let status = client
        .child()
        .try_wait()
        .expect("Error checking child status");
    assert!(
        status.is_none(),
        "Server should still be running after shutdown, waiting for exit notification"
    );

    // Send exit notification (server should terminate)
    client.send_notification("exit", json!(null));

    // Close stdin to signal EOF - this helps tower-lsp's server to exit
    client.close_stdin();

    // Wait for process to exit (up to 2 seconds)
    let mut status = None;
    for _ in 0..20 {
        status = client
            .child()
            .try_wait()
            .expect("Error checking child status");
        if status.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Verify process has exited
    assert!(
        status.is_some(),
        "Server should have exited after exit notification"
    );

    // Verify clean exit (exit code 0)
    let exit_status = status.unwrap();
    assert!(
        exit_status.success(),
        "Server should exit cleanly with code 0, got: {:?}",
        exit_status
    );
}
