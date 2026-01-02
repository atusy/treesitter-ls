//! End-to-end tests for find references using direct LSP communication with treesitter-ls binary.
//!
//! Migrates references tests from tests/test_lsp_references.lua to Rust for faster CI execution
//! and deterministic snapshot testing.
//!
//! Run with: `cargo test --test e2e_references --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lsp_init::initialize_with_rust_bridge;
use helpers::lsp_polling::poll_until;
use helpers::sanitization::sanitize_references_response;
use helpers::test_fixtures::create_references_fixture;
use serde_json::json;

/// Test that references returns locations for all uses of variable 'x'.
///
/// Migrates from tests/test_lsp_references.lua:
/// - Cursor on 'x' definition at line 5 (0-indexed: line 4, column 8)
/// - Expects 3 references: definition at line 4, uses at lines 5 and 6
/// - Verifies all reference lines are >= 3 (host coordinates, after ``` marker)
///
/// This test verifies the async bridge path works for references requests and that
/// coordinate translation from virtual to host document is correct.
#[test]
fn test_references_returns_locations() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_references_fixture();
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

    // Request references for 'x' on line 4, column 8 (0-indexed, the 'x' in "let x = 42")
    // In the Lua test: line 5 (1-indexed)
    // Retry to wait for rust-analyzer indexing
    let references_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/references",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 4,
                    "character": 8
                },
                "context": {
                    "includeDeclaration": true
                }
            }),
        );

        // Verify we got a response
        assert!(
            response.get("result").is_some(),
            "References response should have result: {:?}",
            response
        );

        let result = response.get("result").unwrap().clone();

        // Result should be array of Location or null
        if result.is_array() && !result.as_array().unwrap().is_empty() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        references_result.is_some(),
        "References should return locations for valid position after retries"
    );

    let references = references_result.unwrap();
    let locations = references.as_array().expect("references should be array");

    // Verify we got at least 3 references (definition + 2 uses)
    // rust-analyzer might return more (e.g., if it finds related references)
    assert!(
        locations.len() >= 3,
        "Should find at least 3 references to 'x', got {}",
        locations.len()
    );

    // Verify all reference locations use host document coordinates
    // The references should be at lines 4, 5, 6 (0-indexed) in the Markdown file
    // NOT at lines 0, 1, 2 (virtual document)
    for location in locations {
        let range = location.get("range").expect("Location should have range");
        let start_line = range["start"]["line"]
            .as_u64()
            .expect("range.start.line should be number");

        assert!(
            start_line >= 3,
            "Reference line {} should be in host coordinates (>= 3, after ``` marker)",
            start_line
        );
    }

    // Collect line numbers for debugging
    let lines: Vec<u64> = locations
        .iter()
        .map(|loc| loc["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    eprintln!("Found references at lines: {:?}", lines);
}

/// Test that references response is deterministic and can be snapshot tested.
///
/// This test verifies:
/// - Reference locations are stable across runs
/// - Sanitization removes non-deterministic data (URIs, temp paths)
/// - Snapshot captures expected response structure
/// - Reference coordinates are in host document (lines >= 3)
#[test]
fn test_references_snapshot() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_references_fixture();
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

    // Request references for 'x' on line 4, column 8 (0-indexed)
    // Retry to wait for rust-analyzer indexing
    let references_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/references",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 4,
                    "character": 8
                },
                "context": {
                    "includeDeclaration": true
                }
            }),
        );

        let result = response.get("result").cloned().unwrap_or(json!(null));
        if result.is_array() && !result.as_array().unwrap().is_empty() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        references_result.is_some(),
        "Expected references result for snapshot testing"
    );

    let references = references_result.unwrap();

    // Sanitize the references response for deterministic snapshot
    let sanitized = sanitize_references_response(&references);

    // Capture snapshot
    insta::assert_json_snapshot!("references_response", sanitized);
}
