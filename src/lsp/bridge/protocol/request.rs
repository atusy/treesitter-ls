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
    // Hover request tests
    // ==========================================================================

    #[test]
    fn hover_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_hover_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn hover_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/hover");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn position_translation_at_region_start_becomes_line_zero() {
        // When cursor is at the first line of the region, virtual line should be 0
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 3, // Same as region_start_line
            character: 5,
        };
        let region_start_line = 3;

        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(
            request["params"]["position"]["line"], 0,
            "Position at region start should translate to line 0"
        );
    }

    #[test]
    fn position_translation_with_zero_region_start() {
        // Region starting at line 0 (e.g., first line of document)
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 0,
        };
        let region_start_line = 0;

        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

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
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let notification =
            build_bridge_didchange_notification(&host_uri, "lua", "region-0", "local x = 42", 2);

        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "textDocument/didChange");
        assert!(
            notification.get("id").is_none(),
            "Notification should not have id"
        );

        let uri_str = notification["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "didChange should use virtual URI: {}",
            uri_str
        );
        assert_eq!(notification["params"]["textDocument"]["version"], 2);
    }

    #[test]
    fn didchange_notification_contains_full_text() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let content = "local x = 42\nprint(x)";
        let notification =
            build_bridge_didchange_notification(&host_uri, "lua", "region-0", content, 1);

        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["text"], content);
    }

    // ==========================================================================
    // Completion request tests
    // ==========================================================================

    #[test]
    fn completion_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_completion_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn completion_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_completion_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/completion");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // SignatureHelp request tests
    // ==========================================================================

    #[test]
    fn signature_help_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_signature_help_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn signature_help_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_signature_help_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/signatureHelp");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
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
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_definition_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn definition_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_definition_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/definition");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // TypeDefinition request tests
    // ==========================================================================

    #[test]
    fn type_definition_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_type_definition_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn type_definition_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_type_definition_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/typeDefinition");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // Implementation request tests
    // ==========================================================================

    #[test]
    fn implementation_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_implementation_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn implementation_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_implementation_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/implementation");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // Declaration request tests
    // ==========================================================================

    #[test]
    fn declaration_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_declaration_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn declaration_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_declaration_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/declaration");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // References request tests
    // ==========================================================================

    #[test]
    fn references_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            true, // include_declaration
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn references_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            true, // include_declaration
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/references");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn references_request_includes_context_with_include_declaration_true() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
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
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
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
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_document_highlight_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn document_highlight_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_document_highlight_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/documentHighlight");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // Rename request tests
    // ==========================================================================

    #[test]
    fn rename_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_rename_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            "newName",
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn rename_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_rename_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            "newName",
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/rename");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn rename_request_includes_new_name() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_rename_request(
            &host_uri,
            host_position,
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
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let request = build_bridge_document_link_request(&host_uri, "lua", "region-0", 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn document_link_request_has_correct_method_and_structure() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let request = build_bridge_document_link_request(&host_uri, "lua", "region-0", 123);

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
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let lua_request = build_bridge_document_link_request(&host_uri, "lua", "region-0", 1);
        let python_request = build_bridge_document_link_request(&host_uri, "python", "region-0", 1);

        let lua_uri = lua_request["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();
        let python_uri = python_request["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();

        assert!(lua_uri.ends_with(".lua"));
        assert!(python_uri.ends_with(".py"));
    }
}
