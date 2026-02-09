//! Declaration request handling for bridge connections.
//!
//! This module provides declaration request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{Location, LocationLink, Position, Range, Uri};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri, build_position_based_request};

impl LanguageServerPool {
    /// Send a declaration request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the declaration request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_declaration_request(
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
    ) -> io::Result<Option<Vec<LocationLink>>> {
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

        // Build declaration request
        let declaration_request = build_declaration_request(
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

        // Queue the declaration request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(declaration_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response to host coordinates
        Ok(transform_declaration_response_to_host(
            response?,
            &virtual_uri_string,
            &host_uri_lsp,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC declaration request for a downstream language server.
fn build_declaration_request(
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
        "textDocument/declaration",
    )
}

/// Parse a JSON-RPC declaration response and transform coordinates to host document space.
///
/// The LSP spec defines GotoDeclarationResponse as: Location | Location[] | LocationLink[]
///
/// This function normalizes all variants to Vec<LocationLink> by:
/// - Single Location → [LocationLink]
/// - Location[] → Vec<LocationLink>
/// - LocationLink[] → Vec<LocationLink> (identity)
///
/// Empty arrays are preserved to distinguish "searched, found nothing" from search failure.
///
/// # URI Filtering
///
/// Virtual URIs are filtered based on:
/// - Same virtual URI → transformed to host URI
/// - Cross-region virtual URI → filtered out (stale coordinate data risk)
/// - Real file URI → preserved as-is
///
/// # Arguments
/// * `response` - Raw JSON-RPC response envelope (`{"result": {...}}`)
/// * `request_virtual_uri` - The virtual URI used in the request
/// * `host_uri` - The host document URI
/// * `region_start_line` - Line offset to add to ranges
fn transform_declaration_response_to_host(
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<Vec<LocationLink>> {
    // Extract result from JSON-RPC envelope using Value::take() to avoid clone
    // (performance optimization from commit d315711a)
    let result = response.get_mut("result")?.take();

    if result.is_null() {
        return None;
    }

    // Single Location → convert to LocationLink
    if result.get("uri").is_some() {
        if let Ok(location) = serde_json::from_value::<Location>(result) {
            return transform_location(location, request_virtual_uri, host_uri, region_start_line)
                .map(location_to_location_link)
                .map(|link| vec![link]);
        }
    } else if result.is_array() {
        // Could be Location[] or LocationLink[]
        let arr = result.as_array()?;
        if arr.is_empty() {
            // Preserve empty arrays (semantic: "searched, found nothing")
            return Some(vec![]);
        }

        // Check if first element has "targetUri" to distinguish LocationLink from Location
        if arr.first()?.get("targetUri").is_some() {
            // LocationLink[] → use directly
            if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(result) {
                let transformed: Vec<LocationLink> = links
                    .into_iter()
                    .filter_map(|link| {
                        transform_location_link(
                            link,
                            request_virtual_uri,
                            host_uri,
                            region_start_line,
                        )
                    })
                    .collect();

                // Preserve empty array after filtering
                return Some(transformed);
            }
        } else {
            // Location[] → convert each to LocationLink
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result) {
                let transformed: Vec<LocationLink> = locations
                    .into_iter()
                    .filter_map(|location| {
                        transform_location(
                            location,
                            request_virtual_uri,
                            host_uri,
                            region_start_line,
                        )
                        .map(location_to_location_link)
                    })
                    .collect();

                // Preserve empty array after filtering
                return Some(transformed);
            }
        }
    }

    // Failed to deserialize as any known variant or all items were filtered out
    None
}

/// Convert a Location to LocationLink format.
///
/// This is a lossless conversion - LocationLink is the more feature-rich format.
/// We set `targetSelectionRange` equal to `targetRange` since Location doesn't
/// distinguish between the full symbol range and the selection range.
fn location_to_location_link(location: Location) -> LocationLink {
    LocationLink {
        origin_selection_range: None,
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range, // Use same range for selection
    }
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
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<Location> {
    let uri_str = location.uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return Some(location);
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        location.uri = host_uri.clone();
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
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<LocationLink> {
    let uri_str = link.target_uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return Some(link);
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        link.target_uri = host_uri.clone();
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
