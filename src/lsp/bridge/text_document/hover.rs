//! Hover request handling for bridge connections.
//!
//! This module provides hover request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::Position;
use url::Url;

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    VirtualDocumentUri, build_bridge_hover_request, transform_hover_response_to_host,
};

impl LanguageServerPool {
    /// Send a hover request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Register request with router to get oneshot receiver
    /// 4. Send the hover request (release writer lock after)
    /// 5. Wait for response via oneshot channel (no Mutex held)
    ///
    /// # Note on `_upstream_request_id`
    ///
    /// This parameter is intentionally unused. Originally, the upstream request ID was
    /// passed directly to downstream servers (per ADR-0016's "Request ID Semantics").
    /// However, using upstream IDs caused ID collisions when multiple downstream servers
    /// were active, since each server expects unique request IDs within its connection.
    ///
    /// The current implementation generates unique downstream request IDs via
    /// `register_request()` (using an atomic counter per connection). The parameter is
    /// retained for:
    /// - **API consistency**: All text_document handlers share a uniform signature
    /// - **Future correlation logging**: May be used to correlate upstream/downstream
    ///   request pairs in debug logs
    ///
    /// The underscore prefix signals "intentionally unused" to both the compiler and
    /// code reviewers.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_hover_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        _upstream_request_id: i64,
    ) -> io::Result<serde_json::Value> {
        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(injection_language, server_config)
            .await?;

        // Convert host_uri to lsp_types::Uri for bridge protocol functions
        let host_uri_lsp = crate::lsp::lsp_impl::url_to_uri(host_uri)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(&host_uri_lsp, injection_language, region_id);

        // Register request with router to get oneshot receiver
        let (request_id, response_rx) = handle.register_request()?;

        // Build hover request
        let hover_request = build_bridge_hover_request(
            &host_uri_lsp,
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

            writer.write_message(&hover_request).await?;
        } // writer lock released here

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await?;

        // Transform response to host coordinates
        Ok(transform_hover_response_to_host(
            response,
            region_start_line,
        ))
    }
}
