//! Outbound message types for the writer loop.
//!
//! This module provides the `OutboundMessage` enum that represents messages
//! to be sent to a downstream language server via the single-writer loop
//! (ADR-0015).
//!
//! All messages pass through the unified order queue to ensure FIFO ordering.
//! The writer task consumes these and writes to stdin.

use crate::lsp::bridge::protocol::RequestId;

/// Message to be sent to a downstream language server.
///
/// All messages pass through the unified order queue to ensure FIFO ordering
/// per ADR-0015. The writer task consumes these and writes to stdin.
#[derive(Debug)]
pub(crate) enum OutboundMessage {
    /// Notification (no response expected).
    ///
    /// If the queue is full, notifications are dropped with WARN logging.
    Notification(serde_json::Value),

    /// Request (response expected).
    ///
    /// The request_id must be registered with ResponseRouter BEFORE queuing.
    /// If the queue is full, the request is rejected with REQUEST_FAILED.
    Request {
        /// The JSON-RPC request payload
        payload: serde_json::Value,
        /// Request ID for correlation (already registered with router)
        request_id: RequestId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn outbound_message_notification_debug_format() {
        let notification =
            OutboundMessage::Notification(json!({"method": "textDocument/didChange"}));
        let debug_str = format!("{:?}", notification);
        assert!(
            debug_str.contains("Notification"),
            "Debug format should contain 'Notification': {}",
            debug_str
        );
    }

    #[test]
    fn outbound_message_request_debug_format() {
        let request = OutboundMessage::Request {
            payload: json!({"method": "textDocument/hover", "id": 42}),
            request_id: RequestId::new(42),
        };
        let debug_str = format!("{:?}", request);
        assert!(
            debug_str.contains("Request"),
            "Debug format should contain 'Request': {}",
            debug_str
        );
        assert!(
            debug_str.contains("request_id"),
            "Debug format should contain 'request_id': {}",
            debug_str
        );
    }
}
