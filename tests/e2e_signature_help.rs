//! End-to-end tests for signature help using direct LSP communication with treesitter-ls binary.
//!
//! These tests verify that signature help requests work correctly through the async bridge,
//! providing function parameter information at the cursor position.
//!
//! Run with: `cargo test --test e2e_signature_help --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lsp_init::initialize_with_rust_bridge;
use helpers::lsp_polling::poll_until;
use helpers::sanitization::sanitize_signature_help_response;
use helpers::test_fixtures::create_signature_help_fixture;
use serde_json::{Value, json};

/// Test that signature help returns parameter information for a function call.
///
/// This test verifies:
/// - Signature help works through the async bridge
/// - Parameter information is returned at the correct position
/// - The response includes function signature details
#[test]
fn test_signature_help_returns_signatures() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_signature_help_fixture();
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

    // Request signature help at position inside "greet(" call
    // The fixture has: "greet(" on line 8 (0-indexed)
    // Position after the opening paren: line 8, column 10
    let signature_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/signatureHelp",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 8,
                    "character": 10
                }
            }),
        );

        // Verify we got a response
        assert!(
            response.get("result").is_some(),
            "Signature help response should have result: {:?}",
            response
        );

        let result = response.get("result").unwrap().clone();

        // Result can be SignatureHelp object or null
        if !result.is_null() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        signature_result.is_some(),
        "Signature help should return content for valid function call position after retries"
    );

    let signature_help = signature_result.unwrap();

    // Verify signature help has signatures field
    assert!(
        signature_help.get("signatures").is_some(),
        "SignatureHelp result should have signatures: {:?}",
        signature_help
    );

    let signatures = signature_help.get("signatures").unwrap();
    assert!(
        signatures.is_array(),
        "Signatures should be an array: {:?}",
        signatures
    );

    let signatures_array = signatures.as_array().unwrap();
    assert!(
        !signatures_array.is_empty(),
        "Signatures array should not be empty"
    );

    // Verify first signature has expected fields
    let first_sig = &signatures_array[0];
    assert!(
        first_sig.get("label").is_some(),
        "Signature should have label: {:?}",
        first_sig
    );

    let label = first_sig.get("label").unwrap().as_str().unwrap();
    assert!(
        label.contains("greet") && label.contains("name") && label.contains("age"),
        "Signature label should contain function name and parameter names, got: {}",
        label
    );
}

/// Test that signature help response is deterministic and can be snapshot tested.
///
/// This test verifies:
/// - Signature help content is stable across runs
/// - Sanitization removes non-deterministic data (temp paths)
/// - Snapshot captures expected response structure
#[test]
fn test_signature_help_snapshot() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_signature_help_fixture();
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

    // Request signature help at position inside "greet(" call
    // Retry to wait for rust-analyzer indexing
    let signature_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/signatureHelp",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 8,
                    "character": 10
                }
            }),
        );

        let result = response.get("result").cloned().unwrap_or(Value::Null);
        if result.is_null() {
            return None;
        }

        // Check that we have at least one signature
        if let Some(signatures) = result.get("signatures") {
            if let Some(arr) = signatures.as_array() {
                if !arr.is_empty() {
                    return Some(result);
                }
            }
        }
        None
    });

    assert!(
        signature_result.is_some(),
        "Expected signature help result for snapshot testing"
    );

    let signature_help = signature_result.unwrap();

    // Sanitize the signature help response for deterministic snapshot
    let sanitized = sanitize_signature_help_response(&signature_help);

    // Capture snapshot
    insta::assert_json_snapshot!("signature_help_response", sanitized);
}

/// Test that signature help returns None for positions outside function calls.
///
/// This test verifies:
/// - Signature help correctly returns None when not in a function call context
/// - The bridge doesn't crash or error on invalid positions
#[test]
fn test_signature_help_returns_none_outside_function_call() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_signature_help_fixture();
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

    // Give rust-analyzer time to index
    std::thread::sleep(std::time::Duration::from_millis(2000));

    // Request signature help at a position NOT in a function call
    // Line 0 is the markdown header "# Signature Help Example"
    let response = client.send_request(
        "textDocument/signatureHelp",
        json!({
            "textDocument": {
                "uri": uri
            },
            "position": {
                "line": 0,
                "character": 0
            }
        }),
    );

    // Verify we got a response
    assert!(
        response.get("result").is_some(),
        "Signature help response should have result field: {:?}",
        response
    );

    let result = response.get("result").unwrap();

    // Result should be null or have empty signatures for positions outside function calls
    if !result.is_null() {
        if let Some(signatures) = result.get("signatures") {
            if let Some(arr) = signatures.as_array() {
                assert!(
                    arr.is_empty(),
                    "Signatures should be empty outside function calls, got: {:?}",
                    arr
                );
            }
        }
    }
}
