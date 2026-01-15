//! Declaration request handling for bridge connections.
//!
//! This module provides declaration request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.

use std::io;
use std::time::Duration;

use crate::config::settings::BridgeServerConfig;
use tokio::time::timeout;
use tower_lsp::lsp_types::{Position, Url};

/// Timeout for waiting on downstream language server responses.
/// Matches the connection initialization timeout (30 seconds).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

use super::super::pool::LanguageServerPool;
use super::super::protocol::{
    ResponseTransformContext, VirtualDocumentUri, build_bridge_declaration_request,
    transform_definition_response_to_host,
};

impl LanguageServerPool {
    /// Send a declaration request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the declaration request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` parameter is the request ID from the upstream client,
    /// passed through unchanged to the downstream server per ADR-0016.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_declaration_request(
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

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Register request with router to get oneshot receiver
        let (request_id, response_rx) = handle.register_request()?;

        // Build declaration request
        let declaration_request = build_bridge_declaration_request(
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

            writer.write_message(&declaration_request).await?;
        } // writer lock released here

        // Build transformation context for response handling
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri_string,
            request_host_uri: host_uri.as_str().to_string(),
            request_region_start_line: region_start_line,
        };

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = match timeout(REQUEST_TIMEOUT, response_rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                return Err(io::Error::other("response channel closed"));
            }
            Err(_) => {
                // Timeout - clean up pending entry
                handle.router().remove(request_id);
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "bridge request timeout",
                ));
            }
        };

        // Transform response to host coordinates and URI
        // Reuse transform_definition_response_to_host (same Location/LocationLink format per LSP spec)
        // Cross-region virtual URIs are filtered out
        Ok(transform_definition_response_to_host(response, &context))
    }
}
