//! End-to-end test for incremental sync (TextDocumentSyncKind::Incremental).
//!
//! This test verifies that when didChange uses incremental sync (with range),
//! the apply_edits path (Phase 4) correctly processes the edit and updates
//! region tracking without over-invalidating ULIDs.
//!
//! Run with: `cargo test --test e2e_incremental_sync --features e2e`
//!
//! **Requirements**: lua-language-server must be installed and in PATH.

#![cfg(feature = "e2e")]

mod helpers;

use helpers::lsp_polling::poll_for_hover;
use helpers::lua_bridge::{
    create_lua_configured_client, shutdown_client, skip_if_lua_ls_unavailable,
};
use serde_json::json;

/// E2E test: incremental sync (range-based didChange) works correctly
///
/// This test verifies Phase 4's apply_edits path:
/// 1. Open a markdown document with a Lua block
/// 2. Trigger hover to establish virtual document
/// 3. Send incremental didChange (with range, not full text)
/// 4. Verify hover still works (ULID preserved, region tracking correct)
///
/// The key difference from e2e_didchange_forwarding is using incremental sync
/// with `range` instead of full document replacement.
#[test]
fn e2e_incremental_sync_preserves_region_tracking() {
    if skip_if_lua_ls_unavailable() {
        return;
    }

    let mut client = create_lua_configured_client();

    // Phase 1: Open markdown document with Lua code block
    let markdown_uri = "file:///test_incremental_sync.md";
    // Line numbers (0-indexed):
    // 0: "# Test Document"
    // 1: ""
    // 2: "```lua"
    // 3: "local foo = 1"
    // 4: "print(foo)"
    // 5: "```"
    // 6: ""
    // 7: "More text."
    let initial_content = r#"# Test Document

```lua
local foo = 1
print(foo)
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
                "text": initial_content
            }
        }),
    );

    // Give lua-ls time to initialize
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Phase 2: Trigger hover on "foo" to establish virtual document
    // Line 3: "local foo = 1", character 6 is on "foo"
    // Use more retries (10) to allow lua-ls time to index
    let hover_before = poll_for_hover(&mut client, markdown_uri, 3, 6, 10, 500);

    // STRICT: Fail if hover doesn't work - this indicates lua-ls integration issue
    assert!(
        hover_before.is_some(),
        "Initial hover should succeed after 10 retries. \
         This indicates lua-ls may not be ready or there's an integration issue."
    );

    println!("Phase 2: Hover before incremental change succeeded");

    // Phase 3: Send incremental didChange
    // We'll insert " = 2" at the end of line 3, changing "local foo = 1" to "local foo = 1 = 2"
    // (This is syntactically invalid Lua, but that's okay for testing region tracking)
    //
    // Line 3 content: "local foo = 1" (13 characters, at positions 0-12)
    // Insert at end of line 3: position {line: 3, character: 13}
    // Range: {start: {line: 3, character: 13}, end: {line: 3, character: 13}}
    // Text: " = 2"
    //
    // This is an INCREMENTAL change (has range), not FULL sync
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": markdown_uri,
                "version": 2
            },
            "contentChanges": [
                {
                    "range": {
                        "start": { "line": 3, "character": 13 },
                        "end": { "line": 3, "character": 13 }
                    },
                    "text": " = 2"
                }
            ]
        }),
    );

    println!("Phase 3: Sent incremental didChange (insert at end of line 3)");
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Phase 4: Verify hover still works on the original variable
    // This confirms:
    // - Region tracking correctly processed the incremental edit
    // - ULID was preserved (not over-invalidated)
    // - Virtual document URI remains stable
    let hover_after = poll_for_hover(&mut client, markdown_uri, 3, 6, 10, 500);

    // STRICT: Fail if hover doesn't work after incremental edit
    assert!(
        hover_after.is_some(),
        "Hover after incremental edit should succeed. \
         This indicates region tracking may have incorrectly invalidated the ULID."
    );

    println!("Phase 4: Hover after incremental change succeeded");

    // Verify hover didn't return an error
    assert!(
        hover_after.as_ref().unwrap().get("error").is_none(),
        "Hover after incremental sync should not error: {:?}",
        hover_after.as_ref().unwrap().get("error")
    );

    println!("E2E: Incremental sync preserves region tracking - hover works after edit!");

    // Clean shutdown
    shutdown_client(&mut client);
}

/// E2E test: multiple incremental edits maintain correct region positions
///
/// This test verifies that multiple sequential incremental edits correctly
/// update region positions using the running coordinates approach.
#[test]
fn e2e_multiple_incremental_edits_maintain_positions() {
    if skip_if_lua_ls_unavailable() {
        return;
    }

    let mut client = create_lua_configured_client();

    // Open markdown with two Lua blocks
    let markdown_uri = "file:///test_multi_incremental.md";
    // Line numbers (0-indexed):
    // 0: "# Test"
    // 1: ""
    // 2: "```lua"
    // 3: "local a = 1"
    // 4: "```"
    // 5: ""
    // 6: "Middle text."
    // 7: ""
    // 8: "```lua"
    // 9: "local b = 2"
    // 10: "```"
    let initial_content = r#"# Test

```lua
local a = 1
```

Middle text.

```lua
local b = 2
```
"#;

    client.send_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": markdown_uri,
                "languageId": "markdown",
                "version": 1,
                "text": initial_content
            }
        }),
    );

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Hover on first Lua block to establish virtual document (10 retries)
    let hover1 = poll_for_hover(&mut client, markdown_uri, 3, 6, 10, 500);
    assert!(
        hover1.is_some(),
        "First Lua block hover should succeed after 10 retries"
    );
    println!("Established first Lua block virtual document");

    // Hover on second Lua block to establish its virtual document (10 retries)
    let hover2 = poll_for_hover(&mut client, markdown_uri, 9, 6, 10, 500);
    assert!(
        hover2.is_some(),
        "Second Lua block hover should succeed after 10 retries"
    );
    println!("Established second Lua block virtual document");

    // Send multiple incremental edits in sequence
    // Edit 1: Insert newline after "Middle text." (shifts second Lua block down)
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": markdown_uri,
                "version": 2
            },
            "contentChanges": [
                {
                    "range": {
                        "start": { "line": 6, "character": 12 },
                        "end": { "line": 6, "character": 12 }
                    },
                    "text": "\nExtra line."
                }
            ]
        }),
    );
    println!("Sent edit 1: Insert newline after 'Middle text.'");
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Edit 2: Insert character in first Lua block
    client.send_notification(
        "textDocument/didChange",
        json!({
            "textDocument": {
                "uri": markdown_uri,
                "version": 3
            },
            "contentChanges": [
                {
                    "range": {
                        "start": { "line": 3, "character": 11 },
                        "end": { "line": 3, "character": 11 }
                    },
                    "text": "0"
                }
            ]
        }),
    );
    println!("Sent edit 2: Insert '0' making 'local a = 10'");
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Verify first Lua block still works (after both edits)
    let hover1_after = poll_for_hover(&mut client, markdown_uri, 3, 6, 10, 500);
    assert!(
        hover1_after.is_some(),
        "First Lua block hover should still work after multiple incremental edits"
    );
    println!("First Lua block hover works after multiple incremental edits");

    // Verify second Lua block still works (now shifted to line 10)
    let hover2_after = poll_for_hover(&mut client, markdown_uri, 10, 6, 10, 500);
    assert!(
        hover2_after.is_some(),
        "Second Lua block hover should work at new position after line shift"
    );
    println!("Second Lua block hover works after position shift");

    // All assertions passed
    println!("E2E: Multiple incremental edits maintain correct region positions");

    shutdown_client(&mut client);
}
