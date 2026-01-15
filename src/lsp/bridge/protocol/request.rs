//! Request builders for LSP bridge communication.
//!
//! This module provides functions to build JSON-RPC requests for downstream
//! language servers with proper coordinate translation from host to virtual
//! document coordinates.

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
/// # Preconditions
///
/// **`host_position.line >= region_start_line`** - The host position must be within or after
/// the injection region. This is guaranteed by callers which only invoke bridge requests
/// when the cursor position falls within a detected injection region's line range.
/// Violation would cause underflow in debug builds (wrapping in release).
fn build_position_based_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
    method: &str,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    // Translate position from host to virtual coordinates
    let virtual_position = tower_lsp::lsp_types::Position {
        line: host_position.line - region_start_line,
        character: host_position.character,
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
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

/// Build a JSON-RPC hover request for a downstream language server.
pub(crate) fn build_bridge_hover_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/hover",
    )
}

/// Build a JSON-RPC signature help request for a downstream language server.
pub(crate) fn build_bridge_signature_help_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/signatureHelp",
    )
}

/// Build a JSON-RPC completion request for a downstream language server.
pub(crate) fn build_bridge_completion_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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

/// Build a JSON-RPC definition request for a downstream language server.
pub(crate) fn build_bridge_definition_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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

/// Build a JSON-RPC typeDefinition request for a downstream language server.
pub(crate) fn build_bridge_type_definition_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/typeDefinition",
    )
}

/// Build a JSON-RPC implementation request for a downstream language server.
pub(crate) fn build_bridge_implementation_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/implementation",
    )
}

/// Build a JSON-RPC declaration request for a downstream language server.
pub(crate) fn build_bridge_declaration_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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

/// Build a JSON-RPC document highlight request for a downstream language server.
pub(crate) fn build_bridge_document_highlight_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/documentHighlight",
    )
}

/// Build a JSON-RPC references request for a downstream language server.
///
/// Note: References request has an additional `context.includeDeclaration` parameter
/// that other position-based requests don't have.
pub(crate) fn build_bridge_references_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    include_declaration: bool,
    request_id: i64,
) -> serde_json::Value {
    let mut request = build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/references",
    );

    // Add the context parameter required by references request
    if let Some(params) = request.get_mut("params") {
        params["context"] = serde_json::json!({
            "includeDeclaration": include_declaration
        });
    }

    request
}

/// Build a JSON-RPC rename request for a downstream language server.
///
/// Note: Rename request has an additional `newName` parameter that specifies
/// the new name for the symbol being renamed.
pub(crate) fn build_bridge_rename_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    new_name: &str,
    request_id: i64,
) -> serde_json::Value {
    let mut request = build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/rename",
    );

    // Add the newName parameter required by rename request
    if let Some(params) = request.get_mut("params") {
        params["newName"] = serde_json::json!(new_name);
    }

    request
}

/// Build a JSON-RPC document link request for a downstream language server.
///
/// Unlike position-based requests (hover, definition, etc.), DocumentLinkParams
/// only has a textDocument field - no position. The request asks for all links
/// in the entire document.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_document_link_request(
    host_uri: &tower_lsp::lsp_types::Url,
    injection_language: &str,
    region_id: &str,
    request_id: i64,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "textDocument/documentLink",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            }
        }
    })
}

/// Build a JSON-RPC document symbol request for a downstream language server.
///
/// Similar to DocumentLinkParams, DocumentSymbolParams also only has a
/// textDocument field (no position). The request asks for all symbols in the
/// entire document.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_document_symbol_request(
    host_uri: &tower_lsp::lsp_types::Url,
    injection_language: &str,
    region_id: &str,
    request_id: i64,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "textDocument/documentSymbol",
        "params": {
            "textDocument": {
                "uri": virtual_uri.to_uri_string()
            }
        }
    })
}

