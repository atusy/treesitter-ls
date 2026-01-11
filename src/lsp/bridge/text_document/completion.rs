//! Completion request handling for bridge connections.
//!
//! This module provides completion request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Position, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    VirtualDocumentUri, build_bridge_completion_request, build_bridge_didchange_notification,
    transform_completion_response_to_host,
};

impl LanguageServerPool {
    /// Send a completion request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if not opened, or didChange if already opened
    /// 3. Send the completion request
    /// 4. Wait for and return the response with transformed coordinates
    ///
    /// The `upstream_request_id` parameter is the request ID from the upstream client,
    /// passed through unchanged to the downstream server per ADR-0016.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_completion_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
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
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Send didOpen or didChange depending on whether document is already opened
        if self
            .should_send_didopen(host_uri, injection_language, &virtual_uri_string)
            .await
        {
            // First time: send didOpen
            let did_open = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": virtual_uri_string,
                        "languageId": injection_language,
                        "version": 1,
                        "text": virtual_content
                    }
                }
            });
            conn.write_message(&did_open).await?;
        } else {
            // Document already opened: send didChange with incremented version
            if let Some(version) = self
                .increment_document_version(injection_language, &virtual_uri_string)
                .await
            {
                let did_change = build_bridge_didchange_notification(
                    host_uri,
                    injection_language,
                    region_id,
                    virtual_content,
                    version,
                );
                conn.write_message(&did_change).await?;
            }
        }

        // Build and send completion request using upstream ID (ADR-0016)
        let request_id = upstream_request_id;
        let completion_request = build_bridge_completion_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );
        conn.write_message(&completion_request).await?;

        // Wait for the completion response (skip notifications)
        loop {
            let msg = conn.read_message().await?;
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(request_id)
            {
                // Transform response to host coordinates
                return Ok(transform_completion_response_to_host(
                    msg,
                    region_start_line,
                ));
            }
            // Skip notifications and other responses
        }
    }
}
