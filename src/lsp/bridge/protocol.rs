//! LSP protocol types and transformations for bridge communication.
//!
//! This module provides types for virtual document URIs and message
//! transformation between host and virtual document coordinates.

/// Virtual document URI for injection regions.
///
/// Encodes host URI + injection language + region ID into a file:// URI
/// that downstream language servers can use to identify virtual documents.
///
/// Format: `file:///.treesitter-ls/{host_hash}/{region_id}.{ext}`
///
/// Example: `file:///.treesitter-ls/a1b2c3d4e5f6/region-0.lua`
///
/// The file:// scheme is used for compatibility with language servers that
/// only support file:// URIs (e.g., lua-language-server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtualDocumentUri {
    host_uri: tower_lsp::lsp_types::Url,
    language: String,
    region_id: String,
}

impl VirtualDocumentUri {
    /// Create a new virtual document URI for an injection region.
    ///
    /// # Arguments
    /// * `host_uri` - The URI of the host document (e.g., markdown file)
    /// * `language` - The injection language (e.g., "lua", "python")
    /// * `region_id` - Unique identifier for this injection region within the host
    pub(crate) fn new(
        host_uri: &tower_lsp::lsp_types::Url,
        language: &str,
        region_id: &str,
    ) -> Self {
        Self {
            host_uri: host_uri.clone(),
            language: language.to_string(),
            region_id: region_id.to_string(),
        }
    }

    /// Convert to a URI string.
    ///
    /// Format: `file:///.treesitter-ls/{host_path_hash}/{region_id}.{ext}`
    ///
    /// Uses file:// scheme with a virtual path under .treesitter-ls directory.
    /// This format is compatible with most language servers that expect file:// URIs.
    /// The file extension is derived from the language to help downstream language servers
    /// recognize the file type (e.g., lua-language-server needs `.lua` extension).
    pub(crate) fn to_uri_string(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Hash the host URI to create a unique but deterministic directory name
        let mut hasher = DefaultHasher::new();
        self.host_uri.as_str().hash(&mut hasher);
        let host_hash = hasher.finish();

        // Get file extension for the language
        let extension = Self::language_to_extension(&self.language);

        // Create a file:// URI with a virtual path
        // This allows downstream language servers to recognize the file type by extension
        format!(
            "file:///.treesitter-ls/{:x}/{}.{}",
            host_hash, self.region_id, extension
        )
    }

    /// Map language name to file extension.
    ///
    /// Downstream language servers often use file extension to determine file type.
    fn language_to_extension(language: &str) -> &'static str {
        match language {
            "lua" => "lua",
            "python" => "py",
            "rust" => "rs",
            "javascript" => "js",
            "typescript" => "ts",
            "go" => "go",
            "c" => "c",
            "cpp" => "cpp",
            "java" => "java",
            "ruby" => "rb",
            "php" => "php",
            "swift" => "swift",
            "kotlin" => "kt",
            "scala" => "scala",
            "haskell" => "hs",
            "ocaml" => "ml",
            "elixir" => "ex",
            "erlang" => "erl",
            "clojure" => "clj",
            "r" => "r",
            "julia" => "jl",
            "sql" => "sql",
            "html" => "html",
            "css" => "css",
            "json" => "json",
            "yaml" => "yaml",
            "toml" => "toml",
            "xml" => "xml",
            "markdown" => "md",
            "bash" | "sh" => "sh",
            "powershell" => "ps1",
            _ => "txt", // Default fallback
        }
    }
}

/// Build a JSON-RPC hover request for a downstream language server.
///
/// Transforms the host document position and URI to virtual document coordinates
/// for the injection region.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_position` - The position in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_hover_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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

/// Build a JSON-RPC signature help request for a downstream language server.
///
/// Transforms the host document position and URI to virtual document coordinates
/// for the injection region.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_position` - The position in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_signature_help_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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
        "method": "textDocument/signatureHelp",
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

/// Build a JSON-RPC completion request for a downstream language server.
///
/// Transforms the host document position and URI to virtual document coordinates
/// for the injection region.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_position` - The position in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_completion_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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
        "method": "textDocument/completion",
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

