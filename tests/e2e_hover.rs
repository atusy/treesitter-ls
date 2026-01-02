//! End-to-end tests for hover using direct LSP communication with treesitter-ls binary.
//!
//! Migrates hover tests from tests/test_lsp_hover.lua to Rust for faster CI execution
//! and deterministic snapshot testing.
//!
//! Run with: `cargo test --test e2e_hover --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lsp_init::initialize_with_rust_bridge;
use helpers::lsp_polling::poll_until;
use helpers::sanitization::sanitize_hover_response;
use helpers::test_fixtures::create_hover_fixture;
use serde_json::{Value, json};

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
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_hover_fixture();
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
    let contents_str = extract_hover_contents_as_string(&hover);

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

/// Test that hover response is deterministic and can be snapshot tested.
///
/// This test verifies:
/// - Hover content is stable across runs
/// - Sanitization removes non-deterministic data (temp paths)
/// - Snapshot captures expected response structure
#[test]
fn test_hover_snapshot() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_hover_fixture();
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

        let result = response.get("result").cloned().unwrap_or(Value::Null);
        if result.is_null() {
            return None;
        }

        let sanitized = sanitize_hover_response(&result);
        if hover_contains_indexing_message(&sanitized) {
            None
        } else {
            Some(result)
        }
    });

    assert!(
        hover_result.is_some(),
        "Expected hover result for snapshot testing"
    );

    let hover = hover_result.unwrap();

    // Sanitize the hover response for deterministic snapshot
    let sanitized = sanitize_hover_response(&hover);

    // Capture snapshot
    insta::assert_json_snapshot!("hover_response", sanitized);
}

/// Extract hover contents as a single normalized string.
///
/// Handles multiple content format types:
/// - `Value::String`: Direct string content
/// - `Value::Object`: MarkupContent or MarkedString with "value" field
/// - `Value::Array`: Array of MarkedString values
fn extract_hover_contents_as_string(hover: &Value) -> String {
    match hover.get("contents") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Object(obj)) => {
            // MarkedString or MarkupContent
            if let Some(value) = obj.get("value") {
                value.as_str().unwrap_or("").to_string()
            } else {
                format!("{:?}", obj)
            }
        }
        Some(Value::Array(arr)) => {
            // Array of MarkedString
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => format!("{:?}", hover.get("contents")),
    }
}

fn hover_contains_indexing_message(hover: &Value) -> bool {
    match hover.get("contents") {
        Some(Value::String(value)) => value.contains("indexing"),
        Some(Value::Object(obj)) => obj
            .get("value")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("indexing"))
            .unwrap_or(false),
        Some(Value::Array(values)) => values
            .iter()
            .any(|v| v.as_str().map(|s| s.contains("indexing")).unwrap_or(false)),
        _ => false,
    }
}
