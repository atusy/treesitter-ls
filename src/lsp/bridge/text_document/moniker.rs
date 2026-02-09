//! Moniker request handling for bridge connections.
//!
//! This module provides moniker request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{Moniker, Position};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri, build_position_based_request};

impl LanguageServerPool {
    /// Send a moniker request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the moniker request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_moniker_request(
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
    ) -> io::Result<Option<Vec<Moniker>>> {
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

        // Build moniker request
        let moniker_request = build_moniker_request(
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

        // Queue the moniker request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(moniker_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates (pass-through for moniker)
        Ok(transform_moniker_response_to_host(response?))
    }
}

/// Build a JSON-RPC moniker request for a downstream language server.
fn build_moniker_request(
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
        "textDocument/moniker",
    )
}

/// Transform a moniker response from the downstream language server.
///
/// Moniker responses are arrays of items with scheme, identifier, unique, and kind.
/// These are non-coordinate data, so no line transformation is needed.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
fn transform_moniker_response_to_host(mut response: serde_json::Value) -> Option<Vec<Moniker>> {
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
    let result = response.get_mut("result").map(serde_json::Value::take)?;

    // Null result - return None
    if result.is_null() {
        return None;
    }

    // Parse into typed Vec<Moniker>
    // Moniker doesn't have ranges that need transformation.
    // scheme, identifier, unique, and kind are all non-coordinate data.
    serde_json::from_value(result).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn moniker_request_uses_virtual_uri() {
        use tower_lsp_server::ls_types::Uri;
        use url::Url;

        let host_uri: Uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        let position = Position {
            line: 5,
            character: 10,
        };
        let request = build_moniker_request(
            &host_uri,
            position,
            "lua",
            "region-0",
            3,
            RequestId::new(42),
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        let url = url::Url::parse(uri_str).expect("URI should be parseable");
        let filename = url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("");
        assert!(
            filename.starts_with("kakehashi-virtual-uri-") && filename.ends_with(".lua"),
            "Request should use virtual URI with .lua extension: {}",
            uri_str
        );
    }

    #[test]
    fn moniker_request_translates_position_to_virtual_coordinates() {
        use tower_lsp_server::ls_types::Uri;
        use url::Url;

        let host_uri: Uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let position = Position {
            line: 5,
            character: 10,
        };
        let request = build_moniker_request(
            &host_uri,
            position,
            "lua",
            "region-0",
            3,
            RequestId::new(42),
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/moniker");
        assert_eq!(request["params"]["position"]["line"], 2);
        assert_eq!(request["params"]["position"]["character"], 10);
    }

    #[test]
    fn moniker_response_returns_typed_vec() {
        // Moniker[] has scheme/identifier/unique/kind - no position/range data
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "scheme": "tsc",
                    "identifier": "typescript:foo:bar:Baz",
                    "unique": "document",
                    "kind": "export"
                },
                {
                    "scheme": "npm",
                    "identifier": "package:module:Class.method",
                    "unique": "scheme",
                    "kind": "local"
                }
            ]
        });

        let transformed = transform_moniker_response_to_host(response);

        assert!(transformed.is_some());
        let monikers = transformed.unwrap();
        assert_eq!(monikers.len(), 2);
        assert_eq!(monikers[0].scheme, "tsc");
        assert_eq!(monikers[0].identifier, "typescript:foo:bar:Baz");
        assert_eq!(monikers[1].scheme, "npm");
        assert_eq!(monikers[1].identifier, "package:module:Class.method");
    }

    #[test]
    fn moniker_response_with_null_result_returns_none() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_moniker_response_to_host(response);
        assert!(transformed.is_none());
    }

    #[test]
    fn moniker_response_with_empty_array_returns_empty_vec() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_moniker_response_to_host(response);
        assert!(transformed.is_some());
        let monikers = transformed.unwrap();
        assert!(monikers.is_empty());
    }

    #[test]
    fn moniker_response_with_no_result_key_returns_none() {
        // JSON-RPC error response has no "result" key
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": { "code": -32600, "message": "Invalid Request" }
        });

        let transformed = transform_moniker_response_to_host(response);
        assert!(transformed.is_none());
    }

    #[test]
    fn moniker_response_with_malformed_result_returns_none() {
        // Result is a string instead of a Moniker array
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": "not_an_array"
        });

        let transformed = transform_moniker_response_to_host(response);
        assert!(transformed.is_none());
    }
}
