//! Document link request handling for bridge connections.
//!
//! This module provides document link request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! Unlike position-based requests (hover, definition, etc.), document link requests
//! operate on the entire document - they don't take a position parameter.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use log::warn;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::DocumentLink;
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri, build_whole_document_request};

impl LanguageServerPool {
    /// Send a document link request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the document link request
    /// 4. Wait for and return the response
    ///
    /// Unlike position-based requests, document link operates on the entire document,
    /// so no position translation is needed for the request.
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_document_link_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        upstream_request_id: UpstreamId,
    ) -> io::Result<Option<Vec<DocumentLink>>> {
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

        // Build document link request
        // Note: document link doesn't need position - it operates on the whole document
        let request =
            build_document_link_request(&host_uri_lsp, injection_language, region_id, request_id);

        // Use a closure for cleanup on any failure path
        let cleanup = || {
            handle.router().remove(request_id);
            self.unregister_upstream_request(&upstream_request_id, server_name);
        };

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

        // Queue the document link request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates
        Ok(transform_document_link_response_to_host(
            response?,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC document link request for a downstream language server.
///
/// Unlike position-based requests (hover, definition, etc.), DocumentLinkParams
/// only has a textDocument field - no position. The request asks for all links
/// in the entire document.
fn build_document_link_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    injection_language: &str,
    region_id: &str,
    request_id: RequestId,
) -> serde_json::Value {
    build_whole_document_request(
        host_uri,
        injection_language,
        region_id,
        request_id,
        "textDocument/documentLink",
    )
}

/// Transform a document link response from virtual to host document coordinates.
///
/// DocumentLink responses are arrays of items with range, target, tooltip, and data fields.
/// Only the range needs transformation - target, tooltip, and data are preserved unchanged.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
fn transform_document_link_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> Option<Vec<DocumentLink>> {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for textDocument/documentLink: {}", error);
    }
    let result = response.get_mut("result").map(serde_json::Value::take)?;

    if result.is_null() {
        return None;
    }

    // Parse into typed Vec<DocumentLink>
    let mut links: Vec<DocumentLink> = serde_json::from_value(result).ok()?;

    // Transform ranges to host coordinates
    for link in &mut links {
        link.range.start.line = link.range.start.line.saturating_add(region_start_line);
        link.range.end.line = link.range.end.line.saturating_add(region_start_line);
    }

    Some(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==========================================================================
    // Document link request tests
    // ==========================================================================

    #[test]
    fn document_link_request_uses_virtual_uri() {
        use tower_lsp_server::ls_types::Uri;
        use url::Url;

        let host_uri: Uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        let request = build_document_link_request(&host_uri, "lua", "region-0", RequestId::new(42));

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            VirtualDocumentUri::is_virtual_uri(uri_str),
            "Request should use a virtual URI: {}",
            uri_str
        );
        assert!(
            uri_str.ends_with(".lua"),
            "Virtual URI should have .lua extension: {}",
            uri_str
        );
    }

    #[test]
    fn document_link_request_has_correct_method_and_no_position() {
        use tower_lsp_server::ls_types::Uri;
        use url::Url;

        let host_uri: Uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        let request =
            build_document_link_request(&host_uri, "lua", "region-0", RequestId::new(123));

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 123);
        assert_eq!(request["method"], "textDocument/documentLink");
        assert!(
            request["params"].get("position").is_none(),
            "DocumentLink request should not have position parameter"
        );
    }

    // ==========================================================================
    // Document link response transformation tests
    // ==========================================================================

    #[test]
    fn document_link_response_transforms_ranges_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 10 },
                        "end": { "line": 0, "character": 25 }
                    },
                    "target": "file:///some/module.lua"
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 5 },
                        "end": { "line": 2, "character": 15 }
                    }
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let links = transformed.unwrap();
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].range.start.line, 5);
        assert_eq!(links[0].range.end.line, 5);
        assert_eq!(links[0].range.start.character, 10);
        assert_eq!(
            links[0].target.as_ref().map(|u| u.as_str()),
            Some("file:///some/module.lua")
        );
        assert_eq!(links[1].range.start.line, 7);
        assert_eq!(links[1].range.end.line, 7);
    }

    #[test]
    fn document_link_response_with_null_result_returns_none() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_link_response_to_host(response, 5);
        assert!(transformed.is_none());
    }

    #[test]
    fn document_link_response_with_empty_array_returns_empty_vec() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_link_response_to_host(response, 5);
        assert!(transformed.is_some());
        let links = transformed.unwrap();
        assert!(links.is_empty());
    }

    #[test]
    fn document_link_response_preserves_target_and_tooltip() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 10 }
                },
                "target": "file:///target.lua",
                "tooltip": "Go to definition"
            }]
        });
        let region_start_line = 3;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let links = transformed.unwrap();
        assert_eq!(links[0].range.start.line, 3);
        assert_eq!(
            links[0].target.as_ref().map(|u| u.as_str()),
            Some("file:///target.lua")
        );
        assert_eq!(links[0].tooltip.as_deref(), Some("Go to definition"));
    }

    #[test]
    fn document_link_response_without_target_transforms_range() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "range": {
                    "start": { "line": 1, "character": 5 },
                    "end": { "line": 1, "character": 20 }
                }
            }]
        });
        let region_start_line = 10;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let links = transformed.unwrap();
        assert_eq!(links[0].range.start.line, 11);
        assert_eq!(links[0].range.end.line, 11);
        assert!(links[0].target.is_none());
    }

    #[test]
    fn document_link_response_with_no_result_key_returns_none() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": { "code": -32600, "message": "Invalid Request" }
        });

        let transformed = transform_document_link_response_to_host(response, 5);
        assert!(transformed.is_none());
    }

    #[test]
    fn document_link_response_with_malformed_result_returns_none() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": "not_an_array"
        });

        let transformed = transform_document_link_response_to_host(response, 5);
        assert!(transformed.is_none());
    }

    #[test]
    fn document_link_response_transformation_saturates_on_overflow() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "range": {
                    "start": { "line": u32::MAX, "character": 0 },
                    "end": { "line": u32::MAX, "character": 5 }
                }
            }]
        });
        let region_start_line = 10;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let links = transformed.unwrap();
        assert_eq!(
            links[0].range.start.line,
            u32::MAX,
            "Overflow should saturate at u32::MAX, not panic"
        );
    }
}
