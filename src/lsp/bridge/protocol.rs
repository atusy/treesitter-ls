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
    /// * `language` - The injection language (e.g., "lua", "python"). Must not be empty.
    /// * `region_id` - Unique identifier for this injection region within the host. Must not be empty.
    ///
    /// # Panics (debug builds only)
    /// Panics if `language` or `region_id` is empty. These are programming errors
    /// as callers should always provide valid identifiers.
    ///
    /// # Upstream Guarantees
    /// In practice, these parameters are guaranteed valid by upstream sources:
    /// - `region_id` comes from ULID generation (26-char alphanumeric strings)
    /// - `language` comes from Tree-sitter injection queries (non-empty language names)
    ///
    /// In release builds, invalid inputs are accepted without validation to avoid
    /// runtime overhead. Unknown languages produce `.txt` extensions as a safe fallback.
    pub(crate) fn new(
        host_uri: &tower_lsp::lsp_types::Url,
        language: &str,
        region_id: &str,
    ) -> Self {
        debug_assert!(!language.is_empty(), "language must not be empty");
        debug_assert!(!region_id.is_empty(), "region_id must not be empty");

        Self {
            host_uri: host_uri.clone(),
            language: language.to_string(),
            region_id: region_id.to_string(),
        }
    }

    /// Get the region_id.
    pub(crate) fn region_id(&self) -> &str {
        &self.region_id
    }

    /// Get the language.
    pub(crate) fn language(&self) -> &str {
        &self.language
    }

    /// Convert to a URI string.
    ///
    /// Format: `file:///.treesitter-ls/{host_path_hash}/{region_id}.{ext}`
    ///
    /// Uses file:// scheme with a virtual path under .treesitter-ls directory.
    /// This format is compatible with most language servers that expect file:// URIs.
    /// The file extension is derived from the language to help downstream language servers
    /// recognize the file type (e.g., lua-language-server needs `.lua` extension).
    ///
    /// The region_id is percent-encoded to ensure URI-safe characters. While ULIDs
    /// only contain alphanumeric characters, this provides defense-in-depth.
    pub(crate) fn to_uri_string(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Hash the host URI to create a unique but deterministic directory name
        let mut hasher = DefaultHasher::new();
        self.host_uri.as_str().hash(&mut hasher);
        let host_hash = hasher.finish();

        // Get file extension for the language
        let extension = Self::language_to_extension(&self.language);

        // Percent-encode region_id to ensure URI-safe characters
        // RFC 3986 unreserved characters: A-Z a-z 0-9 - . _ ~
        let encoded_region_id = Self::percent_encode_path_segment(&self.region_id);

        // Create a file:// URI with a virtual path
        // This allows downstream language servers to recognize the file type by extension
        format!(
            "file:///.treesitter-ls/{:x}/{}.{}",
            host_hash, encoded_region_id, extension
        )
    }

    /// Percent-encode a string for use in a URI path segment.
    ///
    /// Encodes all characters except RFC 3986 unreserved characters:
    /// `A-Z a-z 0-9 - . _ ~`
    ///
    /// Multi-byte UTF-8 characters are encoded byte-by-byte, producing valid
    /// percent-encoded sequences (e.g., "日" → "%E6%97%A5").
    ///
    /// # Note
    /// This function is primarily used for defense-in-depth since region_id values
    /// are ULIDs (alphanumeric only), but it ensures URI safety if the format changes.
    fn percent_encode_path_segment(s: &str) -> String {
        let mut encoded = String::with_capacity(s.len());
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    encoded.push(byte as char);
                }
                _ => {
                    encoded.push_str(&format!("%{:02X}", byte));
                }
            }
        }
        encoded
    }

    /// Map language name to file extension.
    ///
    /// Downstream language servers (e.g., lua-language-server) often use file extension
    /// to determine file type and enable appropriate language features.
    ///
    /// # Returns
    /// The file extension (without leading dot) for the given language.
    /// Returns "txt" for unknown languages as a safe fallback that won't
    /// trigger any language-specific behavior in downstream servers.
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

/// Check if a URI string represents a virtual document.
///
/// Virtual document URIs have the pattern `file:///.treesitter-ls/{hash}/{region_id}.{ext}`.
/// This is used to distinguish virtual URIs from real file URIs in definition responses.
pub(crate) fn is_virtual_uri(uri: &str) -> bool {
    uri.contains("/.treesitter-ls/")
}

