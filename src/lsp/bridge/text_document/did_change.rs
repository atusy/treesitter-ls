//! didChange notification handling for bridge connections.
//!
//! This module provides didChange notification forwarding for downstream language servers,
//! propagating document changes from host documents to their virtual documents.

use std::io;
use std::sync::Arc;
use tower_lsp::lsp_types::Url;

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
        // Get opened virtual docs for this host (without removing)
        let opened_docs = self.get_host_virtual_docs(host_uri).await;

        // For each injection, check if it's opened and send didChange
        for (language, region_id, content) in injections {
            let virtual_uri =
                VirtualDocumentUri::new(host_uri, language, region_id).to_uri_string();

            // Check if this virtual doc is opened
            if opened_docs.iter().any(|doc| doc.virtual_uri == virtual_uri) {
                // Get version and send didChange
                if let Some(version) = self
                    .increment_document_version(language, &virtual_uri)
                    .await
                {
                    let handle = {
                        let connections = self.connections().await;
                        let Some(handle) = connections.get(language) else {
                            continue;
                        };

                        if handle.state() != ConnectionState::Ready {
                            continue;
                        }

                        Arc::clone(handle)
                    };

                    let virtual_uri = virtual_uri.clone();
                    let content = content.clone();

                    // Fire-and-forget to avoid blocking didChange on downstream I/O.
                    // TODO: Replace with ADR-0015 single-writer loop for ordered, non-blocking sends.
                    tokio::spawn(async move {
                        let _ = Self::send_didchange_for_virtual_doc(
                            handle,
                            virtual_uri,
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

        let mut conn = handle.connection().await;
        conn.write_message(&notification).await
    }
}
