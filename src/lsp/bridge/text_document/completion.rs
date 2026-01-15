//! Completion request handling for bridge connections.
//!
//! This module provides completion request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Position, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    RequestId, VirtualDocumentUri, build_bridge_completion_request,
    build_bridge_didchange_notification, build_bridge_didopen_notification,
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

        // Send didOpen or didChange depending on whether document is already opened
        if self.should_send_didopen(host_uri, &virtual_uri).await {
            // First time: send didOpen
            let did_open = build_bridge_didopen_notification(&virtual_uri, virtual_content);
            conn.write_message(&did_open).await?;
        } else {
            // Document already opened: send didChange with incremented version
            if let Some(version) = self.increment_document_version(&virtual_uri).await {
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
        let request_id = RequestId::new(upstream_request_id);
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
        let response = conn.wait_for_response(request_id).await?;

        // Transform response to host coordinates
        Ok(transform_completion_response_to_host(
            response,
            region_start_line,
        ))
    }
}
