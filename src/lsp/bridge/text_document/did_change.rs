//! didChange notification handling for bridge connections.
//!
//! This module provides didChange notification forwarding for downstream language servers,
//! propagating document changes from host documents to their virtual documents.

use std::io;
use std::sync::Arc;
use url::Url;

use super::super::pool::{ConnectionHandle, ConnectionState, LanguageServerPool};
use super::super::protocol::VirtualDocumentUri;

impl LanguageServerPool {
    /// Forward didChange notifications to all opened virtual documents for a host document.
    ///
    /// When the host document (e.g., markdown file) changes, this method:
    /// 1. Gets the list of opened virtual documents for the host
    /// 2. For each injection that has an opened virtual document, sends didChange
    /// 3. Skips injections that haven't been opened yet (didOpen will be sent on first request)
    ///
    /// Uses full content sync (TextDocumentSyncKind::Full) for simplicity.
    ///
    /// # Arguments
    /// * `host_uri` - The host document URI
    /// * `injections` - List of (language, region_id, content) tuples for all injection regions
    ///
    // TODO: Support incremental didChange (TextDocumentSyncKind::Incremental) for better
    // performance with large documents. Currently uses full sync for simplicity.
    pub(crate) async fn forward_didchange_to_opened_docs(
        &self,
        host_uri: &Url,
        injections: &[(String, String, String)], // (language, region_id, content)
    ) {
        // Convert host_uri to lsp_types::Uri for bridge protocol functions
        let host_uri_lsp = match crate::lsp::lsp_impl::url_to_uri(host_uri) {
            Ok(uri) => uri,
            Err(e) => {
                log::warn!(
                    target: "kakehashi::bridge",
                    "Failed to convert host URI, skipping didChange: {}",
                    e
                );
                return;
            }
        };

        // For each injection, check if it's actually opened and send didChange
        for (language, region_id, content) in injections {
            let virtual_uri = VirtualDocumentUri::new(&host_uri_lsp, language, region_id);

            // Check if this virtual doc has ACTUALLY been opened (didOpen sent to downstream)
            // per ADR-0015. This prevents sending didChange before didOpen.
            if self.is_document_opened(&virtual_uri) {
                // Look up server_name from tracking (reverse lookup via OpenedVirtualDoc.server_name)
                // This is required because pool is keyed by server_name, not language.
                let Some(server_name) = self.get_server_for_virtual_uri(&virtual_uri).await else {
                    log::warn!(
                        target: "kakehashi::bridge",
                        "Could not find server_name for virtual_uri: {}, skipping didChange",
                        virtual_uri.to_uri_string()
                    );
                    continue;
                };

                // Get version and send didChange
                if let Some(version) = self.increment_document_version(&virtual_uri, &server_name).await {
                    let handle = {
                        let connections = self.connections().await;
                        let Some(handle) = connections.get(&server_name) else {
                            continue;
                        };

                        if handle.state() != ConnectionState::Ready {
                            continue;
                        }

                        Arc::clone(handle)
                    };

                    let virtual_uri_string = virtual_uri.to_uri_string();
                    let content = content.clone();

                    // Fire-and-forget to avoid blocking didChange on downstream I/O.
                    // TODO: Replace with ADR-0015 single-writer loop for ordered, non-blocking sends.
                    tokio::spawn(async move {
                        let _ = Self::send_didchange_for_virtual_doc(
                            handle,
                            virtual_uri_string,
                            content,
                            version,
                        )
                        .await;
                    });
                }
            }
            // If not opened, skip - didOpen will be sent on first request
        }
    }

    /// Send a didChange notification for a virtual document.
    ///
    /// This method sends a didChange notification to the downstream language server
    /// for the specified virtual document URI. Uses full content sync.
    ///
    /// # Arguments
    /// * `language` - The injection language (e.g., "lua")
    /// * `virtual_uri` - The virtual document URI string
    /// * `content` - The new content for the virtual document
    /// * `version` - The document version number
    async fn send_didchange_for_virtual_doc(
        handle: Arc<ConnectionHandle>,
        virtual_uri: String,
        content: String,
        version: i32,
    ) -> io::Result<()> {
        // Build and send the didChange notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didChange",
            "params": {
                "textDocument": {
                    "uri": virtual_uri,
                    "version": version
                },
                "contentChanges": [
                    {
                        "text": content
                    }
                ]
            }
        });

        let mut writer = handle.writer().await;
        writer.write_message(&notification).await
    }
}
