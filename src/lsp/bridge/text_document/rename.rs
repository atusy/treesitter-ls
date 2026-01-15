//! Rename request handling for bridge connections.
//!
//! This module provides rename request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Position, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    ResponseTransformContext, VirtualDocumentUri, build_bridge_didopen_notification,
    build_bridge_rename_request, transform_workspace_edit_to_host,
};

impl LanguageServerPool {
    /// Send a rename request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the rename request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` parameter is the request ID from the upstream client,
    /// passed through unchanged to the downstream server per ADR-0016.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_rename_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        new_name: &str,
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

        // Send didOpen notification only if document hasn't been opened yet
        if self.should_send_didopen(host_uri, &virtual_uri).await {
            let did_open = build_bridge_didopen_notification(&virtual_uri, virtual_content);
            conn.write_message(&did_open).await?;
        }

        // Build and send rename request using upstream ID (ADR-0016)
        let request_id = upstream_request_id;
        let rename_request = build_bridge_rename_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            new_name,
            request_id,
        );
        conn.write_message(&rename_request).await?;

        // Build transformation context for response handling
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri_string,
            request_host_uri: host_uri.as_str().to_string(),
            request_region_start_line: region_start_line,
        };

        // Wait for the rename response (skip notifications)
        loop {
            let msg = conn.read_message().await?;
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(request_id)
            {
                // Transform WorkspaceEdit response to host coordinates and URI
                // Cross-region virtual URIs are filtered out
                return Ok(transform_workspace_edit_to_host(msg, &context));
            }
            // Skip notifications and other responses
        }
    }
}
