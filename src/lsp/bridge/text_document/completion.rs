//! Completion request handling for bridge connections.
//!
//! This module provides completion request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` and `send_notification()` to queue messages via
//! the channel-based writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{CompletionItem, CompletionList, Position, Range};
use url::Url;

use super::super::pool::{
    ConnectionHandleSender, LanguageServerPool, NotificationSendResult, UpstreamId,
};
use super::super::protocol::{
    RequestId, VirtualDocumentUri, build_bridge_didchange_notification,
    build_position_based_request,
};

impl LanguageServerPool {
    /// Send a completion request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if not opened, or didChange if already opened
    /// 3. Register request with router to get oneshot receiver
    /// 4. Send the completion request (release writer lock after)
    /// 5. Wait for response via oneshot channel (no Mutex held)
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_completion_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        upstream_request_id: UpstreamId,
    ) -> io::Result<Option<CompletionList>> {
        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        // Convert host_uri to lsp_types::Uri for bridge protocol functions
        let host_uri_lsp = crate::lsp::lsp_impl::url_to_uri(host_uri)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(&host_uri_lsp, injection_language, region_id);

        // Register in the upstream request registry FIRST for cancel lookup.
        // This order matters: if a cancel arrives between pool and router registration,
        // the cancel will fail at the router lookup (which is acceptable for best-effort
        // cancel semantics) rather than finding the server but no downstream ID.
        self.register_upstream_request(upstream_request_id.clone(), server_name);

        // Register request with upstream ID mapping for cancel forwarding
        let (request_id, response_rx) =
            match handle.register_request_with_upstream(Some(upstream_request_id.clone())) {
                Ok(result) => result,
                Err(e) => {
                    // Clean up the pool registration on failure
                    self.unregister_upstream_request(&upstream_request_id, server_name);
                    return Err(e);
                }
            };

        // Build completion request
        let completion_request = build_completion_request(
            &host_uri_lsp,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );

        // Use a closure for cleanup on any failure path
        let cleanup = || {
            handle.router().remove(request_id);
            self.unregister_upstream_request(&upstream_request_id, server_name);
        };

        // Track if we need to send didChange (when document was already opened)
        let was_already_opened = self.is_document_opened(&virtual_uri);

        // Send didOpen notification only if document hasn't been opened yet
        if let Err(e) = self
            .ensure_document_opened(
                &mut ConnectionHandleSender(&handle),
                host_uri,
                &virtual_uri,
                virtual_content,
                server_name,
            )
            .await
        {
            cleanup();
            return Err(e);
        }

        // Document already opened: send didChange with incremented version
        if was_already_opened
            && let Some(version) = self
                .increment_document_version(&virtual_uri, server_name)
                .await
        {
            let did_change = build_bridge_didchange_notification(
                &host_uri_lsp,
                injection_language,
                region_id,
                virtual_content,
                version,
            );
            // Queue didChange notification via single-writer loop (ADR-0015)
            match handle.send_notification(did_change) {
                NotificationSendResult::Queued => {}
                NotificationSendResult::QueueFull => {
                    cleanup();
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "bridge: didChange notification queue full",
                    ));
                }
                NotificationSendResult::ChannelClosed => {
                    cleanup();
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "bridge: didChange notification channel closed",
                    ));
                }
            }
        }

        // Queue the completion request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(completion_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates
        Ok(transform_completion_response_to_host(
            response?,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC completion request for a downstream language server.
fn build_completion_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_position: tower_lsp_server::ls_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: RequestId,
) -> serde_json::Value {
    build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/completion",
    )
}

/// Parse a JSON-RPC completion response and transform coordinates to host document space.
///
/// Normalizes all responses to `CompletionList` format. If the server returns an array,
/// it's wrapped as `CompletionList { isIncomplete: false, items }`.
///
/// Returns `None` for: null results, missing results, and deserialization failures.
///
/// # Arguments
/// * `response` - Raw JSON-RPC response envelope (`{"result": {...}}`)
/// * `region_start_line` - Line offset to add to completion item ranges
fn transform_completion_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> Option<CompletionList> {
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
    let result = response.get_mut("result").map(serde_json::Value::take)?;
    if result.is_null() {
        return None;
    }

    // Determine format and deserialize into a unified CompletionList
    let mut list = if result.is_array() {
        // Legacy format: array of CompletionItem. Normalize to CompletionList.
        let Ok(items) = serde_json::from_value::<Vec<CompletionItem>>(result) else {
            return None;
        };
        CompletionList {
            is_incomplete: false,
            items,
        }
    } else {
        // Preferred format: CompletionList object
        let Ok(list) = serde_json::from_value::<CompletionList>(result) else {
            return None;
        };
        list
    };

    // Transform all items in the list
    for item in &mut list.items {
        transform_completion_item(item, region_start_line);
    }

    Some(list)
}

