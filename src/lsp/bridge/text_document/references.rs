//! References request handling for bridge connections.
//!
//! This module provides references request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{Location, Position};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{
    VirtualDocumentUri, build_position_based_request, transform_references_response_to_host,
};

impl LanguageServerPool {
    /// Send a references request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the references request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_references_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        include_declaration: bool,
        upstream_request_id: UpstreamId,
    ) -> io::Result<Option<Vec<Location>>> {
        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        // Convert host_uri to lsp_types::Uri for bridge protocol functions
        let host_uri_lsp = crate::lsp::lsp_impl::url_to_uri(host_uri)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(&host_uri_lsp, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

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

        // Build references request using position-based request builder
        let mut references_request = build_position_based_request(
            &host_uri_lsp,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
            "textDocument/references",
        );

        // Add the context parameter required by references request
        if let Some(params) = references_request.get_mut("params") {
            params["context"] = serde_json::json!({
                "includeDeclaration": include_declaration
            });
        }

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

        // Queue the references request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(references_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates and URI
        // Returns Option<Vec<Location>>, filters cross-region virtual URIs
        Ok(transform_references_response_to_host(
            response?,
            &virtual_uri_string,
            &host_uri_lsp,
            region_start_line,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::protocol::transform_references_response_to_host;

    /// Standard test host URI used across most tests.
    fn test_host_uri() -> tower_lsp_server::ls_types::Uri {
        let url = url::Url::parse("file:///project/doc.md").unwrap();
        crate::lsp::lsp_impl::url_to_uri(&url).expect("test URL should convert to URI")
    }

    // ==========================================================================
    // References response transformation tests
    // ==========================================================================

    #[test]
    fn references_response_with_null_result_returns_none() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let transformed = transform_references_response_to_host(
            response,
            "file:///virtual.lua",
            &test_host_uri(),
            5,
        );

        assert!(transformed.is_none());
    }

    #[test]
    fn references_response_with_empty_array_preserves_empty() {
        // Server explicitly returns [] - preserve to distinguish from null
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": []
        });

        let transformed = transform_references_response_to_host(
            response,
            "file:///project/kakehashi-virtual-uri-region-0.lua",
            &test_host_uri(),
            5,
        );

        assert!(transformed.is_some());
        let locations = transformed.unwrap();
        assert!(
            locations.is_empty(),
            "Should preserve empty array from server"
        );
    }

    #[test]
    fn references_response_transforms_single_location_with_same_virtual_uri() {
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 2, "character": 4 },
                        "end": { "line": 2, "character": 9 }
                    }
                }
            ]
        });
        let host_uri = test_host_uri();
        let region_start_line = 10;

        let transformed = transform_references_response_to_host(
            response,
            virtual_uri,
            &host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        let locations = transformed.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].uri, host_uri);
        assert_eq!(locations[0].range.start.line, 12); // 2 + 10
        assert_eq!(locations[0].range.end.line, 12);
        assert_eq!(locations[0].range.start.character, 4);
        assert_eq!(locations[0].range.end.character, 9);
    }

    #[test]
    fn references_response_preserves_real_file_uris() {
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let real_file_uri = "file:///project/real_file.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": real_file_uri,
                    "range": {
                        "start": { "line": 10, "character": 0 },
                        "end": { "line": 10, "character": 5 }
                    }
                }
            ]
        });
        let host_uri = test_host_uri();
        let region_start_line = 5;

        let transformed = transform_references_response_to_host(
            response,
            virtual_uri,
            &host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        let locations = transformed.unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].uri.as_str(), real_file_uri);
        assert_eq!(locations[0].range.start.line, 10); // Unchanged
    }

    #[test]
    fn references_response_filters_cross_region_virtual_uris() {
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let other_virtual_uri = "file:///project/kakehashi-virtual-uri-region-1.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": other_virtual_uri,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                }
            ]
        });
        let host_uri = test_host_uri();
        let region_start_line = 5;

        let transformed = transform_references_response_to_host(
            response,
            request_virtual_uri,
            &host_uri,
            region_start_line,
        );

        // Should filter out cross-region virtual URI, resulting in empty array
        assert!(transformed.is_some());
        let locations = transformed.unwrap();
        assert!(
            locations.is_empty(),
            "Should have empty array after filtering"
        );
    }

    #[test]
    fn references_response_filters_mixed_with_cross_region() {
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let other_virtual_uri = "file:///project/kakehashi-virtual-uri-region-1.lua";
        let real_file_uri = "file:///project/real_file.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": request_virtual_uri,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                },
                {
                    "uri": other_virtual_uri,
                    "range": {
                        "start": { "line": 5, "character": 0 },
                        "end": { "line": 5, "character": 5 }
                    }
                },
                {
                    "uri": real_file_uri,
                    "range": {
                        "start": { "line": 10, "character": 0 },
                        "end": { "line": 10, "character": 5 }
                    }
                }
            ]
        });
        let host_uri = test_host_uri();
        let region_start_line = 3;

        let transformed = transform_references_response_to_host(
            response,
            request_virtual_uri,
            &host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        let locations = transformed.unwrap();
        assert_eq!(locations.len(), 2); // Cross-region filtered out
        assert_eq!(locations[0].uri, host_uri);
        assert_eq!(locations[0].range.start.line, 3); // Transformed: 0 + 3
        assert_eq!(locations[1].uri.as_str(), real_file_uri);
        assert_eq!(locations[1].range.start.line, 10); // Preserved
    }
}
