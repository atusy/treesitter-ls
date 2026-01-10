//! LSP protocol types and transformations for bridge communication.
//!
//! This module provides types for virtual document URIs, request tracking,
//! and message transformation between host and virtual document coordinates.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::oneshot;

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

    /// Parse a virtual document URI from a URI string.
    ///
    /// Returns None if the URI is not a valid treesitter-ls virtual URI.
    /// Format: `file:///.treesitter-ls/{host_hash}/{region_id}.{ext}`
    ///
    /// Note: This method cannot fully reconstruct the original host_uri since only
    /// the hash is stored. It returns a placeholder host_uri for testing purposes.
    #[allow(dead_code)]
    pub(crate) fn parse(uri_str: &str) -> Option<Self> {
        use tower_lsp::lsp_types::Url;

        let url = Url::parse(uri_str).ok()?;

        // Check scheme and path prefix
        if url.scheme() != "file" {
            return None;
        }

        let path = url.path();
        if !path.starts_with("/.treesitter-ls/") {
            return None;
        }

        // Parse path: /.treesitter-ls/{host_hash}/{region_id}.{ext}
        let path_without_prefix = path.strip_prefix("/.treesitter-ls/")?;
        let (_host_hash, filename) = path_without_prefix.split_once('/')?;

        // Extract region_id and extension from filename
        let (region_id, ext) = if let Some(dot_pos) = filename.rfind('.') {
            (&filename[..dot_pos], &filename[dot_pos + 1..])
        } else {
            return None;
        };

        // Infer language from extension
        let language = Self::extension_to_language(ext)?.to_string();

        // Create a placeholder host_uri since we can't recover it from the hash
        let host_uri = Url::parse("file:///unknown").ok()?;

        Some(Self {
            host_uri,
            language,
            region_id: region_id.to_string(),
        })
    }

    /// Map file extension back to language name.
    fn extension_to_language(ext: &str) -> Option<&'static str> {
        match ext {
            "lua" => Some("lua"),
            "py" => Some("python"),
            "rs" => Some("rust"),
            "js" => Some("javascript"),
            "ts" => Some("typescript"),
            "go" => Some("go"),
            "c" => Some("c"),
            "cpp" => Some("cpp"),
            "java" => Some("java"),
            "rb" => Some("ruby"),
            "php" => Some("php"),
            "swift" => Some("swift"),
            "kt" => Some("kotlin"),
            "scala" => Some("scala"),
            "hs" => Some("haskell"),
            "ml" => Some("ocaml"),
            "ex" => Some("elixir"),
            "erl" => Some("erlang"),
            "clj" => Some("clojure"),
            "r" => Some("r"),
            "jl" => Some("julia"),
            "sql" => Some("sql"),
            "html" => Some("html"),
            "css" => Some("css"),
            "json" => Some("json"),
            "yaml" => Some("yaml"),
            "toml" => Some("toml"),
            "xml" => Some("xml"),
            "md" => Some("markdown"),
            "sh" => Some("bash"),
            "ps1" => Some("powershell"),
            "txt" => None, // Default extension, language unknown
            _ => None,
        }
    }

    /// Get the host document URI.
    #[allow(dead_code)]
    pub(crate) fn host_uri(&self) -> &tower_lsp::lsp_types::Url {
        &self.host_uri
    }

    /// Get the injection language.
    #[allow(dead_code)]
    pub(crate) fn language(&self) -> &str {
        &self.language
    }

    /// Get the region ID.
    #[allow(dead_code)]
    pub(crate) fn region_id(&self) -> &str {
        &self.region_id
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
#[allow(dead_code)]
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

/// Request ID type for JSON-RPC messages.
///
/// LSP spec allows either integer or string IDs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum RequestId {
    Int(i64),
    String(String),
}

impl RequestId {
    /// Extract request ID from a JSON-RPC message.
    pub(crate) fn from_json(value: &serde_json::Value) -> Option<Self> {
        match &value["id"] {
            serde_json::Value::Number(n) => n.as_i64().map(RequestId::Int),
            serde_json::Value::String(s) => Some(RequestId::String(s.clone())),
            _ => None,
        }
    }
}

/// Tracks pending requests waiting for responses.
///
/// Uses `DashMap` for concurrent access from writer and reader tasks.
/// Each pending request is associated with a `oneshot::Sender` to deliver
/// the response back to the caller.
#[derive(Clone)]
pub(crate) struct PendingRequests {
    inner: Arc<DashMap<RequestId, oneshot::Sender<serde_json::Value>>>,
}

impl PendingRequests {
    /// Create a new pending request tracker.
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Register a pending request and return a receiver for the response.
    ///
    /// Returns a tuple of (response_receiver, request_id).
    #[allow(dead_code)]
    pub(crate) fn register(&self, id: i64) -> (oneshot::Receiver<serde_json::Value>, RequestId) {
        let request_id = RequestId::Int(id);
        let (tx, rx) = oneshot::channel();
        self.inner.insert(request_id.clone(), tx);
        (rx, request_id)
    }

    /// Complete a pending request by routing the response to its sender.
    ///
    /// Extracts the request ID from the response and sends it to the
    /// corresponding pending request, if one exists.
    #[allow(dead_code)]
    pub(crate) fn complete(&self, response: &serde_json::Value) {
        if let Some(id) = RequestId::from_json(response)
            && let Some((_, sender)) = self.inner.remove(&id)
        {
            // Ignore send error - receiver may have been dropped
            let _ = sender.send(response.clone());
        }
    }
}
