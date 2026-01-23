//! Signature help request handling for bridge connections.
//!
//! This module provides signature help request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::Position;
use url::Url;

use super::super::pool::{LanguageServerPool, UpstreamId};
use super::super::protocol::{
    VirtualDocumentUri, build_bridge_signature_help_request,
    transform_signature_help_response_to_host,
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
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
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
        upstream_request_id: UpstreamId,
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

        // Register in the upstream request registry FIRST for cancel lookup.
        // This order matters: if a cancel arrives between pool and router registration,
        // the cancel will fail at the router lookup (which is acceptable for best-effort
        // cancel semantics) rather than finding the language but no downstream ID.
        self.register_upstream_request(upstream_request_id.clone(), injection_language);

        // Register request with upstream ID mapping for cancel forwarding
        let (request_id, response_rx) = match handle
            .register_request_with_upstream(Some(upstream_request_id.clone()))
        {
            Ok(result) => result,
            Err(e) => {
                // Clean up the pool registration on failure
                self.unregister_upstream_request(&upstream_request_id);
                return Err(e);
            }
        };

        // Build signature help request
        let signature_help_request = build_bridge_signature_help_request(
            &host_uri_lsp,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );

        // Send messages while holding writer lock, then release
        // Use a closure for cleanup on any failure path
        let cleanup = || {
            handle.router().remove(request_id);
            self.unregister_upstream_request(&upstream_request_id);
        };

        {
            let mut writer = handle.writer().await;

            // Send didOpen notification only if document hasn't been opened yet
            if let Err(e) = self
                .ensure_document_opened(&mut writer, host_uri, &virtual_uri, virtual_content)
                .await
            {
                cleanup();
                return Err(e);
            }

            if let Err(e) = writer.write_message(&signature_help_request).await {
                cleanup();
                return Err(e);
            }
        } // writer lock released here

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id);

        // Transform response to host coordinates
        Ok(transform_signature_help_response_to_host(
            response?,
            region_start_line,
        ))
    }
}
