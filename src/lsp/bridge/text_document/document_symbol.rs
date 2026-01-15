//! Document symbol request handling for bridge connections.
//!
//! This module provides document symbol request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! Like document link, document symbol requests operate on the entire document -
//! they don't take a position parameter.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp::lsp_types::Url;

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    ResponseTransformContext, VirtualDocumentUri, build_bridge_didopen_notification,
    build_bridge_document_symbol_request, transform_document_symbol_response_to_host,
};

impl LanguageServerPool {
    /// Send a document symbol request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the document symbol request
    /// 4. Wait for and return the response
    ///
    /// Unlike position-based requests, document symbol operates on the entire document,
    /// so no position translation is needed for the request.
    ///
    /// The `upstream_request_id` parameter is the request ID from the upstream client,
    /// passed through unchanged to the downstream server per ADR-0016.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_document_symbol_request(
        &self,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
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

        // Build and send document symbol request using upstream ID (ADR-0016)
        // Note: document symbol doesn't need position - it operates on the whole document
        let request_id = upstream_request_id;
        let request = build_bridge_document_symbol_request(
            host_uri,
            injection_language,
            region_id,
            request_id,
        );
        conn.write_message(&request).await?;

        // Build transformation context for response
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_uri_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: region_start_line,
        };

        // Wait for the document symbol response (skip notifications)
        let response = conn.wait_for_response(request_id).await?;

        // Transform response to host coordinates
        Ok(transform_document_symbol_response_to_host(
            response, &context,
        ))
    }
}
