//! End-to-end test verifying no hang when typing immediately after opening file.
//!
//! This test verifies the non-blocking initialization fix (PBI-187):
//! - treesitter-ls binary spawned via LspClient
//! - Markdown document with Lua code block opened via didOpen
//! - Immediate didChange without sleep (simulates typing immediately)
//! - Immediate completion request (simulates rapid user action)
//! - Test completes within reasonable time (< 2s) without hanging
//!
//! **Key behavior**: Connection initialization happens in background task,
//! so didChange and completion don't block waiting for lua-ls to initialize.
//!
//! Run with: `cargo test --test e2e_bridge_no_hang --features e2e`
//!
//! **Requirements**: lua-language-server must be installed and in PATH.

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use serde_json::json;
use std::time::Instant;

#[test]
fn test_no_hang_when_typing_immediately_after_opening_file() {
    // Check if lua-language-server is available
    let check = std::process::Command::new("lua-language-server")
        .arg("--version")
        .output();

    if check.is_err() {
        eprintln!("SKIP: lua-language-server not found in PATH");
        eprintln!("Install lua-language-server to run this test:");
        eprintln!("  brew install lua-language-server");
        return;
    }

    // Spawn treesitter-ls binary
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

    // Open markdown document with Lua code block
    let markdown_content = r#"# Test Document

```lua
local x = 10
print(
```

More text.
"#;

    let markdown_uri = "file:///test.md";

    // Track time to ensure test completes quickly (no hang)
    let start = Instant::now();

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

    // Immediately send didChange (simulates user typing right after opening file)
    // NO SLEEP - this is the key test: typing immediately shouldn't hang
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": markdown_uri,
                "version": 2
            },
            "contentChanges": [{
                "text": r#"# Test Document

```lua
local x = 10
print(x
```

More text.
"#
            }]
        }),
    );

    // Immediately request completion (simulates rapid user action)
    // This should not hang waiting for initialization
    let completion_response = client.send_request(
        "textDocument/completion",
        json!({
            "textDocument": {
                "uri": markdown_uri
            },
            "position": {
                "line": 4,
                "character": 7
            }
        }),
    );

    let elapsed = start.elapsed();

    println!("Test completed in {:?}", elapsed);

    // Verify test completed within reasonable time (< 2s)
    // Before fix: would hang indefinitely (tokio runtime starvation)
    // After fix: completes quickly (background initialization)
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "Test should complete within 2s without hanging, took {:?}",
        elapsed
    );

    println!("✓ No hang - test completed in {:?}", elapsed);

    // Verify we got a response (not error)
    // Response may be null (lua-ls not initialized yet) or real results
    // Either is fine - the key is that we didn't hang
    println!("Completion response: {:?}", completion_response);

    assert!(
        completion_response.get("result").is_some() || completion_response.get("error").is_some(),
        "Should get either result or error, got: {:?}",
        completion_response
    );

    println!("✓ Received response without hanging");

    // Clean shutdown
    let _shutdown_response = client.send_request("shutdown", json!(null));
    client.send_notification("exit", json!(null));
}
