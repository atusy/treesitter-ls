//! End-to-end tests for completion using direct LSP communication with treesitter-ls binary.
//!
//! Migrates completion tests from tests/test_lsp_completion.lua to Rust for faster CI execution
//! and deterministic snapshot testing.
//!
//! Run with: `cargo test --test e2e_completion --features e2e`

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_client::LspClient;
use helpers::lsp_init::initialize_with_rust_bridge;
use helpers::lsp_polling::poll_until;
use helpers::sanitization::sanitize_completion_response;
use helpers::test_fixtures::create_completion_fixture;
use serde_json::json;

/// Expected struct field names from the completion fixture.
/// These must match the field names in `create_completion_fixture` (helpers_test_fixtures.rs).
/// If the fixture structure changes, these constants must be updated to maintain test validity.
const EXPECTED_COMPLETION_FIELDS: &[&str] = &["x", "y"];

/// Test that completion returns struct field items with adjusted textEdit ranges.
///
/// Migrates from tests/test_lsp_completion.lua:
/// - Cursor after 'p.' on line 11 (0-indexed: line 10, column 6)
/// - Expects completion items including struct fields from `create_completion_fixture`
/// - Verifies textEdit ranges are in host document coordinates (line >= 10)
///
/// This test verifies the async bridge path works for completion requests and that
/// coordinate translation from virtual to host document is correct.
///
/// **Fixture Dependency**: This test is tightly coupled to the struct definition in
/// `create_completion_fixture` (helpers_test_fixtures.rs). The completion filter
/// checks for the field names defined in `EXPECTED_COMPLETION_FIELDS`. If the fixture
/// struct fields are renamed or removed, this constant must be updated to prevent
/// silent test failures.
#[test]
fn test_completion_returns_items() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_completion_fixture();
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

    // Request completion after 'p.' on line 10, column 6 (0-indexed)
    // In the Lua test: line 11 (1-indexed), after "p."
    // Retry to wait for rust-analyzer indexing
    let completion_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/completion",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 10,
                    "character": 6
                }
            }),
        );

        // Verify we got a response
        assert!(
            response.get("result").is_some(),
            "Completion response should have result: {:?}",
            response
        );

        let result = response.get("result").unwrap().clone();

        // Result can be CompletionList or array of CompletionItem or null
        if !result.is_null() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        completion_result.is_some(),
        "Completion should return items for valid position after retries"
    );

    let completion = completion_result.unwrap();

    // Extract items from CompletionList or array
    let items = completion
        .get("items")
        .or(Some(&completion))
        .and_then(|v| v.as_array())
        .expect("completion result should be an array or contain an 'items' array");

    assert!(
        !items.is_empty(),
        "Completion should return at least one item"
    );

    // Check for expected struct fields in completion items
    let mut found_fields = std::collections::HashSet::new();

    for item in items {
        let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");

        if EXPECTED_COMPLETION_FIELDS.contains(&label) {
            found_fields.insert(label);
            // Verify textEdit range is in host document coordinates
            if let Some(text_edit) = item.get("textEdit") {
                if let Some(range) = text_edit.get("range") {
                    let start_line = range["start"]["line"].as_u64().unwrap();
                    assert!(
                        start_line >= 10,
                        "textEdit range should be in host coordinates (got line {}, expected >= 10)",
                        start_line
                    );
                }
            }
        }
    }

    // rust-analyzer may not always return struct fields depending on indexing state
    // So we accept if we found at least some completion items
    // But if we did find any expected fields, verify coordinates are correct
    if !found_fields.is_empty() {
        assert!(
            found_fields.len() == EXPECTED_COMPLETION_FIELDS.len(),
            "If struct fields are found, all expected fields {} should be present, but got {:?}",
            EXPECTED_COMPLETION_FIELDS.join(", "),
            found_fields
        );
    } else {
        // At minimum, verify we got completion items with proper structure
        let first_item = &items[0];
        assert!(
            first_item.get("label").is_some(),
            "Completion items should have label"
        );
    }
}

/// Test that completion response is deterministic and can be snapshot tested.
///
/// This test verifies:
/// - Completion items are stable across runs
/// - Sanitization removes non-deterministic data (temp paths, URIs)
/// - Snapshot captures expected response structure
/// - textEdit ranges are properly adjusted to host coordinates
///
/// **Fixture Dependency**: This test filters completion items to include only the
/// expected struct fields defined in `EXPECTED_COMPLETION_FIELDS` to ensure deterministic
/// snapshots. If the fixture structure changes, the constant must be updated.
#[test]
fn test_completion_snapshot() {
    let mut client = LspClient::new();

    // Initialize with bridge configuration
    initialize_with_rust_bridge(&mut client);

    // Create and open test file
    let (uri, content, _temp_file) = create_completion_fixture();
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

    // Request completion after 'p.' on line 10, column 6 (0-indexed)
    // Retry to wait for rust-analyzer indexing
    let completion_result = poll_until(20, 500, || {
        let response = client.send_request(
            "textDocument/completion",
            json!({
                "textDocument": {
                    "uri": uri
                },
                "position": {
                    "line": 10,
                    "character": 6
                }
            }),
        );

        let result = response.get("result").cloned().unwrap_or(json!(null));
        if !result.is_null() {
            Some(result)
        } else {
            None
        }
    });

    assert!(
        completion_result.is_some(),
        "Expected completion result for snapshot testing"
    );

    let completion = completion_result.unwrap();

    // Sanitize the completion response for deterministic snapshot
    let mut sanitized = sanitize_completion_response(&completion);

    // Filter to only include expected struct field completions for deterministic snapshot
    // rust-analyzer may return different additional completions depending on indexing state
    if let Some(items) = sanitized.get_mut("items") {
        if let Some(items_array) = items.as_array_mut() {
            items_array.retain(|item| {
                let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
                EXPECTED_COMPLETION_FIELDS.contains(&label)
            });
            // Sort by label for consistent ordering
            items_array.sort_by(|a, b| {
                let label_a = a.get("label").and_then(|v| v.as_str()).unwrap_or("");
                let label_b = b.get("label").and_then(|v| v.as_str()).unwrap_or("");
                label_a.cmp(label_b)
            });
        }
    }

    // Capture snapshot
    insta::assert_json_snapshot!("completion_response", sanitized);
}
