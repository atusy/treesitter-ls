//! Signature help request handling for bridge connections.
//!
//! This module provides signature help request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Position, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    RequestId, VirtualDocumentUri, build_bridge_didopen_notification,
    build_bridge_signature_help_request, transform_signature_help_response_to_host,
};

impl LanguageServerPool {
    /// Send a signature help request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the signature help request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` parameter is the request ID from the upstream client,
    /// passed through unchanged to the downstream server per ADR-0016.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_signature_help_request(
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

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);

        // Build request ID and register with router BEFORE sending
        let request_id = RequestId::new(upstream_request_id);
        let response_rx = handle
            .router()
            .register(request_id)
            .ok_or_else(|| io::Error::other("duplicate request ID"))?;

        // Build signature help request
        let signature_help_request = build_bridge_signature_help_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );

        // Send messages while holding writer lock, then release
        {
            let mut writer = handle.writer().await;

            // Send didOpen notification only if document hasn't been opened yet
            if self.should_send_didopen(host_uri, &virtual_uri).await {
                let did_open = build_bridge_didopen_notification(&virtual_uri, virtual_content);
                writer.write_message(&did_open).await?;
            }

            writer.write_message(&signature_help_request).await?;
        } // writer lock released here

        // Wait for response via oneshot channel (no Mutex held)
        let response = response_rx
            .await
            .map_err(|_| io::Error::other("response channel closed"))?;

        // Transform response to host coordinates
        Ok(transform_signature_help_response_to_host(
            response,
            region_start_line,
        ))
    }
}
