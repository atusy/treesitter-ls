//! References request handling for bridge connections.
//!
//! This module provides references request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Position, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    ResponseTransformContext, VirtualDocumentUri, build_bridge_references_request,
    transform_definition_response_to_host,
};

impl LanguageServerPool {
    /// Send a references request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the references request
    /// 4. Wait for and return the response
    ///
    /// See [`send_hover_request`](Self::send_hover_request) for documentation on why
    /// `_upstream_request_id` is intentionally unused.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_references_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        include_declaration: bool,
        _upstream_request_id: i64,
    ) -> io::Result<serde_json::Value> {
        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(injection_language, server_config)
            .await?;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Register request with router to get oneshot receiver
        let (request_id, response_rx) = handle.register_request()?;

        // Build references request
        let references_request = build_bridge_references_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            include_declaration,
            request_id,
        );

        // Send messages while holding writer lock, then release
        {
            let mut writer = handle.writer().await;

            // Send didOpen notification only if document hasn't been opened yet
            self.ensure_document_opened(
                &mut writer,
                host_uri,
                &virtual_uri,
                virtual_content,
                || {
                    handle.router().remove(request_id);
                },
            )
            .await?;

            writer.write_message(&references_request).await?;
        } // writer lock released here

        // Build transformation context for response handling
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri_string,
            request_host_uri: host_uri.as_str().to_string(),
            request_region_start_line: region_start_line,
        };

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await?;

        // Transform response to host coordinates and URI
        // Reuse transform_definition_response_to_host (same Location[] format per LSP spec)
        // Cross-region virtual URIs are filtered out
        Ok(transform_definition_response_to_host(response, &context))
    }
}
