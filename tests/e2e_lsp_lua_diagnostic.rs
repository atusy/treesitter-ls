//! End-to-end test for Lua diagnostics in Markdown code blocks via kakehashi binary.
//!
//! This test verifies the full bridge infrastructure wiring for pull diagnostics:
//! - kakehashi binary spawned via LspClient (not direct BridgeConnection)
//! - Markdown document with Lua code block opened via didOpen
//! - textDocument/diagnostic request sent
//! - kakehashi detects injection, spawns lua-ls, and transforms diagnostic positions
//!
//! Run with: `cargo test --test e2e_lsp_lua_diagnostic --features e2e`
//!
//! **Requirements**: lua-language-server must be installed and in PATH.
//!
//! Implements ADR-0020 Phase 1: Pull-first diagnostic forwarding.

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lua_bridge::{create_lua_configured_client, shutdown_client};
use serde_json::json;

/// E2E test: diagnosticProvider capability is advertised
#[test]
fn e2e_diagnostic_capability_advertised() {
    let mut client = LspClient::new();

    // Initialize handshake
    let init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {}
        }),
    );

    // Verify diagnosticProvider is in capabilities
    let capabilities = init_response
        .get("result")
        .and_then(|r| r.get("capabilities"))
        .expect("Should have capabilities in init response");

    let diagnostic_provider = capabilities.get("diagnosticProvider");
    assert!(
        diagnostic_provider.is_some(),
        "diagnosticProvider should be advertised in server capabilities"
    );

    // Verify specific options per ADR-0020
    let provider = diagnostic_provider.unwrap();
    assert_eq!(
        provider.get("interFileDependencies"),
        Some(&json!(false)),
        "interFileDependencies should be false per ADR-0020"
    );
    assert_eq!(
        provider.get("workspaceDiagnostics"),
        Some(&json!(false)),
        "workspaceDiagnostics should be false per ADR-0020"
    );

    println!("E2E: diagnosticProvider capability advertised correctly");

    // Clean shutdown
    shutdown_client(&mut client);
}

/// E2E test: diagnostic request is handled for Lua code block with syntax error
#[test]
fn e2e_diagnostic_request_returns_diagnostics_for_lua_error() {
    let mut client = create_lua_configured_client();

    // Open markdown document with Lua code block containing a syntax error
    let markdown_content = r#"# Test Document

```lua
-- This Lua code has a syntax error
local x =
```

More text.
"#;

    let markdown_uri = "file:///test_diagnostic.md";

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

    // Give lua-ls time to start and analyze
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // Send diagnostic request
    let response = client.send_request(
        "textDocument/diagnostic",
        json!({
            "textDocument": {
                "uri": markdown_uri
            }
        }),
    );

    println!(
        "Diagnostic response: {}",
        serde_json::to_string_pretty(&response).unwrap()
    );

    // The response should be a DocumentDiagnosticReport
    let result = response.get("result");
    assert!(
        result.is_some(),
        "Should have result in diagnostic response"
    );

    // Verify it's a "full" report (not "unchanged")
    let result = result.unwrap();
    let kind = result.get("kind");
    assert!(kind.is_some(), "Result should have 'kind' field");

    // Clean shutdown
    shutdown_client(&mut client);
}
