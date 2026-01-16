//! Inlay hint request handling for bridge connections.
//!
//! This module provides inlay hint request functionality for downstream language servers,
//! handling the bidirectional coordinate transformation between host and virtual documents.
//!
//! Unlike position-based requests, inlay hints use a range parameter in the request
//! that specifies the visible document range. Both request range (host->virtual) and
//! response positions/textEdits (virtual->host) need transformation.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::{Range, Url};

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    ResponseTransformContext, VirtualDocumentUri, build_bridge_inlay_hint_request,
    transform_inlay_hint_response_to_host,
};

impl LanguageServerPool {
    /// Send an inlay hint request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the inlay hint request with range transformed to virtual coordinates
    /// 4. Wait for and return the response with positions/textEdits transformed to host
    ///
    /// Unlike position-based requests, this uses a range parameter which needs
    /// transformation from host to virtual coordinates in the request.
    ///
    /// See [`send_hover_request`](Self::send_hover_request) for documentation on why
    /// `_upstream_request_id` is intentionally unused.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_inlay_hint_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_range: Range,
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

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Register request with router to get oneshot receiver
        let (request_id, response_rx) = handle.register_request()?;

        // Build inlay hint request
        // Note: request builder transforms host_range to virtual coordinates
        let request = build_bridge_inlay_hint_request(
            host_uri,
            host_range,
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

            writer.write_message(&request).await?;
        } // writer lock released here

        // Build transformation context for response handling
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri_string,
            request_host_uri: host_uri.as_str().to_string(),
            request_region_start_line: region_start_line,
        };

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await?;

        // Transform response positions and textEdits to host coordinates
        Ok(transform_inlay_hint_response_to_host(response, &context))
    }
}
