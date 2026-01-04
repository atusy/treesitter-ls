//! End-to-end test for fakeit bridge infrastructure.
//!
//! **DEPRECATED**: This test is kept for historical reference but is SUPERSEDED by
//! tests/e2e_lsp_lua_completion.rs which uses the correct E2E pattern.
//!
//! **Why deprecated**:
//! - This test verified the fakeit phase (PBI-178) which returned Ok(None)
//! - Bridge infrastructure is now wired to real language servers (PBI-184)
//! - New E2E tests should use LspClient to spawn treesitter-ls binary
//! - See tests/e2e_lsp_lua_completion.rs for the correct pattern
//!
//! This test verifies that the fakeit bridge implementation works end-to-end:
//! - LSP server starts and initializes
//! - Completion request sent to Lua code block in markdown
//! - Response received (Ok(None) from fakeit implementation)
//! - No hanging or timeouts
//!
//! Run with: `cargo test --test e2e_bridge_fakeit --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use serde_json::json;
use std::time::{Duration, Instant};

#[test]
fn test_fakeit_bridge_completion_returns_quickly() {
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

    // Create markdown document with Lua code block
    let doc_uri = "file:///test.md";
    let markdown_content = r#"# Test Document

Here is some Lua code:

```lua
local function greet(name)
    print("Hello, " .. name)
end
```

End of document.
"#;

    // Send didOpen notification
    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": doc_uri,
                "languageId": "markdown",
                "version": 1,
                "text": markdown_content
            }
        }),
    );

    // Send completion request inside the Lua block
    // Position is inside "local function greet"
    let start = Instant::now();
    let completion_response = client.send_request(
        "textDocument/completion",
        json!({
            "textDocument": {
                "uri": doc_uri
            },
            "position": {
                "line": 5,  // Inside the Lua code block
                "character": 10
            }
        }),
    );
    let elapsed = start.elapsed();

    // Verify response structure (should be Ok with null result from fakeit)
    assert!(
        completion_response.get("result").is_some(),
        "Completion response should have result field: {:?}",
        completion_response
    );

    // In fakeit implementation, result should be null (Ok(None))
    let result = completion_response.get("result").unwrap();
    assert!(
        result.is_null(),
        "Fakeit completion should return null (Ok(None)): {:?}",
        result
    );

    // Verify request completed within reasonable time (no hanging)
    assert!(
        elapsed < Duration::from_secs(5),
        "Completion request took {:?}, expected < 5s (fakeit should be instant)",
        elapsed
    );
}

#[test]
fn test_fakeit_bridge_hover_returns_quickly() {
    let mut client = LspClient::new();

    // Initialize
    let _init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {}
        }),
    );
    client.send_notification("initialized", json!({}));

    // Create markdown with Rust code block
    let doc_uri = "file:///test_rust.md";
    let markdown_content = r#"# Rust Example

```rust
fn main() {
    println!("Hello");
}
```
"#;

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": doc_uri,
                "languageId": "markdown",
                "version": 1,
                "text": markdown_content
            }
        }),
    );

    // Send hover request inside Rust block
    let start = Instant::now();
    let hover_response = client.send_request(
        "textDocument/hover",
        json!({
            "textDocument": {
                "uri": doc_uri
            },
            "position": {
                "line": 3,
                "character": 5
            }
        }),
    );
    let elapsed = start.elapsed();

    // Verify response (fakeit returns null)
    assert!(
        hover_response.get("result").is_some(),
        "Hover response should have result field: {:?}",
        hover_response
    );

    let result = hover_response.get("result").unwrap();
    assert!(
        result.is_null(),
        "Fakeit hover should return null: {:?}",
        result
    );

    // Verify no hanging
    assert!(
        elapsed < Duration::from_secs(5),
        "Hover request took {:?}, expected < 5s",
        elapsed
    );
}