/// Transform textEdit range in a single completion item to host coordinates.
///
/// Handles both TextEdit format and InsertReplaceEdit format. Also transforms
/// additionalTextEdits if present.
fn transform_completion_item(item: &mut CompletionItem, region_start_line: u32) {
    // Transform text_edit if present
    if let Some(ref mut text_edit) = item.text_edit {
        match text_edit {
            tower_lsp_server::ls_types::CompletionTextEdit::Edit(edit) => {
                transform_range(&mut edit.range, region_start_line);
            }
            tower_lsp_server::ls_types::CompletionTextEdit::InsertAndReplace(edit) => {
                transform_range(&mut edit.insert, region_start_line);
                transform_range(&mut edit.replace, region_start_line);
            }
        }
    }

    // Transform additional_text_edits if present
    if let Some(ref mut additional_edits) = item.additional_text_edits {
        for edit in additional_edits {
            transform_range(&mut edit.range, region_start_line);
        }
    }
}

/// Transform a range's line numbers from virtual to host coordinates.
///
/// Uses saturating_add to prevent overflow for large line numbers, consistent
/// with saturating_sub used elsewhere in the codebase.
fn transform_range(range: &mut Range, region_start_line: u32) {
    // Transform start and end positions
    range.start.line = range.start.line.saturating_add(region_start_line);
    range.end.line = range.end.line.saturating_add(region_start_line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tower_lsp_server::ls_types::Position;
    use url::Url;

    // ==========================================================================
    // Test helpers
    // ==========================================================================

    /// Standard test request ID used across most tests.
    fn test_request_id() -> RequestId {
        RequestId::new(42)
    }

    /// Standard test host URI used across most tests.
    fn test_host_uri() -> tower_lsp_server::ls_types::Uri {
        let url = Url::parse("file:///project/doc.md").unwrap();
        crate::lsp::lsp_impl::url_to_uri(&url).expect("test URL should convert to URI")
    }

    /// Standard test position (line 5, character 10).
    fn test_position() -> Position {
        Position {
            line: 5,
            character: 10,
        }
    }

    /// Assert that a request uses a virtual URI with the expected extension.
    fn assert_uses_virtual_uri(request: &serde_json::Value, extension: &str) {
        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        // Use url crate for robust parsing (handles query strings with slashes, fragments, etc.)
        let url = url::Url::parse(uri_str).expect("URI should be parseable");
        let filename = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("");
        assert!(
            filename.starts_with("kakehashi-virtual-uri-")
                && filename.ends_with(&format!(".{}", extension)),
            "Request should use virtual URI with .{} extension: {}",
            extension,
            uri_str
        );
    }

    /// Assert that a position-based request has correct structure and translated coordinates.
    fn assert_position_request(
        request: &serde_json::Value,
        expected_method: &str,
        expected_virtual_line: u64,
    ) {
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], expected_method);
        assert_eq!(
            request["params"]["position"]["line"], expected_virtual_line,
            "Position line should be translated"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // Completion request tests
    // ==========================================================================

    #[test]
    fn completion_request_uses_virtual_uri() {
        let request = build_completion_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            test_request_id(),
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn completion_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_completion_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            test_request_id(),
        );

        assert_position_request(&request, "textDocument/completion", 2);
    }

    // ==========================================================================
    // Completion response transformation tests
    // ==========================================================================

    #[test]
    fn completion_response_transforms_textedit_ranges() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [
                    {
                        "label": "print",
                        "kind": 3,
                        "textEdit": {
                            "range": {
                                "start": { "line": 1, "character": 0 },
                                "end": { "line": 1, "character": 3 }
                            },
                            "newText": "print"
                        }
                    },
                    { "label": "pairs", "kind": 3 }
                ]
            }
        });
        let region_start_line = 3;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let list = transformed.unwrap();
        assert_eq!(list.items.len(), 2);

        // First item should have transformed range
        let item = &list.items[0];
        assert_eq!(item.label, "print");
        if let Some(tower_lsp_server::ls_types::CompletionTextEdit::Edit(ref edit)) = item.text_edit
        {
            assert_eq!(edit.range.start.line, 4); // 1 + 3 = 4
            assert_eq!(edit.range.end.line, 4);
        } else {
            panic!("Expected TextEdit");
        }

        // Second item has no textEdit
        assert_eq!(list.items[1].label, "pairs");
        assert!(list.items[1].text_edit.is_none());
    }

    #[test]
    fn completion_response_with_null_result_returns_none() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_completion_response_to_host(response, 3);
        assert!(transformed.is_none());
    }

    #[test]
    fn completion_response_handles_array_format() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "label": "print",
                "textEdit": {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "print"
                }
            }]
        });
        let region_start_line = 5;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let list = transformed.unwrap();
        // Array format is normalized to CompletionList with isIncomplete=false
        assert!(!list.is_incomplete);
        assert_eq!(list.items.len(), 1);

        if let Some(tower_lsp_server::ls_types::CompletionTextEdit::Edit(ref edit)) =
            list.items[0].text_edit
        {
            assert_eq!(edit.range.start.line, 5); // 0 + 5 = 5
            assert_eq!(edit.range.end.line, 5);
        } else {
            panic!("Expected TextEdit");
        }
    }

    #[test]
    fn completion_response_transforms_insert_replace_edit() {
        // InsertReplaceEdit format used by rust-analyzer, tsserver, etc.
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [{
                    "label": "println!",
                    "textEdit": {
                        "insert": {
                            "start": { "line": 2, "character": 0 },
                            "end": { "line": 2, "character": 3 }
                        },
                        "replace": {
                            "start": { "line": 2, "character": 0 },
                            "end": { "line": 2, "character": 8 }
                        },
                        "newText": "println!"
                    }
                }]
            }
        });
        let region_start_line = 10;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let list = transformed.unwrap();

        let item = &list.items[0];
        if let Some(tower_lsp_server::ls_types::CompletionTextEdit::InsertAndReplace(ref edit)) =
            item.text_edit
        {
            // Insert range transformed: line 2 + 10 = 12
            assert_eq!(edit.insert.start.line, 12);
            assert_eq!(edit.insert.end.line, 12);
            // Replace range transformed: line 2 + 10 = 12
            assert_eq!(edit.replace.start.line, 12);
            assert_eq!(edit.replace.end.line, 12);
        } else {
            panic!("Expected InsertReplaceEdit");
        }
    }

    #[test]
    fn completion_response_without_result_returns_none() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42
        });

        let transformed = transform_completion_response_to_host(response, 3);
        assert!(transformed.is_none());
    }

    #[test]
    fn completion_response_transforms_additional_text_edits() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [{
                    "label": "import",
                    "additionalTextEdits": [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 0 }
                        },
                        "newText": "import module\n"
                    }]
                }]
            }
        });
        let region_start_line = 5;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let list = transformed.unwrap();

        let item = &list.items[0];
        assert!(item.additional_text_edits.is_some());
        let edits = item.additional_text_edits.as_ref().unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range.start.line, 5); // 0 + 5 = 5
    }

    #[test]
    fn completion_response_with_malformed_result_returns_none() {
        // Result is a string instead of a CompletionList or CompletionItem array
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": "not_a_completion_response"
        });

        let transformed = transform_completion_response_to_host(response, 3);
        assert!(transformed.is_none());
    }

    #[test]
    fn completion_response_error_response_returns_none() {
        // JSON-RPC error response has no "result" key
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": { "code": -32600, "message": "Invalid Request" }
        });

        let transformed = transform_completion_response_to_host(response, 3);
        assert!(transformed.is_none());
    }

    #[test]
    fn completion_range_transformation_saturates_on_overflow() {
        // Test defensive arithmetic: saturating_add prevents panic on overflow
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "label": "test",
                "textEdit": {
                    "range": {
                        "start": { "line": 4294967295u32, "character": 0 },  // u32::MAX
                        "end": { "line": 4294967295u32, "character": 5 }
                    },
                    "newText": "test"
                }
            }]
        });
        let region_start_line = 10;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let list = transformed.unwrap();
        // Array format is normalized to CompletionList with isIncomplete=false
        assert!(!list.is_incomplete);

        if let Some(tower_lsp_server::ls_types::CompletionTextEdit::Edit(ref edit)) =
            list.items[0].text_edit
        {
            assert_eq!(edit.range.start.line, u32::MAX);
            assert_eq!(edit.range.end.line, u32::MAX);
        } else {
            panic!("Expected TextEdit");
        }
    }
}
