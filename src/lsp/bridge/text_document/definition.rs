//! Definition request handling for bridge connections.
//!
//! This module provides definition request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{GotoDefinitionResponse, Location, LocationLink, Position, Range};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri, build_position_based_request};

impl LanguageServerPool {
    /// Send a definition request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Register request with router to get oneshot receiver
    /// 4. Queue the definition request via single-writer loop
    /// 5. Wait for response via oneshot channel (no Mutex held)
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_definition_request(
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
    ) -> io::Result<Option<GotoDefinitionResponse>> {
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

        // Build definition request
        let definition_request = build_definition_request(
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

        // Queue the definition request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(definition_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates and URI
        // Cross-region virtual URIs are filtered out
        Ok(transform_definition_response_to_host(
            response?,
            &virtual_uri_string,
            host_uri.as_str(),
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC definition request for a downstream language server.
fn build_definition_request(
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
        "textDocument/definition",
    )
}

/// Parse a JSON-RPC definition response and transform coordinates to host document space.
///
/// Instead of returning a modified JSON envelope, this deserializes the response
/// into `Option<GotoDefinitionResponse>` with coordinates already transformed.
///
/// Returns `None` for: null results, missing results, and deserialization failures.
/// Returns `Some(Array(vec![]))` or `Some(Link(vec![]))` for empty arrays, preserving
/// the semantic distinction between "searched, found nothing" (empty array) and
/// "search failed/unsupported" (null).
///
/// The GotoDefinitionResponse enum has three variants:
/// - `Scalar(Location)` - single location
/// - `Array(Vec<Location>)` - array of locations (may be empty)
/// - `Link(Vec<LocationLink>)` - array of location links (may be empty)
///
/// # URI Filtering Logic
///
/// - Real file URIs → keep as-is (cross-file jumps)
/// - Same virtual URI as request → transform coordinates
/// - Different virtual URI → filter out (cross-region, can't transform)
///
/// When filtering produces an empty array, it is preserved rather than converted to `None`
/// to maintain compatibility with the previous JSON-based implementation and to provide
/// better debugging information to clients.
///
/// # Arguments
/// * `response` - Raw JSON-RPC response envelope (`{"result": {...}}`)
/// * `request_virtual_uri` - The virtual URI from the request
/// * `request_host_uri` - The host URI to use in transformed responses
/// * `region_start_line` - Line offset to add when transforming to host coordinates
fn transform_definition_response_to_host(
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    request_host_uri: &str,
    region_start_line: u32,
) -> Option<GotoDefinitionResponse> {
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
    let result = response.get_mut("result").map(serde_json::Value::take)?;
    if result.is_null() {
        return None;
    }

    // The LSP spec defines GotoDefinitionResponse as: Location | Location[] | LocationLink[]
    // Inspect structure first to determine which type to deserialize into

    if result.is_object() {
        // Must be a single Location (Scalar variant)
        if let Ok(location) = serde_json::from_value::<Location>(result) {
            return transform_location(
                location,
                request_virtual_uri,
                request_host_uri,
                region_start_line,
            )
            .map(GotoDefinitionResponse::Scalar);
        }
    } else if result.is_array() {
        // Could be Location[] or LocationLink[]
        // Inspect the first element to distinguish them
        let arr = result.as_array()?;
        if arr.is_empty() {
            // Preserve empty arrays (semantic: "searched, found nothing")
            // Return Array variant as the default for empty responses
            return Some(GotoDefinitionResponse::Array(vec![]));
        }

        // Check if first element has "targetUri" to distinguish LocationLink from Location
        if arr.first()?.get("targetUri").is_some() {
            // Try as LocationLink array (Link variant)
            if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(result) {
                let transformed: Vec<LocationLink> = links
                    .into_iter()
                    .filter_map(|link| {
                        transform_location_link(
                            link,
                            request_virtual_uri,
                            request_host_uri,
                            region_start_line,
                        )
                    })
                    .collect();

                // Preserve empty array after filtering
                return Some(GotoDefinitionResponse::Link(transformed));
            }
        } else {
            // Try as Location array (Array variant)
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result) {
                let transformed: Vec<Location> = locations
                    .into_iter()
                    .filter_map(|location| {
                        transform_location(
                            location,
                            request_virtual_uri,
                            request_host_uri,
                            region_start_line,
                        )
                    })
                    .collect();

                // Preserve empty array after filtering
                return Some(GotoDefinitionResponse::Array(transformed));
            }
        }
    }

    // Failed to deserialize as any known variant or all items were filtered out
    None
}

/// Transform a single Location to host coordinates.
///
/// Returns `None` if the location should be filtered out (cross-region virtual URI).
///
/// # URI Filtering Logic
///
/// 1. Real file URI → preserve as-is (cross-file jump to real file) - KEEP
/// 2. Same virtual URI as request → transform using request's context - KEEP
/// 3. Different virtual URI → cross-region jump - FILTER OUT
fn transform_location(
    mut location: Location,
    request_virtual_uri: &str,
    request_host_uri: &str,
    region_start_line: u32,
) -> Option<Location> {
    let uri_str = location.uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return Some(location);
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        // Parse request_host_uri back to Uri type
        let Ok(url) = url::Url::parse(request_host_uri) else {
            return None;
        };
        let Ok(host_uri) = crate::lsp::lsp_impl::url_to_uri(&url) else {
            return None;
        };

        location.uri = host_uri;
        transform_range_to_host(&mut location.range, region_start_line);
        return Some(location);
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    None
}

/// Transform a single LocationLink to host coordinates.
///
/// Returns `None` if the location should be filtered out (cross-region virtual URI).
///
/// Note: originSelectionRange stays in host coordinates (it's already correct).
fn transform_location_link(
    mut link: LocationLink,
    request_virtual_uri: &str,
    request_host_uri: &str,
    region_start_line: u32,
) -> Option<LocationLink> {
    let uri_str = link.target_uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return Some(link);
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        // Parse request_host_uri back to Uri type
        let Ok(url) = url::Url::parse(request_host_uri) else {
            return None;
        };
        let Ok(host_uri) = crate::lsp::lsp_impl::url_to_uri(&url) else {
            return None;
        };

        link.target_uri = host_uri;
        transform_range_to_host(&mut link.target_range, region_start_line);
        transform_range_to_host(&mut link.target_selection_range, region_start_line);
        return Some(link);
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    None
}

/// Transform a range from virtual to host coordinates.
///
/// Uses `saturating_add` to prevent overflow, consistent with `saturating_sub`
/// used elsewhere in the codebase for defensive arithmetic.
fn transform_range_to_host(range: &mut Range, region_start_line: u32) {
    range.start.line = range.start.line.saturating_add(region_start_line);
    range.end.line = range.end.line.saturating_add(region_start_line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Position;

    // ==========================================================================
    // Test helpers
    // ==========================================================================

    /// Standard test request ID used across most tests.
    fn test_request_id() -> RequestId {
        RequestId::new(42)
    }

    /// Standard test host URI used across most tests.
    fn test_host_uri() -> tower_lsp_server::ls_types::Uri {
        let url = url::Url::parse("file:///project/doc.md").unwrap();
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
    // Definition request tests
    // ==========================================================================

    #[test]
    fn definition_request_uses_virtual_uri() {
        let request = build_definition_request(
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
    fn definition_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_definition_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            test_request_id(),
        );

        assert_position_request(&request, "textDocument/definition", 2);
    }

    // ==========================================================================
    // Definition response transformation tests
    // ==========================================================================

    #[test]
    fn definition_response_transforms_single_location() {
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": virtual_uri,
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }
        });
        let host_uri = "file:///project/doc.md";
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(
            response,
            virtual_uri,
            host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri.as_str(), host_uri);
                assert_eq!(location.range.start.line, 3);
                assert_eq!(location.range.end.line, 3);
            }
            _ => panic!("Expected Scalar variant"),
        }
    }

