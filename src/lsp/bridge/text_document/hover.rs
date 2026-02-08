//! Hover request handling for bridge connections.
//!
//! This module provides hover request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::Position;
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri};

impl LanguageServerPool {
    /// Send a hover request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Register request with router to get oneshot receiver
    /// 4. Queue the hover request via single-writer loop
    /// 5. Wait for response via oneshot channel (no Mutex held)
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_hover_request(
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
    ) -> io::Result<serde_json::Value> {
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

        // Build hover request
        let hover_request = build_hover_request(
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

        // Send didOpen notification only if document hasn't been opened yet
        // Uses ConnectionHandleSender wrapper for MessageSender trait
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

        // Queue the hover request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(hover_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates
        Ok(transform_hover_response_to_host(
            response?,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC hover request for a downstream language server.
///
/// This function handles:
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
///
/// # Defensive Arithmetic
///
/// Uses `saturating_sub` for line translation to prevent panic on underflow.
/// This can occur during race conditions when document edits invalidate region
/// data while an LSP request is in flight. In such cases, the request will use
/// line 0, which may produce incorrect results but won't crash the server.
fn build_hover_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_position: tower_lsp_server::ls_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: RequestId,
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
        "method": "textDocument/hover",
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

/// Transform a hover response from virtual to host document coordinates.
///
/// If the response contains a range, translates the line numbers from virtual
/// document coordinates back to host document coordinates by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
fn transform_hover_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Check if response has a result with a range
    if let Some(result) = response.get_mut("result")
        && result.is_object()
        && let Some(range) = result.get_mut("range")
        && range.is_object()
    {
        transform_range(range, region_start_line);
    }

    response
}

/// Transform a range's line numbers from virtual to host coordinates.
///
/// Uses saturating_add to prevent overflow for large line numbers, consistent
/// with saturating_sub used elsewhere in the codebase.
fn transform_range(range: &mut serde_json::Value, region_start_line: u32) {
    // Transform start position
    if let Some(start) = range.get_mut("start")
        && let Some(line) = start.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
    }

    // Transform end position
    if let Some(end) = range.get_mut("end")
        && let Some(line) = end.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    // Hover request tests
    // ==========================================================================

    #[test]
    fn hover_request_uses_virtual_uri() {
        let request = build_hover_request(
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
    fn hover_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_hover_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            test_request_id(),
        );

        assert_position_request(&request, "textDocument/hover", 2);
    }

    #[test]
    fn position_translation_at_region_start_becomes_line_zero() {
        // When cursor is at the first line of the region, virtual line should be 0
        let host_position = Position {
            line: 3, // Same as region_start_line
            character: 5,
        };

        let request = build_hover_request(
            &test_host_uri(),
            host_position,
            "lua",
            "region-0",
            3,
            test_request_id(),
        );

        assert_eq!(
            request["params"]["position"]["line"], 0,
            "Position at region start should translate to line 0"
        );
    }

    #[test]
    fn position_translation_with_zero_region_start() {
        // Region starting at line 0 (e.g., first line of document)
        let host_position = Position {
            line: 5,
            character: 0,
        };

        let request = build_hover_request(
            &test_host_uri(),
            host_position,
            "lua",
            "region-0",
            0,
            test_request_id(),
        );

        assert_eq!(
            request["params"]["position"]["line"], 5,
            "With region_start_line=0, virtual line equals host line"
        );
    }

    #[test]
    fn position_translation_saturates_on_underflow() {
        // Simulate race condition: host_position.line (2) < region_start_line (5)
        // This should NOT panic, instead saturate to line 0
        let host_position = Position {
            line: 2, // Less than region_start_line
            character: 10,
        };

        let request = build_hover_request(
            &test_host_uri(),
            host_position,
            "lua",
            "region-0",
            5, // region_start_line > host_position.line
            test_request_id(),
        );

        assert_eq!(
            request["params"]["position"]["line"], 0,
            "Underflow should saturate to line 0, not panic"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }
}
