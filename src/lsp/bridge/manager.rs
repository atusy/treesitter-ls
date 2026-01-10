//! Bridge connection manager for downstream language servers.
//!
//! This module provides the BridgeManager which handles lazy initialization
//! of connections and the LSP handshake with downstream language servers.

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::connection::AsyncBridgeConnection;
use super::protocol::{
    VirtualDocumentUri, build_bridge_hover_request, transform_hover_response_to_host,
};

/// Manages bridge connections to downstream language servers.
///
/// Provides lazy initialization of connections and handles the LSP handshake
/// (initialize/initialized) for each language server.
pub(crate) struct BridgeManager {
    /// Map of language -> initialized connection
    connections: Mutex<HashMap<String, Arc<Mutex<AsyncBridgeConnection>>>>,
    /// Map of language -> (virtual document URI -> version)
    /// Tracks which documents have been opened and their current version number
    document_versions: Mutex<HashMap<String, HashMap<String, i32>>>,
    /// Counter for generating unique request IDs
    next_request_id: std::sync::atomic::AtomicI64,
}

impl BridgeManager {
    /// Create a new bridge manager.
    pub(crate) fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            document_versions: Mutex::new(HashMap::new()),
            next_request_id: std::sync::atomic::AtomicI64::new(1),
        }
    }

    /// Check if a virtual document has been opened for a given language.
    ///
    /// This is used to avoid sending duplicate didOpen notifications.
    pub(crate) fn is_document_opened(&self, language: &str, virtual_uri: &str) -> bool {
        // Use try_lock for synchronous access (will always succeed in single-threaded context)
        if let Ok(versions) = self.document_versions.try_lock() {
            if let Some(docs) = versions.get(language) {
                return docs.contains_key(virtual_uri);
            }
        }
        false
    }

    /// Get the current version of a virtual document, if it has been opened.
    pub(crate) fn get_document_version(&self, language: &str, virtual_uri: &str) -> Option<i32> {
        if let Ok(versions) = self.document_versions.try_lock() {
            if let Some(docs) = versions.get(language) {
                return docs.get(virtual_uri).copied();
            }
        }
        None
    }

    /// Mark a virtual document as opened for a given language.
    ///
    /// This should be called after sending didOpen notification to avoid duplicates.
    /// Sets the initial version to 1.
    pub(crate) async fn mark_document_opened(&self, language: &str, virtual_uri: &str) {
        let mut versions = self.document_versions.lock().await;
        versions
            .entry(language.to_string())
            .or_default()
            .insert(virtual_uri.to_string(), 1);
    }

    /// Increment the version of a virtual document and return the new version.
    ///
    /// Returns None if the document has not been opened.
    pub(crate) async fn increment_document_version(
        &self,
        language: &str,
        virtual_uri: &str,
    ) -> Option<i32> {
        let mut versions = self.document_versions.lock().await;
        if let Some(docs) = versions.get_mut(language) {
            if let Some(version) = docs.get_mut(virtual_uri) {
                *version += 1;
                return Some(*version);
            }
        }
        None
    }

    /// Check if document is opened and mark it as opened atomically.
    ///
    /// Returns true if the document was NOT previously opened (i.e., didOpen should be sent).
    /// Returns false if the document was already opened (i.e., skip didOpen).
    async fn should_send_didopen(&self, language: &str, virtual_uri: &str) -> bool {
        let mut versions = self.document_versions.lock().await;
        let docs = versions.entry(language.to_string()).or_default();
        if docs.contains_key(virtual_uri) {
            false
        } else {
            docs.insert(virtual_uri.to_string(), 1);
            true
        }
    }

    /// Get or create a connection for the specified language.
    ///
    /// If no connection exists, spawns the language server and performs
    /// the LSP initialize/initialized handshake.
    #[allow(dead_code)]
    pub(crate) async fn get_or_create_connection(
        &self,
        language: &str,
        server_config: &crate::config::settings::BridgeServerConfig,
    ) -> io::Result<Arc<Mutex<AsyncBridgeConnection>>> {
        let mut connections = self.connections.lock().await;

        // Check if we already have a connection for this language
        if let Some(conn) = connections.get(language) {
            return Ok(Arc::clone(conn));
        }

        // Spawn new connection
        let mut conn = AsyncBridgeConnection::spawn(server_config.cmd.clone()).await?;

        // Perform LSP initialize handshake
        let init_request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.next_request_id(),
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {},
                "initializationOptions": server_config.initialization_options
            }
        });

        conn.write_message(&init_request).await?;

        // Read initialize response (skip any notifications)
        loop {
            let msg = conn.read_message().await?;
            if msg.get("id").is_some() {
                // Got the initialize response
                if msg.get("error").is_some() {
                    return Err(io::Error::other(format!(
                        "Initialize failed: {:?}",
                        msg.get("error")
                    )));
                }
                break;
            }
            // Skip notifications
        }

        // Send initialized notification
        let initialized = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        conn.write_message(&initialized).await?;

        let conn = Arc::new(Mutex::new(conn));
        connections.insert(language.to_string(), Arc::clone(&conn));
        Ok(conn)
    }

    /// Generate a unique request ID.
    fn next_request_id(&self) -> i64 {
        self.next_request_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Send a hover request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection to the language server
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the hover request
    /// 4. Wait for and return the response
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_hover_request(
        &self,
        server_config: &crate::config::settings::BridgeServerConfig,
        host_uri: &tower_lsp::lsp_types::Url,
        host_position: tower_lsp::lsp_types::Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
    ) -> io::Result<serde_json::Value> {
        // Get or create connection
        let conn = self
            .get_or_create_connection(injection_language, server_config)
            .await?;
        let mut conn = conn.lock().await;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Send didOpen notification only if document hasn't been opened yet
        if self
            .should_send_didopen(injection_language, &virtual_uri_string)
            .await
        {
            let did_open = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": virtual_uri_string,
                        "languageId": injection_language,
                        "version": 1,
                        "text": virtual_content
                    }
                }
            });
            conn.write_message(&did_open).await?;
        }

        // Build and send hover request
        let request_id = self.next_request_id();
        let hover_request = build_bridge_hover_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );
        conn.write_message(&hover_request).await?;

        // Wait for the hover response (skip notifications)
        loop {
            let msg = conn.read_message().await?;
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(request_id)
            {
                // Transform response to host coordinates
                return Ok(transform_hover_response_to_host(msg, region_start_line));
            }
            // Skip notifications and other responses
        }
    }
}
