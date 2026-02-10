//! Inlay hint request handling for bridge connections.
//!
//! This module provides inlay hint request functionality for downstream language servers,
//! handling the bidirectional coordinate transformation between host and virtual documents.
//!
//! Unlike position-based requests, inlay hints use a range parameter in the request
//! that specifies the visible document range. Both request range (host->virtual) and
//! response positions/textEdits (virtual->host) need transformation.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{InlayHint, InlayHintLabel, Range, Uri};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri};

impl LanguageServerPool {
    /// Send an inlay hint request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the inlay hint request with range transformed to virtual coordinates
    /// 4. Wait for and return the response with positions/textEdits transformed to host
    ///
    /// Unlike position-based requests, this uses a range parameter which needs
    /// transformation from host to virtual coordinates in the request.
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_inlay_hint_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_range: Range,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        upstream_request_id: UpstreamId,
    ) -> io::Result<Option<Vec<InlayHint>>> {
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

        // Build inlay hint request
        // Note: request builder transforms host_range to virtual coordinates
        let request = build_inlay_hint_request(
            &host_uri_lsp,
            host_range,
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

        // Queue the inlay hint request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response positions and textEdits to host coordinates
        Ok(transform_inlay_hint_response_to_host(
            response?,
            &virtual_uri_string,
            &host_uri_lsp,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC inlay hint request for a downstream language server.
///
/// Unlike position-based requests (hover, definition, etc.), InlayHintParams
/// has a range field that specifies the visible document range for which
/// inlay hints should be computed. This range needs to be translated from
/// host to virtual coordinates.
///
/// # Defensive Arithmetic
///
/// Uses `saturating_sub` for line translation to prevent panic on underflow during
/// race conditions when document edits invalidate region data.
fn build_inlay_hint_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_range: Range,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: RequestId,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    // Translate range from host to virtual coordinates
    // Uses saturating_sub to prevent panic on race conditions
    let virtual_range = Range {
        start: tower_lsp_server::ls_types::Position {
            line: host_range.start.line.saturating_sub(region_start_line),
            character: host_range.start.character,
        },
        end: tower_lsp_server::ls_types::Position {
            line: host_range.end.line.saturating_sub(region_start_line),
            character: host_range.end.character,
        },
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.as_i64(),
        "method": "textDocument/inlayHint",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            },
            "range": {
                "start": {
                    "line": virtual_range.start.line,
                    "character": virtual_range.start.character
                },
                "end": {
                    "line": virtual_range.end.line,
                    "character": virtual_range.end.character
                }
            }
        }
    })
}

/// Transform an inlay hint response from virtual to host document coordinates.
///
/// InlayHint responses are arrays of items where each hint has:
/// - position: The position where the hint should appear (needs transformation)
/// - label: The hint text (string or InlayHintLabelPart[] with optional location)
/// - textEdits: Optional array of TextEdit (needs transformation)
///
/// When label is an array of InlayHintLabelPart, each part may have a location field
/// that needs URI and range transformation:
/// 1. **Real file URI** → preserved as-is
/// 2. **Same virtual URI as request** → transform coordinates, replace URI with host URI
/// 3. **Different virtual URI** (cross-region) → label part filtered out
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `request_virtual_uri` - The virtual URI from the request
/// * `host_uri` - The pre-parsed host URI to use in transformed responses
/// * `region_start_line` - Line offset to add when transforming to host coordinates
fn transform_inlay_hint_response_to_host(
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<Vec<InlayHint>> {
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
    let result = response.get_mut("result").map(serde_json::Value::take)?;

    if result.is_null() {
        return None;
    }

    // Parse into typed Vec<InlayHint>
    let mut hints: Vec<InlayHint> = serde_json::from_value(result).ok()?;

    for hint in &mut hints {
        // Transform position to host coordinates
        hint.position.line = hint.position.line.saturating_add(region_start_line);

        // Transform textEdits ranges
        if let Some(text_edits) = &mut hint.text_edits {
            for edit in text_edits.iter_mut() {
                edit.range.start.line = edit.range.start.line.saturating_add(region_start_line);
                edit.range.end.line = edit.range.end.line.saturating_add(region_start_line);
            }
        }

        // Transform label parts if label is an array (InlayHintLabelPart[])
        if let InlayHintLabel::LabelParts(parts) = &mut hint.label {
            parts.retain_mut(|part| {
                let Some(location) = &mut part.location else {
                    return true; // Parts without location are always kept
                };

                let uri_str = location.uri.as_str();

                // Case 1: Real file URI (not virtual) → keep as-is
                if !VirtualDocumentUri::is_virtual_uri(uri_str) {
                    return true;
                }

                // Case 2: Same virtual URI → transform to host coordinates
                if uri_str == request_virtual_uri {
                    location.uri = host_uri.clone();
                    location.range.start.line =
                        location.range.start.line.saturating_add(region_start_line);
                    location.range.end.line =
                        location.range.end.line.saturating_add(region_start_line);
                    return true;
                }

                // Case 3: Different virtual URI (cross-region) → filter out
                false
            });
        }
    }

    Some(hints)
}
