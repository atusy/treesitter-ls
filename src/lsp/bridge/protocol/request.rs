//! Request builders for LSP bridge communication.
//!
//! This module provides functions to build JSON-RPC requests for downstream
//! language servers with proper coordinate translation from host to virtual
//! document coordinates.

use super::request_id::RequestId;
use super::virtual_uri::VirtualDocumentUri;

/// Build a position-based JSON-RPC request for a downstream language server.
///
/// This is the core helper for building LSP requests that operate on a position
/// (hover, completion, definition, etc.). It handles:
/// - Creating the virtual document URI
/// - Translating host position to virtual coordinates
/// - Building the JSON-RPC request structure
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_position` - The position in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
/// * `method` - The LSP method name (e.g., "textDocument/hover")
///
/// # Defensive Arithmetic
///
/// Uses `saturating_sub` for line translation to prevent panic on underflow.
/// This can occur during race conditions when document edits invalidate region
/// data while an LSP request is in flight. In such cases, the request will use
/// line 0, which may produce incorrect results but won't crash the server.
pub(crate) fn build_position_based_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_position: tower_lsp_server::ls_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: RequestId,
    method: &str,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    // Translate position from host to virtual coordinates
    // Uses saturating_sub to prevent panic on race conditions where stale region data
    // has region_start_line > host_position.line after a document edit
    let virtual_position = tower_lsp_server::ls_types::Position {
        line: host_position.line.saturating_sub(region_start_line),
        character: host_position.character,
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.as_i64(),
        "method": method,
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            },
            "position": {
                "line": virtual_position.line,
                "character": virtual_position.character
            }
        }
    })
}

/// Build a whole-document JSON-RPC request for a downstream language server.
///
/// This is the core helper for building LSP requests that operate on an entire
/// document without position (documentLink, documentSymbol, documentColor, etc.).
/// It handles:
/// - Creating the virtual document URI
/// - Building the JSON-RPC request structure with just textDocument
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `request_id` - The JSON-RPC request ID
/// * `method` - The LSP method name (e.g., "textDocument/documentLink")
pub(crate) fn build_whole_document_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    injection_language: &str,
    region_id: &str,
    request_id: RequestId,
    method: &str,
) -> serde_json::Value {
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.as_i64(),
        "method": method,
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            }
        }
    })
}

/// Build a JSON-RPC didOpen notification for a downstream language server.
///
/// Sends the initial document content to the downstream language server when
/// a virtual document is first opened.
///
/// # Arguments
/// * `virtual_uri` - The virtual document URI
/// * `content` - The initial content of the virtual document
pub(crate) fn build_didopen_notification(
    virtual_uri: &VirtualDocumentUri,
    content: &str,
) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string(),
                "languageId": virtual_uri.language(),
                "version": 1,
                "text": content
            }
        }
    })
}

/// Build a JSON-RPC didChange notification for a downstream language server.
///
/// Uses full text sync (TextDocumentSyncKind::Full) which sends the entire
/// document content on each change. This is simpler and sufficient for bridge use.
pub(crate) fn build_didchange_notification(
    host_uri: &tower_lsp_server::ls_types::Uri,
    injection_language: &str,
    region_id: &str,
    new_content: &str,
    version: i32,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    serde_json::json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string(),
                "version": version
            },
            "contentChanges": [
                {
                    "text": new_content
                }
            ]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Uri;
    use url::Url;

    // ==========================================================================
    // Test helpers
    // ==========================================================================

    /// Standard test host URI used across most tests.
    fn test_host_uri() -> Uri {
        let url = Url::parse("file:///project/doc.md").unwrap();
        crate::lsp::lsp_impl::url_to_uri(&url).expect("test URL should convert to URI")
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

    #[test]
    fn assert_uses_virtual_uri_handles_fragments() {
        // URIs with fragments (e.g., vscode-notebook-cell://) preserve the fragment
        // The helper should correctly detect the extension before the fragment
        let request = serde_json::json!({
            "params": {
                "textDocument": {
                    "uri": "vscode-notebook-cell://authority/path/kakehashi-virtual-uri-REGION.py#cell-id"
                }
            }
        });

        // This should pass - the extension is .py even though URI ends with #cell-id
        assert_uses_virtual_uri(&request, "py");
    }

    // ==========================================================================
    // didChange notification tests
    // ==========================================================================

    #[test]
    fn didchange_notification_uses_virtual_uri() {
        let notification = build_didchange_notification(
            &test_host_uri(),
            "lua",
            "region-0",
            "local x = 42",
            2,
        );

        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "textDocument/didChange");
        assert!(
            notification.get("id").is_none(),
            "Notification should not have id"
        );
        assert_uses_virtual_uri(&notification, "lua");
        assert_eq!(notification["params"]["textDocument"]["version"], 2);
    }

    #[test]
    fn didchange_notification_contains_full_text() {
        let content = "local x = 42\nprint(x)";
        let notification =
            build_didchange_notification(&test_host_uri(), "lua", "region-0", content, 1);

        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["text"], content);
    }
}