/// Context for transforming definition responses to host coordinates.
///
/// Contains information about the original request to enable proper coordinate
/// transformation for responses that may reference different virtual documents.
#[derive(Debug, Clone)]
pub(crate) struct ResponseTransformContext {
    /// The virtual URI string we sent in the request
    pub request_virtual_uri: String,
    /// The host URI string for the request
    pub request_host_uri: String,
    /// The region start line for the request's injection region
    pub request_region_start_line: u32,
}

/// Transform a definition response from virtual to host document coordinates.
///
/// Definition responses can be in multiple formats per LSP spec:
/// - null (no definition found)
/// - Location (single location with uri + range)
/// - Location[] (array of locations)
/// - LocationLink[] (array of location links with target ranges)
///
/// This function handles three cases for each URI in the response:
/// 1. **Real file URI** (not a virtual URI): Preserved as-is with original coordinates
/// 2. **Same virtual URI as request**: Transformed using request's context
/// 3. **Different virtual URI** (cross-region): Filtered out from results
///
/// Cross-region virtual URIs are filtered because we cannot reliably map their
/// coordinates back to the host document (the region_start_line may be stale
/// after host document edits).
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `context` - The transformation context with request information
pub(crate) fn transform_definition_response_to_host(
    mut response: serde_json::Value,
    context: &ResponseTransformContext,
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
        // Filter out cross-region virtual URIs, transform the rest
        arr.retain_mut(|item| transform_definition_item(item, context));
    } else if result.is_object() {
        // Single Location or LocationLink
        if !transform_definition_item(result, context) {
            // Item was filtered - return null result
            response["result"] = serde_json::Value::Null;
        }
    }

    response
}

/// Transform a single Location or LocationLink item to host coordinates.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
///
/// Handles three cases:
/// 1. Real file URI → preserve as-is (cross-file jump to real file) - KEEP
/// 2. Same virtual URI as request → transform using request's context - KEEP
/// 3. Different virtual URI → cross-region jump - FILTER OUT
fn transform_definition_item(
    item: &mut serde_json::Value,
    context: &ResponseTransformContext,
) -> bool {
    // Handle Location format (has uri + range)
    if let Some(uri_str) = item
        .get("uri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        return transform_location_uri(item, &uri_str, "uri", "range", context);
    }

    // Handle LocationLink format (has targetUri + targetRange + targetSelectionRange)
    if let Some(target_uri_str) = item
        .get("targetUri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        return transform_location_link_target(item, &target_uri_str, context);
    }
    // Note: originSelectionRange stays in host coordinates (it's already correct)

    // Unknown format - keep it
    true
}