/// Build a JSON-RPC definition request for a downstream language server.
///
/// Transforms the host document position and URI to virtual document coordinates
/// for the injection region.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `host_position` - The position in the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `region_start_line` - The starting line of the injection region in the host document
/// * `request_id` - The JSON-RPC request ID
pub(crate) fn build_bridge_definition_request(
    host_uri: &tower_lsp::lsp_types::Url,
    host_position: tower_lsp::lsp_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: i64,
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
        "method": "textDocument/definition",
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

/// Build a JSON-RPC didChange notification for a downstream language server.
///
/// Uses full text sync (TextDocumentSyncKind::Full) which sends the entire
/// document content on each change. This is simpler and sufficient for bridge use.
///
/// # Arguments
/// * `host_uri` - The URI of the host document
/// * `injection_language` - The injection language (e.g., "lua")
/// * `region_id` - The unique region ID for this injection
/// * `new_content` - The new content of the virtual document
/// * `version` - The document version number
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

/// Transform a hover response from virtual to host document coordinates.
///
/// If the response contains a range, translates the line numbers from virtual
/// document coordinates back to host document coordinates by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_hover_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Check if response has a result with a range
    if let Some(result) = response.get_mut("result")
        && result.is_object()
        && let Some(range) = result.get_mut("range")
        && range.is_object()
    {
        // Transform start position
        if let Some(start) = range.get_mut("start")
            && let Some(line) = start.get_mut("line")
            && let Some(line_num) = line.as_u64()
        {
            *line = serde_json::json!(line_num + region_start_line as u64);
        }

        // Transform end position
        if let Some(end) = range.get_mut("end")
            && let Some(line) = end.get_mut("line")
            && let Some(line_num) = line.as_u64()
        {
            *line = serde_json::json!(line_num + region_start_line as u64);
        }
    }

    response
}

/// Transform a completion response from virtual to host document coordinates.
///
/// If completion items contain textEdit ranges, translates the line numbers from virtual
/// document coordinates back to host document coordinates by adding region_start_line.
/// Handles both CompletionList format (with items array) and direct array format.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_completion_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // Determine the items to transform
    // CompletionList: { items: [...] } or direct array: [...]
    let items = if result.is_array() {
        result.as_array_mut()
    } else if result.is_object() {
        result.get_mut("items").and_then(|i| i.as_array_mut())
    } else {
        None
    };

    if let Some(items) = items {
        for item in items.iter_mut() {
            transform_completion_item_range(item, region_start_line);
        }
    }

    response
}

/// Transform the textEdit range in a single completion item to host coordinates.
fn transform_completion_item_range(item: &mut serde_json::Value, region_start_line: u32) {
    // Check for textEdit field
    if let Some(text_edit) = item.get_mut("textEdit")
        && let Some(range) = text_edit.get_mut("range")
        && range.is_object()
    {
        transform_range(range, region_start_line);
    }

    // Also check for additionalTextEdits (array of TextEdit)
    if let Some(additional) = item.get_mut("additionalTextEdits")
        && let Some(additional_arr) = additional.as_array_mut()
    {
        for edit in additional_arr.iter_mut() {
            if let Some(range) = edit.get_mut("range")
                && range.is_object()
            {
                transform_range(range, region_start_line);
            }
        }
    }
}

/// Transform a range's line numbers from virtual to host coordinates.
fn transform_range(range: &mut serde_json::Value, region_start_line: u32) {
    // Transform start position
    if let Some(start) = range.get_mut("start")
        && let Some(line) = start.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num + region_start_line as u64);
    }

    // Transform end position
    if let Some(end) = range.get_mut("end")
        && let Some(line) = end.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num + region_start_line as u64);
    }
}

/// Transform a signature help response from virtual to host document coordinates.
///
/// SignatureHelp responses don't contain ranges that need transformation.
/// This function passes through the response unchanged, preserving:
/// - signatures array with label, documentation, and parameters
/// - activeSignature index
/// - activeParameter index
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `_region_start_line` - The starting line (unused for signature help, kept for API consistency)
pub(crate) fn transform_signature_help_response_to_host(
    response: serde_json::Value,
    _region_start_line: u32,
) -> serde_json::Value {
    // SignatureHelp doesn't have ranges that need transformation.
    // activeSignature and activeParameter are indices, not coordinates.
    // Pass through unchanged.
    response
}

