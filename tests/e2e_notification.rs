//! End-to-end tests for notification infrastructure.
//!
//! Verifies that progress notifications and other notification forwarding
//! mechanisms work correctly through the async bridge.
//!
//! Based on tests/test_lsp_notification.lua which verifies:
//! - LSP attaches successfully
//! - eager_spawn_for_injections is triggered for code blocks
//! - rust-analyzer bridge is configured and spawning works
//! - Progress notifications can be forwarded (when rust-analyzer sends them)
//!
//! Note: Actual progress messages depend on rust-analyzer having work to do
//! (e.g., loading crates). For simple projects without dependencies,
//! rust-analyzer may not send progress notifications.
//!
//! Run with: `cargo test --test e2e_notification --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lsp_init::initialize_with_rust_bridge;
use serde_json::json;
use std::time::Duration;

/// Create a temporary markdown file with Rust code block.
///
/// This triggers eager_spawn_for_injections when opened.
fn create_rust_code_block_fixture() -> (String, String, tempfile::NamedTempFile) {
    let content = r#"# Rust Code Block Test

This file contains a Rust code block for testing bridge server functionality.

```rust
fn main() {
    let x = 42;
    println!("{}", x);
}
```
"#;

    let temp_file = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .expect("Failed to create temp file");

    std::fs::write(temp_file.path(), content).expect("Failed to write temp file");

    let uri = url::Url::from_file_path(temp_file.path())
        .expect("Failed to construct file URI")
        .to_string();

    (uri, content.to_string(), temp_file)
}

/// Test that progress notification infrastructure is set up correctly.
///
/// This test verifies:
/// 1. LSP server starts and accepts connections
/// 2. didOpen notification is processed without errors
/// 3. Server remains responsive after opening a file with Rust code block
/// 4. Progress notifications can be received (though rust-analyzer may not send any)
///
/// The infrastructure is considered working if the server:
/// - Accepts initialization
/// - Processes didOpen for markdown with Rust injection
/// - Remains responsive (doesn't crash)
/// - Can handle subsequent requests
#[test]
fn test_progress_notification_infrastructure() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open markdown file with Rust code block
    // This should trigger eager_spawn_for_injections
    let (uri, content, _temp_file) = create_rust_code_block_fixture();

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

    // Give server time to process didOpen and potentially spawn rust-analyzer
    // The eager_spawn_for_injections happens asynchronously after didOpen
    std::thread::sleep(Duration::from_millis(2000));

    // Verify server is still responsive by sending a request
    // If the notification infrastructure has issues, the server might crash or hang
    let response = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 0 }
        }),
    );

    // The server should respond (even if hover returns null for markdown header)
    assert!(
        response.get("result").is_some(),
        "Server should remain responsive after opening file with Rust injection"
    );

    // Note: We don't assert on progress notifications being received because
    // rust-analyzer only sends them when it has work to do (loading crates, indexing).
    // For simple projects without dependencies, no progress notifications are sent.
    //
    // The infrastructure test passes if:
    // - Server initialized successfully
    // - didOpen was processed without crashing
    // - Server remains responsive
    //
    // In production with real projects that have dependencies, rust-analyzer
    // will send progress notifications like "Loading crates", "Indexing", etc.
    // which will be forwarded through the notification infrastructure.
}

/// Test that server handles didOpen for files with multiple injections.
///
/// Verifies that opening a file with multiple code blocks doesn't cause issues.
#[test]
fn test_multiple_injection_didopen() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create markdown with multiple code blocks
    let content = r#"# Multiple Code Blocks

First Rust block:

```rust
fn foo() {
    println!("foo");
}
```

Second Rust block:

```rust
fn bar() {
    println!("bar");
}
```
"#;

    let temp_file = tempfile::Builder::new()
        .suffix(".md")
        .tempfile()
        .expect("Failed to create temp file");

    std::fs::write(temp_file.path(), content).expect("Failed to write temp file");

    let uri = url::Url::from_file_path(temp_file.path())
        .expect("Failed to construct file URI")
        .to_string();

    // Send didOpen
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

    // Give server time to process
    std::thread::sleep(Duration::from_millis(1000));

    // Verify server is still responsive
    let response = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 0, "character": 0 }
        }),
    );

    assert!(
        response.get("result").is_some(),
        "Server should handle multiple injections without issues"
    );
}

/// Test that notifications don't block requests.
///
/// Verifies that the server can handle requests even if notification
/// processing is happening in the background.
#[test]
fn test_notifications_dont_block_requests() {
    let mut client = LspClient::new();

    // Initialize
    initialize_with_rust_bridge(&mut client);

    // Create test file
    let (uri, content, _temp_file) = create_rust_code_block_fixture();

    // Send didOpen (triggers notification processing)
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

    // Immediately send a request without waiting
    // This tests that notifications are non-blocking
    let response = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri },
            "position": { "line": 5, "character": 3 }
        }),
    );

    // Request should complete successfully
    assert!(
        response.get("result").is_some(),
        "Requests should not be blocked by notification processing"
    );
}

/// Test that didClose notifications are processed correctly.
///
/// Verifies the server handles document close notifications.
#[test]
fn test_didclose_notification() {
    let mut client = LspClient::new();

    // Initialize
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_rust_code_block_fixture();

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

    std::thread::sleep(Duration::from_millis(500));

    // Send didClose notification
    client.send_notification(
        "textDocument/didClose",
        json!({
            "textDocument": {
                "uri": uri
            }
        }),
    );

    std::thread::sleep(Duration::from_millis(100));

    // Server should still be running after didClose
    // We can verify by sending a request to another document
    let (uri2, content2, _temp_file2) = create_rust_code_block_fixture();

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": uri2,
                "languageId": "markdown",
                "version": 1,
                "text": content2
            }
        }),
    );

    std::thread::sleep(Duration::from_millis(100));

    let response = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": uri2 },
            "position": { "line": 0, "character": 0 }
        }),
    );

    assert!(
        response.get("result").is_some(),
        "Server should remain responsive after didClose notification"
    );
}