    #[test]
    fn definition_response_transforms_location_array() {
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                },
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 2, "character": 8 }
                    }
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(
            response,
            virtual_uri,
            host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 2);
                assert_eq!(locations[0].range.start.line, 3);
                assert_eq!(locations[1].range.start.line, 5);
            }
            _ => panic!("Expected Array variant"),
        }
    }

    #[test]
    fn definition_response_transforms_location_link_array() {
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "targetUri": virtual_uri,
                    "targetRange": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 10 }
                    },
                    "targetSelectionRange": {
                        "start": { "line": 0, "character": 4 },
                        "end": { "line": 0, "character": 9 }
                    }
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(
            response,
            virtual_uri,
            host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Link(links) => {
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].target_uri.as_str(), host_uri);
                assert_eq!(links[0].target_range.start.line, 3);
                assert_eq!(links[0].target_selection_range.start.line, 3);
            }
            _ => panic!("Expected Link variant"),
        }
    }

    #[test]
    fn definition_response_preserves_real_file_uris() {
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
        let host_uri = "file:///project/doc.md";
        let region_start_line = 5;

        let transformed = transform_definition_response_to_host(
            response,
            virtual_uri,
            host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 1);
                assert_eq!(locations[0].uri.as_str(), real_file_uri);
                assert_eq!(locations[0].range.start.line, 10); // Unchanged
            }
            _ => panic!("Expected Array variant"),
        }
    }

    #[test]
    fn definition_response_filters_cross_region_virtual_uris() {
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
        let host_uri = "file:///project/doc.md";
        let region_start_line = 5;

        let transformed = transform_definition_response_to_host(
            response,
            request_virtual_uri,
            host_uri,
            region_start_line,
        );

        // Should filter out cross-region virtual URI, resulting in empty array
        // Preserve empty array to distinguish "found nothing" from "failed/null"
        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert!(
                    locations.is_empty(),
                    "Should have empty array after filtering"
                );
            }
            _ => panic!("Expected Array variant with empty vec"),
        }
    }

    #[test]
    fn definition_response_filters_mixed_with_cross_region() {
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
        let host_uri = "file:///project/doc.md";
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(
            response,
            request_virtual_uri,
            host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 2); // Cross-region filtered out
                assert_eq!(locations[0].uri.as_str(), host_uri);
                assert_eq!(locations[0].range.start.line, 3); // Transformed
                assert_eq!(locations[1].uri.as_str(), real_file_uri);
                assert_eq!(locations[1].range.start.line, 10); // Preserved
            }
            _ => panic!("Expected Array variant"),
        }
    }

    #[test]
    fn definition_response_with_null_result_returns_none() {
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let transformed = transform_definition_response_to_host(
            response,
            "file:///virtual.lua",
            "file:///host.md",
            5,
        );

        assert!(transformed.is_none());
    }

    #[test]
    fn definition_response_with_empty_array_preserves_empty() {
        // Server explicitly returns [] - preserve it to distinguish from null
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": []
        });

        let transformed = transform_definition_response_to_host(
            response,
            "file:///project/kakehashi-virtual-uri-region-0.lua",
            "file:///project/doc.md",
            5,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert!(
                    locations.is_empty(),
                    "Should preserve empty array from server"
                );
            }
            _ => panic!("Expected Array variant with empty vec"),
        }
    }

    #[test]
    fn definition_response_transformation_saturates_on_overflow() {
        // Test defensive arithmetic: saturating_add prevents panic on overflow
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": virtual_uri,
                "range": {
                    "start": { "line": u32::MAX, "character": 0 },
                    "end": { "line": u32::MAX, "character": 5 }
                }
            }
        });
        let host_uri = "file:///project/doc.md";
        let region_start_line = 10;

        let transformed = transform_definition_response_to_host(
            response,
            virtual_uri,
            host_uri,
            region_start_line,
        );

        assert!(transformed.is_some());
        match transformed.unwrap() {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(
                    location.range.start.line,
                    u32::MAX,
                    "Overflow should saturate at u32::MAX, not panic"
                );
            }
            _ => panic!("Expected Scalar variant"),
        }
    }
}