/// Transform a definition response from virtual to host document coordinates.
///
/// Definition responses can be in multiple formats per LSP spec:
/// - null (no definition found)
/// - Location (single location with uri + range)
/// - Location[] (array of locations)
/// - LocationLink[] (array of location links with target ranges)
///
/// This function transforms all range line numbers from virtual document
/// coordinates back to host document coordinates by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_definition_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // Array format: Location[] or LocationLink[]
    if let Some(arr) = result.as_array_mut() {
        for item in arr.iter_mut() {
            transform_definition_item(item, region_start_line);
        }
    } else if result.is_object() {
        // Single Location or LocationLink
        transform_definition_item(result, region_start_line);
    }

    response
}

/// Transform a single Location or LocationLink item to host coordinates.
fn transform_definition_item(item: &mut serde_json::Value, region_start_line: u32) {
    // Check if this is a Location (has uri + range)
    if let Some(range) = item.get_mut("range") {
        transform_range(range, region_start_line);
    }

    // Check if this is a LocationLink (has targetUri + targetRange + targetSelectionRange)
    if let Some(target_range) = item.get_mut("targetRange") {
        transform_range(target_range, region_start_line);
    }
    if let Some(target_selection_range) = item.get_mut("targetSelectionRange") {
        transform_range(target_selection_range, region_start_line);
    }
    // Note: originSelectionRange stays in host coordinates (it's already correct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tower_lsp::lsp_types::{Position, Url};

    // ==========================================================================
    // VirtualDocumentUri tests
    // ==========================================================================

    #[test]
    fn virtual_uri_uses_treesitter_ls_path_prefix() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.starts_with("file:///.treesitter-ls/"),
            "URI should use file:///.treesitter-ls/ path: {}",
            uri_string
        );
    }

    #[test]
    fn virtual_uri_includes_language_extension() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.ends_with(".lua"),
            "URI should have .lua extension: {}",
            uri_string
        );
    }

    // ==========================================================================
    // Hover request/response transformation tests
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
    fn hover_response_transforms_range_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": { "kind": "markdown", "value": "docs" },
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }
        });
        let region_start_line = 3;

        let transformed = transform_hover_response_to_host(response, region_start_line);

        assert_eq!(
            transformed["result"]["range"]["start"]["line"], 3,
            "Start line should be translated (0 + 3 = 3)"
        );
        assert_eq!(
            transformed["result"]["range"]["end"]["line"], 3,
            "End line should be translated (0 + 3 = 3)"
        );
        // Characters unchanged
        assert_eq!(transformed["result"]["range"]["start"]["character"], 9);
        assert_eq!(transformed["result"]["range"]["end"]["character"], 14);
    }

    #[test]
    fn hover_response_without_range_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": { "contents": "Simple hover text" }
        });

        let transformed = transform_hover_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn hover_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_hover_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
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
    // Completion request/response transformation tests
    // ==========================================================================

    #[test]
    fn completion_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 6,
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
        let host_position = Position {
            line: 5,
            character: 6,
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
            request["params"]["position"]["character"], 6,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn completion_response_transforms_textedit_ranges() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [
                    {
                        "label": "print",
                        "kind": 3,
                        "textEdit": {
                            "range": {
                                "start": { "line": 1, "character": 0 },
                                "end": { "line": 1, "character": 3 }
                            },
                            "newText": "print"
                        }
                    },
                    { "label": "pairs", "kind": 3 }
                ]
            }
        });
        let region_start_line = 3;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        let items = transformed["result"]["items"].as_array().unwrap();
        // Item with textEdit has transformed range
        assert_eq!(items[0]["textEdit"]["range"]["start"]["line"], 4);
        assert_eq!(items[0]["textEdit"]["range"]["end"]["line"], 4);
        // Item without textEdit unchanged
        assert_eq!(items[1]["label"], "pairs");
        assert!(items[1].get("textEdit").is_none());
    }

    #[test]
    fn completion_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_completion_response_to_host(response.clone(), 3);
        assert_eq!(transformed, response);
    }

    #[test]
    fn completion_response_handles_array_format() {
        // Some servers return array directly instead of CompletionList
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "label": "print",
                "textEdit": {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "print"
                }
            }]
        });
        let region_start_line = 5;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        let items = transformed["result"].as_array().unwrap();
        assert_eq!(items[0]["textEdit"]["range"]["start"]["line"], 5);
        assert_eq!(items[0]["textEdit"]["range"]["end"]["line"], 5);
    }

    // ==========================================================================
    // SignatureHelp request/response transformation tests
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

    #[test]
    fn signature_help_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_signature_help_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn signature_help_response_preserves_active_parameter_and_signature() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "signatures": [
                    {
                        "label": "string.format(formatstring, ...)",
                        "documentation": "Formats a string",
                        "parameters": [
                            { "label": "formatstring" },
                            { "label": "..." }
                        ]
                    }
                ],
                "activeSignature": 0,
                "activeParameter": 1
            }
        });
        let region_start_line = 3;

        let transformed =
            transform_signature_help_response_to_host(response.clone(), region_start_line);

        // activeSignature and activeParameter must be preserved unchanged
        assert_eq!(
            transformed["result"]["activeSignature"], 0,
            "activeSignature must be preserved"
        );
        assert_eq!(
            transformed["result"]["activeParameter"], 1,
            "activeParameter must be preserved"
        );
        // signatures array must be preserved
        assert_eq!(
            transformed["result"]["signatures"][0]["label"],
            "string.format(formatstring, ...)"
        );
    }

    #[test]
    fn signature_help_response_without_metadata_passes_through() {
        // Some servers may return minimal response without activeSignature/activeParameter
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "signatures": [
                    { "label": "print(...)" }
                ]
            }
        });

        let transformed = transform_signature_help_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    // ==========================================================================
    // Definition request/response transformation tests
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

    #[test]
    fn definition_response_transforms_location_array_ranges() {
        // Definition response as Location[] format
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": "file:///.treesitter-ls/abc123/region-0.lua",
                    "range": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 14 }
                    }
                },
                {
                    "uri": "file:///.treesitter-ls/abc123/region-0.lua",
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 2, "character": 10 }
                    }
                }
            ]
        });
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        // First location: line 0 -> 3
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["range"]["end"]["line"], 3);
        // Second location: line 2 -> 5
        assert_eq!(result[1]["range"]["start"]["line"], 5);
        assert_eq!(result[1]["range"]["end"]["line"], 5);
        // Characters unchanged
        assert_eq!(result[0]["range"]["start"]["character"], 9);
        assert_eq!(result[0]["range"]["end"]["character"], 14);
    }

    #[test]
    fn definition_response_transforms_single_location() {
        // Definition response as single Location (not array)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": "file:///.treesitter-ls/abc123/region-0.lua",
                "range": {
                    "start": { "line": 1, "character": 5 },
                    "end": { "line": 1, "character": 15 }
                }
            }
        });
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(response, region_start_line);

        // Single location: line 1 -> 4
        assert_eq!(transformed["result"]["range"]["start"]["line"], 4);
        assert_eq!(transformed["result"]["range"]["end"]["line"], 4);
        // Characters unchanged
        assert_eq!(transformed["result"]["range"]["start"]["character"], 5);
        assert_eq!(transformed["result"]["range"]["end"]["character"], 15);
    }

    #[test]
    fn definition_response_transforms_location_link_array() {
        // Definition response as LocationLink[] format
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "originSelectionRange": {
                        "start": { "line": 5, "character": 0 },
                        "end": { "line": 5, "character": 10 }
                    },
                    "targetUri": "file:///.treesitter-ls/abc123/region-0.lua",
                    "targetRange": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 2, "character": 3 }
                    },
                    "targetSelectionRange": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 14 }
                    }
                }
            ]
        });
        let region_start_line = 3;

        let transformed = transform_definition_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        // originSelectionRange should NOT be transformed (it's in host coordinates)
        assert_eq!(result[0]["originSelectionRange"]["start"]["line"], 5);
        assert_eq!(result[0]["originSelectionRange"]["end"]["line"], 5);
        // targetRange should be transformed: line 0 -> 3, line 2 -> 5
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetRange"]["end"]["line"], 5);
        // targetSelectionRange should be transformed: line 0 -> 3
        assert_eq!(result[0]["targetSelectionRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetSelectionRange"]["end"]["line"], 3);
        // Characters unchanged
        assert_eq!(result[0]["targetSelectionRange"]["start"]["character"], 9);
        assert_eq!(result[0]["targetSelectionRange"]["end"]["character"], 14);
    }

    #[test]
    fn definition_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_definition_response_to_host(response.clone(), 3);
        assert_eq!(transformed, response);
    }
}
