//! Downstream message types for notification forwarding.
//!
//! This module defines the types used to forward notifications and (future) requests
//! from downstream language servers to the client. Part of the Channel-based
//! notification forwarding architecture per ADR-0016.
//!
//! # Architecture
//!
//! ```text
//! Reader Task --> mpsc::Sender<DownstreamMessage> --> DownstreamMessageHandler --> Client
//! ```
//!
//! # Message Types
//!
//! - `DownstreamNotification`: Notifications from downstream LS (e.g., `$/progress`)
//! - `DownstreamRequest`: Future support for server-to-client requests (e.g., `workspace/applyEdit`)

use serde_json::Value;
use url::Url;

/// Messages from downstream language servers that need to be forwarded to the client.
///
/// Currently only supports notifications. The `Request` variant is reserved for
/// future support of server-to-client requests (e.g., `workspace/applyEdit`).
#[derive(Debug)]
pub(crate) enum DownstreamMessage {
    /// A notification from a downstream language server.
    Notification(DownstreamNotification),
    // Future: Request(DownstreamRequest)
}

/// A notification from a downstream language server.
///
/// Contains the server name (for token prefixing in `$/progress`) and
/// the raw JSON-RPC notification.
#[derive(Debug)]
pub(crate) struct DownstreamNotification {
    /// Name of the originating server (e.g., "lua-language-server").
    ///
    /// Used for:
    /// - Token prefix in `$/progress` notifications (e.g., "lua-ls:token123")
    /// - Logging and debugging
    pub(crate) server_name: String,

    /// The raw JSON-RPC notification.
    ///
    /// Contains "jsonrpc", "method", and "params" fields.
    pub(crate) notification: Value,
}

/// Context for virtual document URI resolution.
///
/// Used by the DownstreamMessageHandler to map virtual document URIs back to
/// host document URIs and adjust positions (e.g., in `publishDiagnostics`).
///
/// This struct is defined in Phase 0 but will be used starting in Phase 1.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Will be used in Phase 1
pub(crate) struct VirtualDocContext {
    /// The host document URI that contains this virtual document.
    pub(crate) host_uri: Url,

    /// The starting line of this virtual document region in the host document.
    ///
    /// Used to offset positions when converting from virtual to host coordinates.
    pub(crate) region_start_line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn downstream_notification_stores_server_name_and_notification() {
        let notification = DownstreamNotification {
            server_name: "lua-ls".to_string(),
            notification: json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": "token123",
                    "value": { "kind": "begin", "title": "Indexing" }
                }
            }),
        };

        assert_eq!(notification.server_name, "lua-ls");
        assert_eq!(notification.notification["method"], "$/progress");
    }

    #[test]
    fn downstream_message_wraps_notification() {
        let notification = DownstreamNotification {
            server_name: "pyright".to_string(),
            notification: json!({
                "jsonrpc": "2.0",
                "method": "window/showMessage",
                "params": { "type": 3, "message": "Hello" }
            }),
        };

        let message = DownstreamMessage::Notification(notification);

        match message {
            DownstreamMessage::Notification(n) => {
                assert_eq!(n.server_name, "pyright");
                assert_eq!(n.notification["method"], "window/showMessage");
            }
        }
    }

    #[test]
    fn virtual_doc_context_stores_host_uri_and_region_start() {
        let context = VirtualDocContext {
            host_uri: Url::parse("file:///test/doc.md").unwrap(),
            region_start_line: 10,
        };

        assert_eq!(context.host_uri.as_str(), "file:///test/doc.md");
        assert_eq!(context.region_start_line, 10);
    }
}
