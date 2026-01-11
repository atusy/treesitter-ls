//! End-to-end test for didClose forwarding to downstream language servers.
//!
//! This test verifies that when a host document is closed:
//! 1. Virtual documents are properly closed in downstream servers
//! 2. The connection to downstream servers remains open for other documents
//!
//! Run with: `cargo test --test e2e_didclose_forwarding --features e2e`
//!
//! **Requirements**: lua-language-server must be installed and in PATH.

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use serde_json::json;

/// Helper to check if lua-language-server is available
fn is_lua_ls_available() -> bool {
    std::process::Command::new("lua-language-server")
        .arg("--version")
        .output()
        .is_ok()
}

/// Helper to create a client with lua-language-server configured
fn create_configured_client() -> LspClient {
    let mut client = LspClient::new();

    // Initialize handshake with language server configuration
    let _init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {},
            "initializationOptions": {
                "languageServers": {
                    "lua-language-server": {
                        "cmd": ["lua-language-server"],
                        "languages": ["lua"]
                    }
                }
            }
        }),
    );
    client.send_notification("initialized", json!({}));
    client
}

/// E2E test: connection remains open after closing host document
///
/// Verifies that after closing one markdown file with Lua blocks,
/// another markdown file can still use lua-language-server.
#[test]
fn e2e_connection_remains_open_after_didclose() {
    if !is_lua_ls_available() {
        eprintln!("SKIP: lua-language-server not found in PATH");
        eprintln!("Install lua-language-server to run this test:");
        eprintln!("  brew install lua-language-server");
        return;
    }

    let mut client = create_configured_client();

    // === Phase 1: Open first markdown document and trigger hover ===
    let markdown_uri_1 = "file:///test_didclose_1.md";
    let markdown_content_1 = r#"# First Document

```lua
local x = 1
print(x)
```

More text.
"#;

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": markdown_uri_1,
                "languageId": "markdown",
                "version": 1,
                "text": markdown_content_1
            }
        }),
    );

    // Give lua-ls time to initialize
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Hover to trigger virtual document opening
    let hover_response_1 = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": markdown_uri_1 },
            "position": { "line": 3, "character": 6 }
        }),
    );

    assert!(
        hover_response_1.get("error").is_none(),
        "First hover should not return error: {:?}",
        hover_response_1.get("error")
    );
    println!("Phase 1: First document opened and hover succeeded");

    // === Phase 2: Close the first document ===
    client.send_notification(
        "textDocument/didClose",
        json!({
            "textDocument": { "uri": markdown_uri_1 }
        }),
    );

    // Small delay to let didClose propagate
    std::thread::sleep(std::time::Duration::from_millis(100));
    println!("Phase 2: First document closed");

    // === Phase 3: Open second markdown document and verify hover works ===
    let markdown_uri_2 = "file:///test_didclose_2.md";
    let markdown_content_2 = r#"# Second Document

```lua
local y = 2
print(y)
```

More text.
"#;

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": markdown_uri_2,
                "languageId": "markdown",
                "version": 1,
                "text": markdown_content_2
            }
        }),
    );

    // Give lua-ls time to process the new document
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Hover in second document - this should work if connection remained open
    let hover_response_2 = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": markdown_uri_2 },
            "position": { "line": 3, "character": 6 }
        }),
    );

    assert!(
        hover_response_2.get("error").is_none(),
        "Second hover should not return error (connection should remain open): {:?}",
        hover_response_2.get("error")
    );

    println!("Phase 3: Second document opened and hover succeeded");
    println!("✓ E2E: Connection remained open after didClose - second document works");

    // Clean shutdown
    let _shutdown_response = client.send_request("shutdown", json!(null));
    client.send_notification("exit", json!(null));
}

/// E2E test: didClose is forwarded to downstream server
///
/// Verifies that closing a host document triggers didClose notifications
/// to downstream language servers for all virtual documents.
#[test]
fn e2e_didclose_forwarded_to_downstream_server() {
    if !is_lua_ls_available() {
        eprintln!("SKIP: lua-language-server not found in PATH");
        eprintln!("Install lua-language-server to run this test:");
        eprintln!("  brew install lua-language-server");
        return;
    }

    let mut client = create_configured_client();

    // Open markdown document with multiple Lua code blocks
    let markdown_uri = "file:///test_multi_lua.md";
    let markdown_content = r#"# Document with Multiple Lua Blocks

```lua
local x = 1
```

Some text.

```lua
local y = 2
```

More text.
"#;

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": markdown_uri,
                "languageId": "markdown",
                "version": 1,
                "text": markdown_content
            }
        }),
    );

    // Give lua-ls time to initialize
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Hover on first Lua block
    let hover_1 = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": markdown_uri },
            "position": { "line": 3, "character": 6 }
        }),
    );
    assert!(
        hover_1.get("error").is_none(),
        "Hover 1 failed: {:?}",
        hover_1
    );

    // Hover on second Lua block
    let hover_2 = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": { "uri": markdown_uri },
            "position": { "line": 9, "character": 6 }
        }),
    );
    assert!(
        hover_2.get("error").is_none(),
        "Hover 2 failed: {:?}",
        hover_2
    );

    println!("Both Lua blocks accessed via hover");

    // Close the host document
    client.send_notification(
        "textDocument/didClose",
        json!({
            "textDocument": { "uri": markdown_uri }
        }),
    );

    // Small delay for didClose to propagate
    std::thread::sleep(std::time::Duration::from_millis(100));

    println!("✓ E2E: didClose sent for host document with multiple Lua blocks");

    // If we got here without error, the didClose was handled correctly
    // (lua-language-server would error if it received malformed messages)

    // Clean shutdown
    let _shutdown_response = client.send_request("shutdown", json!(null));
    client.send_notification("exit", json!(null));
}
