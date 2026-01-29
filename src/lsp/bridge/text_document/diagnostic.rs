//! Diagnostic request handling for bridge connections.
//!
//! This module provides pull diagnostic request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! Like document symbol, diagnostic requests operate on the entire document -
//! they don't take a position parameter.
//!
//! Implements ADR-0020 Phase 1: Pull-first diagnostic forwarding.

use std::io;
use std::time::Duration;

use crate::config::settings::BridgeServerConfig;
use url::Url;

use super::super::pool::{LanguageServerPool, UpstreamId};
use super::super::protocol::{
    ResponseTransformContext, VirtualDocumentUri, build_bridge_diagnostic_request,
    transform_diagnostic_response_to_host,
};

/// Timeout for waiting for server to become ready.
///
/// This matches the INIT_TIMEOUT_SECS in pool.rs (30 seconds).
/// Diagnostic requests wait for initializing servers to provide better UX.
const WAIT_FOR_READY_TIMEOUT_SECS: u64 = 30;

impl LanguageServerPool {
    /// Send a diagnostic request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection, waiting for initialization if needed
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the diagnostic request
    /// 4. Wait for and return the response
    ///
    /// Unlike position-based requests, diagnostic operates on the entire document,
    /// so no position translation is needed for the request.
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    ///
    /// # Wait-for-Ready Behavior
    ///
    /// Unlike other request types that fail fast when a server is initializing,
    /// diagnostic requests wait for the server to become Ready. This provides
    /// better UX - users see diagnostics appear once the server is ready rather
    /// than seeing empty results.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_diagnostic_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        upstream_request_id: UpstreamId,
        previous_result_id: Option<&str>,
    ) -> io::Result<serde_json::Value> {
        // Get or create connection, waiting for Ready state if initializing.
        // Unlike other requests that fail fast, diagnostics wait for initialization
        // to provide better UX (diagnostics appear once server is ready).
        let handle = self
            .get_or_create_connection_wait_ready(
                server_name,
                server_config,
                Duration::from_secs(WAIT_FOR_READY_TIMEOUT_SECS),
            )
            .await?;

        // Convert host_uri to lsp_types::Uri for bridge protocol functions
        let host_uri_lsp = crate::lsp::lsp_impl::url_to_uri(host_uri)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(&host_uri_lsp, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Register in the upstream request registry FIRST for cancel lookup.
        // This order matters: if a cancel arrives between pool and router registration,
        // the cancel will fail at the router lookup (which is acceptable for best-effort
        // cancel semantics) rather than finding the server but no downstream ID.
        self.register_upstream_request(upstream_request_id.clone(), server_name);

        // Register request with upstream ID mapping for cancel forwarding
        let (request_id, response_rx) =
            match handle.register_request_with_upstream(Some(upstream_request_id.clone())) {
                Ok(result) => result,
                Err(e) => {
                    // Clean up the pool registration on failure
                    self.unregister_upstream_request(&upstream_request_id);
                    return Err(e);
                }
            };

        // Build diagnostic request
        // Note: diagnostic doesn't need position - it operates on the whole document
        let request = build_bridge_diagnostic_request(
            &host_uri_lsp,
            injection_language,
            region_id,
            request_id,
            previous_result_id,
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
                .ensure_document_opened(
                    &mut writer,
                    host_uri,
                    &virtual_uri,
                    virtual_content,
                    server_name,
                )
                .await
            {
                cleanup();
                return Err(e);
            }

            if let Err(e) = writer.write_message(&request).await {
                cleanup();
                return Err(e);
            }
        } // writer lock released here

        // Build transformation context for response handling
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri_string,
            request_host_uri: host_uri.as_str().to_string(),
            request_region_start_line: region_start_line,
        };

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id);

        // Transform response to host coordinates
        Ok(transform_diagnostic_response_to_host(response?, &context))
    }
}
