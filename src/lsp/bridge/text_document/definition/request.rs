//! Request building for definition requests.

use super::super::super::protocol::{RequestId, VirtualDocumentUri};

/// Build a JSON-RPC definition request for a downstream language server.
///
/// # Defensive Arithmetic
///
/// Uses `saturating_sub` for line translation to prevent panic on underflow.
pub(super) fn build_bridge_definition_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_position: tower_lsp_server::ls_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    request_id: RequestId,
) -> serde_json::Value {
    let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

    let virtual_position = tower_lsp_server::ls_types::Position {
        line: host_position.line.saturating_sub(region_start_line),
        character: host_position.character,
    };

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id.as_i64(),
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
