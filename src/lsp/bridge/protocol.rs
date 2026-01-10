//! LSP protocol types and transformations for bridge communication.
//!
//! This module provides types for virtual document URIs, request tracking,
//! and message transformation between host and virtual document coordinates.

use std::sync::Arc;

use dashmap::DashMap;
use tokio::sync::oneshot;

/// Virtual document URI for injection regions.
///
/// Encodes host URI + injection language + region ID into a unique URI scheme
/// that downstream language servers can use to identify virtual documents.
///
/// Format: `tsls-virtual://{language}/{region_id}?host={url_encoded_host_uri}`
///
/// Example: `tsls-virtual://lua/region-0?host=file%3A%2F%2F%2Fproject%2Fdoc.md`
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
    /// Returns None if the URI is not a valid tsls-virtual:// URI.
    #[allow(dead_code)]
    pub(crate) fn parse(uri_str: &str) -> Option<Self> {
        use percent_encoding::percent_decode_str;
        use tower_lsp::lsp_types::Url;

        let url = Url::parse(uri_str).ok()?;

        // Check scheme
        if url.scheme() != "tsls-virtual" {
            return None;
        }

        // Extract language from host (authority) part
        let language = url.host_str()?.to_string();

        // Extract region_id from path (strip leading /)
        let region_id = url.path().strip_prefix('/')?.to_string();

        // Extract host URI from query parameter
        let query = url.query()?;
        let host_encoded = query.strip_prefix("host=")?;
        let host_decoded = percent_decode_str(host_encoded)
            .decode_utf8()
            .ok()?
            .to_string();
        let host_uri = Url::parse(&host_decoded).ok()?;

        Some(Self {
            host_uri,
            language,
            region_id,
        })
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
    /// Format: `tsls-virtual://{language}/{region_id}?host={url_encoded_host_uri}`
    pub(crate) fn to_uri_string(&self) -> String {
        use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

        // Encode all characters that are not safe in query strings
        const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
            .add(b' ')
            .add(b'"')
            .add(b'#')
            .add(b'<')
            .add(b'>')
            .add(b'`')
            .add(b'?')
            .add(b'{')
            .add(b'}')
            .add(b'/')
            .add(b':')
            .add(b'@')
            .add(b'%');

        let host_encoded = utf8_percent_encode(self.host_uri.as_str(), QUERY_ENCODE_SET);
        format!(
            "tsls-virtual://{}/{}?host={}",
            self.language, self.region_id, host_encoded
        )
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
