//! Color presentation request handling for bridge connections.
//!
//! This module provides color presentation request functionality for downstream language servers,
//! handling the bidirectional coordinate transformation between host and virtual documents.
//!
//! Like inlay hints, color presentation uses a range parameter in the request (the range
//! where the color was found) and the response may contain textEdits and additionalTextEdits
//! that need transformation back to host coordinates.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use log::warn;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{ColorPresentation, Range};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri};

impl LanguageServerPool {
    /// Send a color presentation request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Register request with router to get oneshot receiver
    /// 4. Queue the color presentation request via single-writer loop
    /// 5. Wait for response via oneshot channel (no Mutex held)
    ///
    /// Like inlay hints, this uses a range parameter which needs transformation from
    /// host to virtual coordinates in the request, and textEdits in the response need
    /// transformation back to host coordinates.
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_color_presentation_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_range: Range,
        color: &serde_json::Value,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        upstream_request_id: UpstreamId,
    ) -> io::Result<Vec<ColorPresentation>> {
        // Convert url::Url to ls_types::Uri for protocol functions
        let host_uri_lsp = crate::lsp::lsp_impl::url_to_uri(host_uri)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(server_name, server_config)
            .await?;

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

        // Build color presentation request
        // Note: request builder transforms host_range to virtual coordinates
        let request = build_color_presentation_request(
            &host_uri_lsp,
            host_range,
            color,
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

        // Queue the color presentation request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform response textEdits and additionalTextEdits to host coordinates
        Ok(transform_color_presentation_response_to_host(
            response?,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC color presentation request for a downstream language server.
///
/// ColorPresentationParams has a range field that specifies the color's location
/// in the document. This range needs to be translated from host to virtual coordinates.
///
/// # Defensive Arithmetic
///
/// Uses `saturating_sub` for line translation to prevent panic on underflow during
/// race conditions when document edits invalidate region data.
fn build_color_presentation_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_range: Range,
    color: &serde_json::Value,
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
        "method": "textDocument/colorPresentation",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            },
            "color": color,
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

/// Transform a color presentation response from virtual to host document coordinates.
///
/// ColorPresentation responses are arrays of ColorPresentation items, each containing:
/// - label: The presentation label (preserved unchanged)
/// - textEdit: Optional TextEdit with range (needs transformation)
/// - additionalTextEdits: Optional array of TextEdits (ranges need transformation)
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
fn transform_color_presentation_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> Vec<ColorPresentation> {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for textDocument/colorPresentation: {}", error);
    }
    let Some(result) = response.get_mut("result").map(serde_json::Value::take) else {
        return vec![];
    };

    if result.is_null() {
        return vec![];
    }

    // Parse into typed Vec<ColorPresentation>
    let mut presentations: Vec<ColorPresentation> = match serde_json::from_value(result) {
        Ok(presentations) => presentations,
        Err(_) => return vec![],
    };

    // Transform textEdit and additionalTextEdits ranges to host coordinates
    for presentation in &mut presentations {
        if let Some(text_edit) = &mut presentation.text_edit {
            text_edit.range.start.line =
                text_edit.range.start.line.saturating_add(region_start_line);
            text_edit.range.end.line = text_edit.range.end.line.saturating_add(region_start_line);
        }

        if let Some(additional_edits) = &mut presentation.additional_text_edits {
            for edit in additional_edits.iter_mut() {
                edit.range.start.line = edit.range.start.line.saturating_add(region_start_line);
                edit.range.end.line = edit.range.end.line.saturating_add(region_start_line);
            }
        }
    }

    presentations
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==========================================================================
    // Color presentation request tests
    // ==========================================================================

    #[test]
    fn color_presentation_request_uses_virtual_uri() {
        use tower_lsp_server::ls_types::Position;
        use url::Url;

        let host_uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        let host_range = Range {
            start: Position {
                line: 5,
                character: 10,
            },
            end: Position {
                line: 5,
                character: 17,
            },
        };
        let color = json!({
            "red": 1.0,
            "green": 0.0,
            "blue": 0.0,
            "alpha": 1.0
        });
        let request = build_color_presentation_request(
            &host_uri,
            host_range,
            &color,
            "lua",
            "region-0",
            3,
            RequestId::new(42),
        );

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
    fn color_presentation_request_transforms_range_to_virtual_coordinates() {
        use tower_lsp_server::ls_types::Position;
        use url::Url;

        let host_uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        // Host range: line 5, region starts at line 3
        // Virtual range should be: line 2 (5-3=2)
        let host_range = Range {
            start: Position {
                line: 5,
                character: 10,
            },
            end: Position {
                line: 5,
                character: 17,
            },
        };
        let color = json!({
            "red": 1.0,
            "green": 0.0,
            "blue": 0.0,
            "alpha": 1.0
        });
        let request = build_color_presentation_request(
            &host_uri,
            host_range,
            &color,
            "lua",
            "region-0",
            3,
            RequestId::new(42),
        );

        let range = &request["params"]["range"];
        assert_eq!(
            range["start"]["line"], 2,
            "Start line should be translated from 5 to 2 (5-3)"
        );
        assert_eq!(
            range["start"]["character"], 10,
            "Start character should remain unchanged"
        );
        assert_eq!(
            range["end"]["line"], 2,
            "End line should be translated from 5 to 2 (5-3)"
        );
        assert_eq!(
            range["end"]["character"], 17,
            "End character should remain unchanged"
        );
    }

    #[test]
    fn color_presentation_request_includes_color() {
        use tower_lsp_server::ls_types::Position;
        use url::Url;

        let host_uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        let host_range = Range {
            start: Position {
                line: 3,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 7,
            },
        };
        let color = json!({
            "red": 0.5,
            "green": 0.25,
            "blue": 0.75,
            "alpha": 1.0
        });
        let request = build_color_presentation_request(
            &host_uri,
            host_range,
            &color,
            "lua",
            "region-0",
            3,
            RequestId::new(42),
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/colorPresentation");
        assert_eq!(request["params"]["color"]["red"], 0.5);
        assert_eq!(request["params"]["color"]["green"], 0.25);
        assert_eq!(request["params"]["color"]["blue"], 0.75);
        assert_eq!(request["params"]["color"]["alpha"], 1.0);
    }

    #[test]
    fn color_presentation_request_range_saturates_on_underflow() {
        use tower_lsp_server::ls_types::Position;
        use url::Url;

        let host_uri =
            crate::lsp::lsp_impl::url_to_uri(&Url::parse("file:///project/doc.md").unwrap())
                .unwrap();
        // Simulate race condition: range lines < region_start_line
        let host_range = Range {
            start: Position {
                line: 2,
                character: 5,
            },
            end: Position {
                line: 2,
                character: 12,
            },
        };
        let color = json!({
            "red": 1.0,
            "green": 0.0,
            "blue": 0.0,
            "alpha": 1.0
        });

        let request = build_color_presentation_request(
            &host_uri,
            host_range,
            &color,
            "lua",
            "region-0",
            10, // region_start_line > range lines
            RequestId::new(42),
        );

        let range = &request["params"]["range"];
        assert_eq!(
            range["start"]["line"], 0,
            "Start line underflow should saturate to 0"
        );
        assert_eq!(
            range["end"]["line"], 0,
            "End line underflow should saturate to 0"
        );
    }

    // ==========================================================================
    // Color presentation response transformation tests
    // ==========================================================================

    #[test]
    fn color_presentation_response_transforms_text_edit_range_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "label": "#ff0000",
                    "textEdit": {
                        "range": {
                            "start": { "line": 0, "character": 10 },
                            "end": { "line": 0, "character": 17 }
                        },
                        "newText": "#ff0000"
                    }
                }
            ]
        });
        let region_start_line = 5;

        let presentations =
            transform_color_presentation_response_to_host(response, region_start_line);

        assert_eq!(presentations.len(), 1);
        let text_edit = presentations[0].text_edit.as_ref().unwrap();
        assert_eq!(text_edit.range.start.line, 5);
        assert_eq!(text_edit.range.end.line, 5);
        assert_eq!(presentations[0].label, "#ff0000");
        assert_eq!(text_edit.new_text, "#ff0000");
    }

    #[test]
    fn color_presentation_response_transforms_additional_text_edits_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "label": "rgb(255, 0, 0)",
                "textEdit": {
                    "range": {
                        "start": { "line": 2, "character": 5 },
                        "end": { "line": 2, "character": 12 }
                    },
                    "newText": "rgb(255, 0, 0)"
                },
                "additionalTextEdits": [
                    {
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 0 }
                        },
                        "newText": "import { rgb } from 'colors';\n"
                    },
                    {
                        "range": {
                            "start": { "line": 4, "character": 0 },
                            "end": { "line": 4, "character": 10 }
                        },
                        "newText": "cleanup()"
                    }
                ]
            }]
        });
        let region_start_line = 3;

        let presentations =
            transform_color_presentation_response_to_host(response, region_start_line);

        assert_eq!(presentations.len(), 1);
        let text_edit = presentations[0].text_edit.as_ref().unwrap();
        assert_eq!(text_edit.range.start.line, 5);
        assert_eq!(text_edit.range.end.line, 5);

        let additional = presentations[0].additional_text_edits.as_ref().unwrap();
        assert_eq!(additional.len(), 2);
        assert_eq!(additional[0].range.start.line, 3);
        assert_eq!(additional[0].range.end.line, 3);
        assert_eq!(additional[1].range.start.line, 7);
        assert_eq!(additional[1].range.end.line, 7);
    }

    #[test]
    fn color_presentation_response_without_text_edit_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                { "label": "#ff0000" },
                { "label": "rgb(255, 0, 0)" },
                { "label": "hsl(0, 100%, 50%)" }
            ]
        });
        let region_start_line = 5;

        let presentations =
            transform_color_presentation_response_to_host(response, region_start_line);

        assert_eq!(presentations.len(), 3);
        assert_eq!(presentations[0].label, "#ff0000");
        assert_eq!(presentations[1].label, "rgb(255, 0, 0)");
        assert_eq!(presentations[2].label, "hsl(0, 100%, 50%)");
    }

    #[test]
    fn color_presentation_response_with_null_result_returns_empty() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let presentations = transform_color_presentation_response_to_host(response, 5);
        assert!(presentations.is_empty());
    }

    #[test]
    fn color_presentation_response_with_empty_array_returns_empty() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let presentations = transform_color_presentation_response_to_host(response, 5);
        assert!(presentations.is_empty());
    }

    #[test]
    fn color_presentation_response_with_no_result_key_returns_empty() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": { "code": -32600, "message": "Invalid Request" }
        });

        let presentations = transform_color_presentation_response_to_host(response, 5);
        assert!(presentations.is_empty());
    }

    #[test]
    fn color_presentation_response_with_malformed_result_returns_empty() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": "not_an_array"
        });

        let presentations = transform_color_presentation_response_to_host(response, 5);
        assert!(presentations.is_empty());
    }
}
