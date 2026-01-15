//! Rename request handling for bridge connections.
//!
//! This module provides rename request functionality for downstream language servers,
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

        // Build rename request
        let rename_request = build_bridge_rename_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            new_name,
            request_id,
        );

        // Send messages while holding writer lock, then release
        {
            let mut writer = handle.writer().await;

            // Send didOpen notification only if document hasn't been opened yet
            if self.should_send_didopen(host_uri, &virtual_uri).await {
                let did_open = build_bridge_didopen_notification(&virtual_uri, virtual_content);
                writer.write_message(&did_open).await?;
                // Mark as opened AFTER successful write (ADR-0015)
                self.mark_document_opened(&virtual_uri);
            } else if !self.is_document_opened(&virtual_uri) {
                // Document marked for opening but didOpen not yet sent (race condition)
                // Drop the request per ADR-0015
                // Clean up pending entry to avoid memory leak
                handle.router().remove(request_id);
                return Err(io::Error::other(
                    "bridge: document not yet opened (didOpen pending)",
                ));
            }

            writer.write_message(&rename_request).await?;
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

        // Transform WorkspaceEdit response to host coordinates and URI
        // Cross-region virtual URIs are filtered out
        Ok(transform_workspace_edit_to_host(response, &context))
    }
}