/// Transform a Location's uri and range based on URI type.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
fn transform_location_uri(
    item: &mut serde_json::Value,
    uri_str: &str,
    uri_field: &str,
    range_field: &str,
    context: &ResponseTransformContext,
) -> bool {
    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(uri_str) {
        return true;
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == context.request_virtual_uri {
        item[uri_field] = serde_json::json!(&context.request_host_uri);
        if let Some(range) = item.get_mut(range_field) {
            transform_range(range, context.request_region_start_line);
        }
        return true;
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    // We cannot reliably transform these because region_start_line may be stale
    false
}

/// Transform a LocationLink's targetUri and associated ranges.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
fn transform_location_link_target(
    item: &mut serde_json::Value,
    target_uri_str: &str,
    context: &ResponseTransformContext,
) -> bool {
    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(target_uri_str) {
        return true;
    }

    // Case 2: Same virtual URI as request → use request's context
    if target_uri_str == context.request_virtual_uri {
        item["targetUri"] = serde_json::json!(&context.request_host_uri);
        if let Some(range) = item.get_mut("targetRange") {
            transform_range(range, context.request_region_start_line);
        }
        if let Some(range) = item.get_mut("targetSelectionRange") {
            transform_range(range, context.request_region_start_line);
        }
        return true;
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    // We cannot reliably transform these because region_start_line may be stale
    false
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

    #[test]
    fn region_id_accessor_returns_stored_value() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "01ARZ3NDEKTSV4RRFFQ69G5FAV");

        assert_eq!(virtual_uri.region_id(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    }

    #[test]
    fn language_accessor_returns_stored_value() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "python", "region-0");

        assert_eq!(virtual_uri.language(), "python");
    }

    #[test]
    fn virtual_uri_percent_encodes_special_characters_in_region_id() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Test with characters that need encoding: space, slash, question mark
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region/0?test");

        let uri_string = virtual_uri.to_uri_string();
        // "/" should be encoded as %2F, "?" should be encoded as %3F
        assert!(
            uri_string.contains("region%2F0%3Ftest"),
            "Special characters should be percent-encoded: {}",
            uri_string
        );
    }

    #[test]
    fn virtual_uri_preserves_alphanumeric_and_safe_chars_in_region_id() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // RFC 3986 unreserved characters: A-Z a-z 0-9 - . _ ~
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "ABC-xyz_123.test~v2");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.contains("ABC-xyz_123.test~v2.lua"),
            "Unreserved characters should not be encoded: {}",
            uri_string
        );
    }

    #[test]
    fn virtual_uri_same_inputs_produce_same_output() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        assert_eq!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Same inputs should produce deterministic output"
        );
    }

    #[test]
    fn virtual_uri_different_region_ids_produce_different_uris() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host_uri, "lua", "region-1");

        assert_ne!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Different region_ids should produce different URIs"
        );
    }

    #[test]
    fn virtual_uri_different_languages_produce_different_extensions() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let lua_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let python_uri = VirtualDocumentUri::new(&host_uri, "python", "region-0");

        assert!(lua_uri.to_uri_string().ends_with(".lua"));
        assert!(python_uri.to_uri_string().ends_with(".py"));
    }

    #[test]
    fn language_to_extension_maps_common_languages() {
        // Test a representative sample of the supported languages
        let test_cases = [
            ("lua", "lua"),
            ("python", "py"),
            ("rust", "rs"),
            ("javascript", "js"),
            ("typescript", "ts"),
            ("go", "go"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("java", "java"),
            ("ruby", "rb"),
            ("bash", "sh"),
            ("sh", "sh"),
        ];

        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        for (language, expected_ext) in test_cases {
            let uri = VirtualDocumentUri::new(&host_uri, language, "region-0");
            let uri_string = uri.to_uri_string();
            assert!(
                uri_string.ends_with(&format!(".{}", expected_ext)),
                "Language '{}' should produce extension '{}', got: {}",
                language,
                expected_ext,
                uri_string
            );
        }
    }

    #[test]
    fn language_to_extension_falls_back_to_txt_for_unknown() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Unknown languages should default to .txt
        let unknown_cases = ["unknown-lang", "foobar", "notareallan"];

        for language in unknown_cases {
            let uri = VirtualDocumentUri::new(&host_uri, language, "region-0");
            let uri_string = uri.to_uri_string();
            assert!(
                uri_string.ends_with(".txt"),
                "Unknown language '{}' should produce .txt extension, got: {}",
                language,
                uri_string
            );
        }
    }

    #[test]
    fn virtual_uri_different_hosts_produce_different_hashes() {
        let host1 = Url::parse("file:///project/doc1.md").unwrap();
        let host2 = Url::parse("file:///project/doc2.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&host1, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host2, "lua", "region-0");

        assert_ne!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Different host URIs should produce different hashes"
        );
    }

    #[test]
    fn virtual_uri_equality_checks_all_fields() {
        let host1 = Url::parse("file:///project/doc1.md").unwrap();
        let host2 = Url::parse("file:///project/doc2.md").unwrap();

        let uri1 = VirtualDocumentUri::new(&host1, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host1, "lua", "region-0");
        let uri3 = VirtualDocumentUri::new(&host2, "lua", "region-0");
        let uri4 = VirtualDocumentUri::new(&host1, "python", "region-0");
        let uri5 = VirtualDocumentUri::new(&host1, "lua", "region-1");

        assert_eq!(uri1, uri2, "Same fields should be equal");
        assert_ne!(uri1, uri3, "Different host_uri should not be equal");
        assert_ne!(uri1, uri4, "Different language should not be equal");
        assert_ne!(uri1, uri5, "Different region_id should not be equal");
    }

    #[test]
    #[should_panic(expected = "language must not be empty")]
    #[cfg(debug_assertions)]
    fn virtual_uri_panics_on_empty_language_in_debug() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let _ = VirtualDocumentUri::new(&host_uri, "", "region-0");
    }

    #[test]
    #[should_panic(expected = "region_id must not be empty")]
    #[cfg(debug_assertions)]
    fn virtual_uri_panics_on_empty_region_id_in_debug() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let _ = VirtualDocumentUri::new(&host_uri, "lua", "");
    }

    #[test]
    fn percent_encode_preserves_unreserved_characters() {
        // RFC 3986 unreserved: ALPHA / DIGIT / "-" / "." / "_" / "~"
        let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, input,
            "Unreserved characters should not be encoded"
        );
    }

    #[test]
    fn percent_encode_encodes_reserved_characters() {
        // Some reserved characters that need encoding in path segments
        let input = "test/path?query#fragment";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "test%2Fpath%3Fquery%23fragment",
            "Reserved characters should be percent-encoded"
        );
    }

    #[test]
    fn percent_encode_encodes_space() {
        let input = "hello world";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(encoded, "hello%20world", "Space should be encoded as %20");
    }

    #[test]
    fn percent_encode_handles_multibyte_utf8() {
        // UTF-8 multi-byte characters should have each byte percent-encoded
        // "日" (U+65E5) = E6 97 A5 in UTF-8
        let input = "日";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "%E6%97%A5",
            "Multi-byte UTF-8 should encode each byte"
        );
    }

    #[test]
    fn percent_encode_handles_mixed_ascii_and_utf8() {
        // Mix of ASCII alphanumerics (preserved) and UTF-8 (encoded)
        let input = "region-日本語-test";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        // "日" = E6 97 A5, "本" = E6 9C AC, "語" = E8 AA 9E
        assert_eq!(
            encoded, "region-%E6%97%A5%E6%9C%AC%E8%AA%9E-test",
            "Mixed content should preserve ASCII and encode UTF-8"
        );
    }

    #[test]
    fn percent_encode_handles_emoji() {
        // Emoji are 4-byte UTF-8 sequences
        // "🦀" (U+1F980) = F0 9F A6 80 in UTF-8
        let input = "rust🦀";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "rust%F0%9F%A6%80",
            "4-byte UTF-8 (emoji) should encode all bytes"
        );
    }

    #[test]
    fn to_uri_string_contains_region_id_in_filename() {
        // Verify that the region_id appears in the URI (partial round-trip)
        // Note: Full round-trip isn't possible since host_uri is hashed
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let region_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", region_id);

        let uri_string = virtual_uri.to_uri_string();

        // Extract filename from the URI path
        let filename = uri_string.rsplit('/').next().unwrap();
        // Remove extension to get the region_id
        let extracted_id = filename.rsplit_once('.').map(|(name, _)| name).unwrap();

        assert_eq!(
            extracted_id, region_id,
            "Region ID should be extractable from URI"
        );
    }

    #[test]
    fn to_uri_string_produces_valid_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();

        // Verify the output is a valid URL
        let parsed = Url::parse(&uri_string);
        assert!(
            parsed.is_ok(),
            "to_uri_string() should produce a valid URL: {}",
            uri_string
        );

        let parsed = parsed.unwrap();
        assert_eq!(parsed.scheme(), "file");
        assert!(parsed.path().starts_with("/.treesitter-ls/"));
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

    #[test]
    fn response_transformation_with_zero_region_start() {
        // Response transformation when region starts at line 0
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": { "kind": "markdown", "value": "docs" },
                "range": {
                    "start": { "line": 2, "character": 0 },
                    "end": { "line": 2, "character": 10 }
                }
            }
        });
        let region_start_line = 0;

        let transformed = transform_hover_response_to_host(response, region_start_line);

        assert_eq!(
            transformed["result"]["range"]["start"]["line"], 2,
            "With region_start_line=0, host line equals virtual line"
        );
    }

    #[test]
    fn response_transformation_at_line_zero() {
        // Virtual document line 0 should map to region_start_line
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": { "kind": "markdown", "value": "docs" },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 5 }
                }
            }
        });
        let region_start_line = 10;

        let transformed = transform_hover_response_to_host(response, region_start_line);

        assert_eq!(
            transformed["result"]["range"]["start"]["line"], 10,
            "Virtual line 0 should map to region_start_line"
        );
        assert_eq!(
            transformed["result"]["range"]["end"]["line"], 10,
            "Virtual line 0 should map to region_start_line"
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

    /// Helper to create a ResponseTransformContext for tests.
    fn test_context(
        virtual_uri: &str,
        host_uri: &str,
        region_start_line: u32,
    ) -> ResponseTransformContext {
        ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: region_start_line,
        }
    }

    #[test]
    fn definition_response_transforms_location_array_ranges() {
        // Definition response as Location[] format
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 14 }
                    }
                },
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 2, "character": 10 }
                    }
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

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
        // URI transformed to host
        assert_eq!(result[0]["uri"], host_uri);
        assert_eq!(result[1]["uri"], host_uri);
    }

    #[test]
    fn definition_response_transforms_single_location() {
        // Definition response as single Location (not array)
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": virtual_uri,
                "range": {
                    "start": { "line": 1, "character": 5 },
                    "end": { "line": 1, "character": 15 }
                }
            }
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        // Single location: line 1 -> 4
        assert_eq!(transformed["result"]["range"]["start"]["line"], 4);
        assert_eq!(transformed["result"]["range"]["end"]["line"], 4);
        // Characters unchanged
        assert_eq!(transformed["result"]["range"]["start"]["character"], 5);
        assert_eq!(transformed["result"]["range"]["end"]["character"], 15);
        // URI transformed to host
        assert_eq!(transformed["result"]["uri"], host_uri);
    }

    #[test]
    fn definition_response_transforms_location_link_array() {
        // Definition response as LocationLink[] format
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "originSelectionRange": {
                        "start": { "line": 5, "character": 0 },
                        "end": { "line": 5, "character": 10 }
                    },
                    "targetUri": virtual_uri,
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
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

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
        // targetUri transformed to host
        assert_eq!(result[0]["targetUri"], host_uri);
    }

    #[test]
    fn definition_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);

        let transformed = transform_definition_response_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    #[test]
    fn definition_response_transforms_location_uri_to_host_uri() {
        // Definition response with virtual URI should be transformed to host URI
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 14 }
                    }
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        // URI should be transformed to host URI
        assert_eq!(
            result[0]["uri"], host_uri,
            "Location.uri should be transformed to host URI"
        );
        // Range transformation still works
        assert_eq!(result[0]["range"]["start"]["line"], 3);
    }

    #[test]
    fn definition_response_transforms_location_link_target_uri_to_host_uri() {
        // Definition response as LocationLink[] with virtual targetUri should be transformed
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "originSelectionRange": {
                        "start": { "line": 5, "character": 0 },
                        "end": { "line": 5, "character": 10 }
                    },
                    "targetUri": virtual_uri,
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
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        // targetUri should be transformed to host URI
        assert_eq!(
            result[0]["targetUri"], host_uri,
            "LocationLink.targetUri should be transformed to host URI"
        );
        // Range transformations still work
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetSelectionRange"]["start"]["line"], 3);
    }

    // ==========================================================================
    // New cross-document transformation tests
    // ==========================================================================

    #[test]
    fn is_virtual_uri_detects_virtual_uris() {
        assert!(is_virtual_uri("file:///.treesitter-ls/abc123/region-0.lua"));
        assert!(is_virtual_uri(
            "file:///.treesitter-ls/def456/01JPMQ8ZYYQA.py"
        ));
        assert!(is_virtual_uri("file:///.treesitter-ls/hash/test.txt"));
    }

    #[test]
    fn is_virtual_uri_rejects_real_uris() {
        assert!(!is_virtual_uri("file:///home/user/project/main.lua"));
        assert!(!is_virtual_uri("file:///C:/Users/dev/code.py"));
        assert!(!is_virtual_uri("untitled:Untitled-1"));
        assert!(!is_virtual_uri("file:///some/treesitter-ls/file.lua")); // No dot prefix
    }

    #[test]
    fn definition_response_preserves_real_file_uri() {
        // Response with a real file URI should be preserved as-is
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let real_file_uri = "file:///real/path/utils.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "uri": real_file_uri,
                "range": { "start": { "line": 10, "character": 0 }, "end": { "line": 10, "character": 5 } }
            }]
        });

        let host_uri = "file:///doc.md";
        let context = test_context(virtual_uri, host_uri, 5);
        let transformed = transform_definition_response_to_host(response, &context);

        // Real file URI should be preserved
        assert_eq!(transformed["result"][0]["uri"], real_file_uri);
        // Range should be unchanged (real file coordinates)
        assert_eq!(transformed["result"][0]["range"]["start"]["line"], 10);
    }

    #[test]
    fn definition_response_filters_out_different_region_virtual_uri() {
        // Response with a different virtual URI should be filtered out
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let different_virtual_uri = "file:///.treesitter-ls/abc/region-1.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "uri": different_virtual_uri,
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } }
            }]
        });

        let host_uri = "file:///doc.md";
        let context = test_context(request_virtual_uri, host_uri, 5);

        let transformed = transform_definition_response_to_host(response, &context);

        // Cross-region virtual URI should be filtered out, leaving empty result
        let result = transformed["result"].as_array().unwrap();
        assert!(
            result.is_empty(),
            "Cross-region virtual URI should be filtered out"
        );
    }

    // ==========================================================================
    // TypeDefinition request/response transformation tests
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
    // Implementation request/response transformation tests
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
    // Declaration request/response transformation tests
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
    // References request/response transformation tests
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
}
