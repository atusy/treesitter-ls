//! Color presentation request handling for bridge connections.
//!
//! This module provides color presentation request functionality for downstream language servers,
//! handling the bidirectional coordinate transformation between host and virtual documents.
//!
//! Like inlay hints, color presentation uses a range parameter in the request (the range
//! where the color was found) and the response may contain textEdits and additionalTextEdits
//! that need transformation back to host coordinates.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Range, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    VirtualDocumentUri, build_bridge_color_presentation_request, build_bridge_didopen_notification,
    transform_color_presentation_response_to_host,
};

impl LanguageServerPool {
    /// Send a color presentation request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the color presentation request with range transformed to virtual coordinates
    /// 4. Wait for and return the response with textEdits transformed to host coordinates
    ///
    /// Like inlay hints, this uses a range parameter which needs transformation from
    /// host to virtual coordinates in the request, and textEdits in the response need
    /// transformation back to host coordinates.
    ///
    /// The `upstream_request_id` parameter is the request ID from the upstream client,
    /// passed through unchanged to the downstream server per ADR-0016.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_color_presentation_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_range: Range,
        color: &serde_json::Value,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        upstream_request_id: i64,
    ) -> io::Result<serde_json::Value> {
        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(injection_language, server_config)
            .await?;
        let mut conn = handle.connection().await;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

        // Send didOpen notification only if document hasn't been opened yet
        if self.should_send_didopen(host_uri, &virtual_uri).await {
            let did_open = build_bridge_didopen_notification(&virtual_uri, virtual_content);
            conn.write_message(&did_open).await?;
        }

        // Build and send color presentation request using upstream ID (ADR-0016)
        // Note: request builder transforms host_range to virtual coordinates
        let request_id = upstream_request_id;
        let request = build_bridge_color_presentation_request(
            host_uri,
            host_range,
            color,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );
        conn.write_message(&request).await?;

        // Wait for the color presentation response (skip notifications)
        loop {
            let msg = conn.read_message().await?;
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(request_id)
            {
                // Transform response textEdits and additionalTextEdits to host coordinates
                return Ok(transform_color_presentation_response_to_host(
                    msg,
                    region_start_line,
                ));
            }
            // Skip notifications and other responses
        }
    }
}