/// Build a JSON-RPC inlay hint request for a downstream language server.
///
/// Unlike position-based requests (hover, definition, etc.), InlayHintParams
/// has a range field that specifies the visible document range for which
/// inlay hints should be computed. This range needs to be translated from
/// host to virtual coordinates.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_range` - The range in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
///
/// # Preconditions
///
/// **`host_range.start.line >= region_start_line`** - The host range must be within or after
/// the injection region. This is guaranteed by callers which only invoke bridge requests
/// when the range falls within a detected injection region's line range.
pub(crate) fn build_bridge_inlay_hint_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_range: tower_lsp::lsp_types::Range,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    // Translate range from host to virtual coordinates
    let virtual_range = tower_lsp::lsp_types::Range {
        start: tower_lsp::lsp_types::Position {
            line: host_range.start.line - region_start_line,
            character: host_range.start.character,
        },
        end: tower_lsp::lsp_types::Position {
            line: host_range.end.line - region_start_line,
            character: host_range.end.character,
        },
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
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

/// Build a JSON-RPC color presentation request for a downstream language server.
///
/// ColorPresentationParams has a range field that specifies the color's location
/// in the document. This range needs to be translated from host to virtual coordinates.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_range` - The range in the host document where the color is located
/// * `color` - The color value (RGBA) to get presentations for
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
///
/// # Preconditions
///
/// **`host_range.start.line >= region_start_line`** - The host range must be within or after
/// the injection region. This is guaranteed by callers which only invoke bridge requests
/// when the range falls within a detected injection region's line range.
pub(crate) fn build_bridge_color_presentation_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_range: tower_lsp::lsp_types::Range,
    color: &serde_json::Value,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    // Translate range from host to virtual coordinates
    let virtual_range = tower_lsp::lsp_types::Range {
        start: tower_lsp::lsp_types::Position {
            line: host_range.start.line - region_start_line,
            character: host_range.start.character,
        },
        end: tower_lsp::lsp_types::Position {
            line: host_range.end.line - region_start_line,
            character: host_range.end.character,
        },
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
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

/// Build a JSON-RPC moniker request for a downstream language server.
pub(crate) fn build_bridge_moniker_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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

/// Build a JSON-RPC document color request for a downstream language server.
///
/// Similar to DocumentLinkParams, DocumentColorParams also only has a
/// textDocument field (no position). The request asks for all colors in the
/// entire document.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_document_color_request(
    host_uri: &tower_lsp::lsp_types::Url,
    injection_language: &str,
    region_id: &str,
    request_id: i64,
) -> serde_json::Value {
    // Create virtual document URI
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "textDocument/documentColor",
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
pub(crate) fn build_bridge_didopen_notification(
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
pub(crate) fn build_bridge_didchange_notification(
    host_uri: &tower_lsp::lsp_types::Url,
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
    use tower_lsp::lsp_types::{Position, Url};

    // ==========================================================================
    // Test helpers
    // ==========================================================================

    /// Standard test host URI used across most tests.
    fn test_host_uri() -> Url {
        Url::parse("file:///project/doc.md").unwrap()
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
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/")
                && uri_str.ends_with(&format!(".{}", extension)),
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
        let request =
            build_bridge_hover_request(&test_host_uri(), test_position(), "lua", "region-0", 3, 42);

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn hover_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request =
            build_bridge_hover_request(&test_host_uri(), test_position(), "lua", "region-0", 3, 42);

        assert_position_request(&request, "textDocument/hover", 2);
    }

    #[test]
    fn position_translation_at_region_start_becomes_line_zero() {
        // When cursor is at the first line of the region, virtual line should be 0
        let host_position = Position {
            line: 3, // Same as region_start_line
            character: 5,
        };

        let request =
            build_bridge_hover_request(&test_host_uri(), host_position, "lua", "region-0", 3, 42);

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

        let request =
            build_bridge_hover_request(&test_host_uri(), host_position, "lua", "region-0", 0, 42);

        assert_eq!(
            request["params"]["position"]["line"], 5,
            "With region_start_line=0, virtual line equals host line"
        );
    }

    // ==========================================================================
    // didChange notification tests
    // ==========================================================================

    #[test]
    fn didchange_notification_uses_virtual_uri() {
        let notification = build_bridge_didchange_notification(
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
            build_bridge_didchange_notification(&test_host_uri(), "lua", "region-0", content, 1);

        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["text"], content);
    }

    // ==========================================================================
    // Completion request tests
    // ==========================================================================

    #[test]
    fn completion_request_uses_virtual_uri() {
        let request = build_bridge_completion_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn completion_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_completion_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/completion", 2);
    }

    // ==========================================================================
    // SignatureHelp request tests
    // ==========================================================================

    #[test]
    fn signature_help_request_uses_virtual_uri() {
        let request = build_bridge_signature_help_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn signature_help_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_signature_help_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/signatureHelp", 2);
    }

    // ==========================================================================
    // Definition request tests
    // ==========================================================================

    #[test]
    fn definition_request_uses_virtual_uri() {
        let request = build_bridge_definition_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn definition_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_definition_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/definition", 2);
    }

    // ==========================================================================
    // TypeDefinition request tests
    // ==========================================================================

    #[test]
    fn type_definition_request_uses_virtual_uri() {
        let request = build_bridge_type_definition_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn type_definition_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_type_definition_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/typeDefinition", 2);
    }

    // ==========================================================================
    // Implementation request tests
    // ==========================================================================

    #[test]
    fn implementation_request_uses_virtual_uri() {
        let request = build_bridge_implementation_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn implementation_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_implementation_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/implementation", 2);
    }

    // ==========================================================================
    // Declaration request tests
    // ==========================================================================

    #[test]
    fn declaration_request_uses_virtual_uri() {
        let request = build_bridge_declaration_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn declaration_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_declaration_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/declaration", 2);
    }

    // ==========================================================================
    // References request tests
    // ==========================================================================

    #[test]
    fn references_request_uses_virtual_uri() {
        let request = build_bridge_references_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            true, // include_declaration
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn references_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_references_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            true, // include_declaration
            42,
        );

        assert_position_request(&request, "textDocument/references", 2);
    }

    #[test]
    fn references_request_includes_context_with_include_declaration_true() {
        let request = build_bridge_references_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            true, // include_declaration = true
            42,
        );

        assert_eq!(
            request["params"]["context"]["includeDeclaration"], true,
            "Context should include includeDeclaration = true"
        );
    }

    #[test]
    fn references_request_includes_context_with_include_declaration_false() {
        let request = build_bridge_references_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            false, // include_declaration = false
            42,
        );

        assert_eq!(
            request["params"]["context"]["includeDeclaration"], false,
            "Context should include includeDeclaration = false"
        );
    }

    // ==========================================================================
    // Document highlight request tests
    // ==========================================================================

    #[test]
    fn document_highlight_request_uses_virtual_uri() {
        let request = build_bridge_document_highlight_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn document_highlight_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_document_highlight_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/documentHighlight", 2);
    }

    // ==========================================================================
    // Rename request tests
    // ==========================================================================

    #[test]
    fn rename_request_uses_virtual_uri() {
        let request = build_bridge_rename_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            "newName",
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn rename_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_rename_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            "newName",
            42,
        );

        assert_position_request(&request, "textDocument/rename", 2);
    }

    #[test]
    fn rename_request_includes_new_name() {
        let request = build_bridge_rename_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            "renamedVariable",
            42,
        );

        assert_eq!(
            request["params"]["newName"], "renamedVariable",
            "Request should include newName parameter"
        );
    }

    // ==========================================================================
    // Document link request tests
    // ==========================================================================

    #[test]
    fn document_link_request_uses_virtual_uri() {
        let request = build_bridge_document_link_request(&test_host_uri(), "lua", "region-0", 42);

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn document_link_request_has_correct_method_and_structure() {
        let request = build_bridge_document_link_request(&test_host_uri(), "lua", "region-0", 123);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 123);
        assert_eq!(request["method"], "textDocument/documentLink");
        // DocumentLink request has no position parameter
        assert!(
            request["params"].get("position").is_none(),
            "DocumentLink request should not have position parameter"
        );
    }

    #[test]
    fn document_link_request_different_languages_produce_different_extensions() {
        let lua_request =
            build_bridge_document_link_request(&test_host_uri(), "lua", "region-0", 1);
        let python_request =
            build_bridge_document_link_request(&test_host_uri(), "python", "region-0", 1);

        assert_uses_virtual_uri(&lua_request, "lua");
        assert_uses_virtual_uri(&python_request, "py");
    }

    // ==========================================================================
    // Document symbol request tests
    // ==========================================================================

    #[test]
    fn document_symbol_request_uses_virtual_uri() {
        let request = build_bridge_document_symbol_request(&test_host_uri(), "lua", "region-0", 42);

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn document_symbol_request_has_correct_method_and_structure() {
        let request =
            build_bridge_document_symbol_request(&test_host_uri(), "lua", "region-0", 123);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 123);
        assert_eq!(request["method"], "textDocument/documentSymbol");
        // DocumentSymbol request has no position parameter (whole-document operation)
        assert!(
            request["params"].get("position").is_none(),
            "DocumentSymbol request should not have position parameter"
        );
    }

    // ==========================================================================
    // Inlay hint request tests
    // ==========================================================================

    #[test]
    fn inlay_hint_request_uses_virtual_uri() {
        use tower_lsp::lsp_types::Range;

        let host_range = Range {
            start: Position {
                line: 5,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 20,
            },
        };
        let request = build_bridge_inlay_hint_request(
            &test_host_uri(),
            host_range,
            "lua",
            "region-0",
            3, // region_start_line
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn inlay_hint_request_has_correct_method_and_structure() {
        use tower_lsp::lsp_types::Range;

        let host_range = Range {
            start: Position {
                line: 5,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 20,
            },
        };
        let request = build_bridge_inlay_hint_request(
            &test_host_uri(),
            host_range,
            "lua",
            "region-0",
            3,
            123,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 123);
        assert_eq!(request["method"], "textDocument/inlayHint");
        // InlayHint request has range, not position
        assert!(
            request["params"].get("range").is_some(),
            "InlayHint request should have range parameter"
        );
        assert!(
            request["params"].get("position").is_none(),
            "InlayHint request should not have position parameter"
        );
    }

    #[test]
    fn inlay_hint_request_transforms_range_to_virtual_coordinates() {
        use tower_lsp::lsp_types::Range;

        // Host range: lines 5-10, region starts at line 3
        // Virtual range should be: lines 2-7 (5-3=2, 10-3=7)
        let host_range = Range {
            start: Position {
                line: 5,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 20,
            },
        };
        let request = build_bridge_inlay_hint_request(
            &test_host_uri(),
            host_range,
            "lua",
            "region-0",
            3, // region_start_line
            42,
        );

        let range = &request["params"]["range"];
        assert_eq!(
            range["start"]["line"], 2,
            "Start line should be translated from 5 to 2 (5-3)"
        );
        assert_eq!(
            range["start"]["character"], 0,
            "Start character should remain unchanged"
        );
        assert_eq!(
            range["end"]["line"], 7,
            "End line should be translated from 10 to 7 (10-3)"
        );
        assert_eq!(
            range["end"]["character"], 20,
            "End character should remain unchanged"
        );
    }

    #[test]
    fn inlay_hint_request_at_region_start_becomes_line_zero() {
        use tower_lsp::lsp_types::Range;

        // When range starts at region_start_line, virtual start should be 0
        let host_range = Range {
            start: Position {
                line: 3,
                character: 5,
            },
            end: Position {
                line: 5,
                character: 10,
            },
        };
        let request = build_bridge_inlay_hint_request(
            &test_host_uri(),
            host_range,
            "lua",
            "region-0",
            3, // region_start_line
            42,
        );

        let range = &request["params"]["range"];
        assert_eq!(
            range["start"]["line"], 0,
            "Range starting at region_start_line should translate to line 0"
        );
        assert_eq!(
            range["end"]["line"], 2,
            "End line should be translated from 5 to 2 (5-3)"
        );
    }

    // ==========================================================================
    // Document color request tests
    // ==========================================================================

    #[test]
    fn document_color_request_uses_virtual_uri() {
        let request = build_bridge_document_color_request(&test_host_uri(), "lua", "region-0", 42);

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn document_color_request_has_correct_method_and_structure() {
        let request = build_bridge_document_color_request(&test_host_uri(), "lua", "region-0", 123);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 123);
        assert_eq!(request["method"], "textDocument/documentColor");
        // DocumentColor request has no position parameter (whole-document operation)
        assert!(
            request["params"].get("position").is_none(),
            "DocumentColor request should not have position parameter"
        );
    }

    // ==========================================================================
    // Color presentation request tests
    // ==========================================================================

    #[test]
    fn color_presentation_request_uses_virtual_uri() {
        use tower_lsp::lsp_types::Range;

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
        let color = serde_json::json!({
            "red": 1.0,
            "green": 0.0,
            "blue": 0.0,
            "alpha": 1.0
        });
        let request = build_bridge_color_presentation_request(
            &test_host_uri(),
            host_range,
            &color,
            "lua",
            "region-0",
            3, // region_start_line
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn color_presentation_request_transforms_range_to_virtual_coordinates() {
        use tower_lsp::lsp_types::Range;

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
        let color = serde_json::json!({
            "red": 1.0,
            "green": 0.0,
            "blue": 0.0,
            "alpha": 1.0
        });
        let request = build_bridge_color_presentation_request(
            &test_host_uri(),
            host_range,
            &color,
            "lua",
            "region-0",
            3, // region_start_line
            42,
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
        use tower_lsp::lsp_types::Range;

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
        let color = serde_json::json!({
            "red": 0.5,
            "green": 0.25,
            "blue": 0.75,
            "alpha": 1.0
        });
        let request = build_bridge_color_presentation_request(
            &test_host_uri(),
            host_range,
            &color,
            "lua",
            "region-0",
            3, // region_start_line
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/colorPresentation");
        assert_eq!(request["params"]["color"]["red"], 0.5);
        assert_eq!(request["params"]["color"]["green"], 0.25);
        assert_eq!(request["params"]["color"]["blue"], 0.75);
        assert_eq!(request["params"]["color"]["alpha"], 1.0);
    }

    // ==========================================================================
    // Moniker request tests
    // ==========================================================================

    #[test]
    fn moniker_request_uses_virtual_uri() {
        let request = build_bridge_moniker_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_uses_virtual_uri(&request, "lua");
    }

    #[test]
    fn moniker_request_translates_position_to_virtual_coordinates() {
        // Host line 5, region starts at line 3 -> virtual line 2
        let request = build_bridge_moniker_request(
            &test_host_uri(),
            test_position(),
            "lua",
            "region-0",
            3,
            42,
        );

        assert_position_request(&request, "textDocument/moniker", 2);
    }
}
