//! Tokio-based async language server pool.
//!
//! This module provides `TokioAsyncLanguageServerPool` which manages `TokioAsyncBridgeConnection`
//! instances for concurrent LSP request handling with fully async I/O.
//!
//! # Key Differences from AsyncLanguageServerPool
//!
//! - Uses `TokioAsyncBridgeConnection` instead of `AsyncBridgeConnection`
//! - Spawn and initialize are fully async (no spawn_blocking)
//! - All I/O operations use tokio async primitives

use super::tokio_connection::TokioAsyncBridgeConnection;
use super::workspace::{language_to_extension, setup_workspace_with_option};
use crate::config::settings::BridgeServerConfig;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Mutex, mpsc};
use tokio::time::{Duration, timeout};

/// Timeout for language server initialization
const INIT_TIMEOUT_SECS: u64 = 60;

/// Pool of tokio-based async language server connections.
///
/// Unlike `AsyncLanguageServerPool` which uses `spawn_blocking` for initialization,
/// this pool uses fully async I/O throughout. Each connection is a
/// `TokioAsyncBridgeConnection` that uses tokio::process for spawning.
pub struct TokioAsyncLanguageServerPool {
    /// Active connections by key (server name)
    connections: DashMap<String, Arc<TokioAsyncBridgeConnection>>,
    /// Virtual file URIs per connection key (for textDocument requests)
    virtual_uris: DashMap<String, String>,
    /// Channel for forwarding $/progress notifications
    notification_sender: mpsc::Sender<Value>,
    /// Document versions per virtual URI (for didOpen/didChange tracking)
    document_versions: DashMap<String, u32>,
    /// Mapping from host document URIs to their associated bridge virtual URIs
    /// This tracks which bridge documents belong to each host document for scoped cleanup
    host_to_bridge_uris: DashMap<String, HashSet<String>>,
    /// Per-key spawn locks to prevent concurrent connection spawning.
    ///
    /// This pattern is necessary because:
    /// 1. DashMap entry API doesn't support async operations
    /// 2. We need to hold a lock across async spawn_and_initialize
    /// 3. Each key needs independent locking to allow concurrent spawns for different keys
    ///
    /// The double-mutex pattern (Mutex<HashMap<..., Arc<Mutex<()>>>>) allows:
    /// - Quick lookup of per-key lock (outer sync Mutex, held briefly)
    /// - Per-key async locking (inner tokio Mutex, held across spawn)
    spawn_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Instance-level counter for generating unique temporary directory names
    spawn_counter: AtomicU64,
}

impl TokioAsyncLanguageServerPool {
    /// Create a new pool with a notification channel.
    ///
    /// # Arguments
    /// * `notification_sender` - Channel for forwarding $/progress notifications
    pub fn new(notification_sender: mpsc::Sender<Value>) -> Self {
        Self {
            connections: DashMap::new(),
            virtual_uris: DashMap::new(),
            notification_sender,
            document_versions: DashMap::new(),
            host_to_bridge_uris: DashMap::new(),
            spawn_locks: Mutex::new(HashMap::new()),
            spawn_counter: AtomicU64::new(0),
        }
    }

    /// Get or create a tokio async connection for the given key.
    ///
    /// This method is fully async - unlike `AsyncLanguageServerPool::get_connection`
    /// which uses `spawn_blocking` internally, this uses tokio async I/O throughout.
    ///
    /// Uses per-key mutex to prevent race conditions where concurrent calls could
    /// spawn multiple processes for the same key. The mutex is held across the spawn
    /// operation to ensure only one process is spawned.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning a new connection if needed
    pub async fn get_connection(
        &self,
        key: &str,
        config: &BridgeServerConfig,
    ) -> Option<Arc<TokioAsyncBridgeConnection>> {
        // Fast path: check if we already have a connection
        if let Some(conn) = self.connections.get(key) {
            return Some(conn.clone());
        }

        // Get or create a lock for this key
        let lock = {
            let mut locks = self.spawn_locks.lock().await;
            locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Acquire the per-key lock
        let _guard = lock.lock().await;

        // Double-check: another task might have created the connection while we waited for the lock
        if let Some(conn) = self.connections.get(key) {
            return Some(conn.clone());
        }

        // We hold the lock and no connection exists - spawn and initialize
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[POOL] Spawning new connection for key={}",
            key
        );

        let spawn_result = timeout(
            Duration::from_secs(INIT_TIMEOUT_SECS),
            self.spawn_and_initialize(config),
        )
        .await;

        let (conn, virtual_uri) = match spawn_result {
            Ok(Some(result)) => result,
            Ok(None) => {
                log::warn!(target: "treesitter_ls::bridge", "spawn_and_initialize returned None");
                return None;
            }
            Err(_) => {
                log::warn!(target: "treesitter_ls::bridge", "spawn_and_initialize timed out after {}s", INIT_TIMEOUT_SECS);
                return None;
            }
        };
        let conn = Arc::new(conn);

        // Insert into maps
        self.connections.insert(key.to_string(), conn.clone());
        self.virtual_uris.insert(key.to_string(), virtual_uri);

        Some(conn)
    }

    /// Spawn a new connection and initialize it with the language server.
    ///
    /// This is fully async unlike `AsyncLanguageServerPool::spawn_async_connection_blocking`.
    async fn spawn_and_initialize(
        &self,
        config: &BridgeServerConfig,
    ) -> Option<(TokioAsyncBridgeConnection, String)> {
        let program = config.cmd.first()?;
        let args: Vec<&str> = config.cmd.iter().skip(1).map(|s| s.as_str()).collect();

        // Create temp directory with instance-level counter
        let counter = self
            .spawn_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "treesitter-ls-tokio-{}-{}-{}",
            program,
            std::process::id(),
            counter
        ));
        tokio::fs::create_dir_all(&temp_dir).await.ok()?;

        // Determine extension and setup workspace
        let extension = config
            .languages
            .first()
            .map(|lang| language_to_extension(lang))
            .unwrap_or("rs");

        // Use spawn_blocking to avoid blocking tokio runtime on sync file I/O
        let virtual_file_path = {
            let temp_dir = temp_dir.clone();
            let workspace_type = config.workspace_type;
            let extension = extension.to_string();
            tokio::task::spawn_blocking(move || {
                setup_workspace_with_option(&temp_dir, workspace_type, &extension)
            })
            .await
            .ok()? // JoinError -> None
            ? // Option<PathBuf> -> PathBuf or None
        };

        let root_uri = format!("file://{}", temp_dir.display());

        // Spawn connection using TokioAsyncBridgeConnection with temp_dir as cwd
        // Pass notification_sender to forward $/progress notifications
        // Pass temp_dir for cleanup on drop
        let conn = TokioAsyncBridgeConnection::spawn(
            program,
            &args,
            Some(&temp_dir),
            Some(self.notification_sender.clone()),
            Some(temp_dir.clone()),
        )
        .await
        .ok()?;

        // Send initialize request
        let mut init_params = serde_json::json!({
            "processId": std::process::id(),
            "capabilities": {},
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": "virtual"}],
        });

        if let Some(ref init_opts) = config.initialization_options {
            init_params["initializationOptions"] = init_opts.clone();
        }

        // Send initialize request and await response
        let (_, receiver) = conn.send_request("initialize", init_params).await.ok()?;

        // Wait for init response
        let init_response = receiver.await.ok()?;
        init_response.response.as_ref()?;

        // Send initialized notification
        conn.send_notification("initialized", serde_json::json!({}))
            .await
            .ok()?;

        log::info!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[POOL] Connection spawned for {}",
            program
        );

        // Return connection along with the virtual file URI
        let virtual_uri = format!("file://{}", virtual_file_path.display());
        Some((conn, virtual_uri))
    }

    /// Check if the pool has a connection for the given key.
    pub fn has_connection(&self, key: &str) -> bool {
        self.connections.contains_key(key)
    }

    /// Get the virtual file URI for a connection.
    ///
    /// Returns the stored virtual file URI that was created when the connection
    /// was spawned. This URI is used for textDocument/* requests.
    pub fn get_virtual_uri(&self, key: &str) -> Option<String> {
        self.virtual_uris.get(key).map(|r| r.clone())
    }

    /// Get the notification sender for forwarding $/progress notifications.
    pub fn notification_sender(&self) -> &mpsc::Sender<Value> {
        &self.notification_sender
    }

    /// Get the document version for a URI.
    ///
    /// Returns the current version number if the URI has been opened,
    /// or None if the URI has not been opened yet.
    pub fn get_document_version(&self, uri: &str) -> Option<u32> {
        self.document_versions.get(uri).map(|v| *v)
    }

    /// Set the document version for a URI.
    ///
    /// Used internally to track document versions for didOpen/didChange.
    pub fn set_document_version(&self, uri: &str, version: u32) {
        self.document_versions.insert(uri.to_string(), version);
    }

    /// Atomically increment document version and return the new version.
    ///
    /// Returns 1 for first call (didOpen), increments for subsequent calls (didChange).
    /// This method uses DashMap entry API for atomic read-modify-write to prevent
    /// race conditions where concurrent calls could read the same version.
    fn increment_document_version(&self, uri: &str) -> u32 {
        *self
            .document_versions
            .entry(uri.to_string())
            .and_modify(|v| *v += 1)
            .or_insert(1)
    }

    /// Remove document version tracking when a document is closed.
    /// This prevents unbounded growth of document_versions map.
    pub fn close_document(&self, uri: &str) {
        self.document_versions.remove(uri);
    }

    /// Close a document asynchronously, sending didClose notification to the bridge server.
    ///
    /// This method:
    /// 1. Sends textDocument/didClose notification to the language server
    /// 2. Removes the document from version tracking
    ///
    /// Should be called when a host document is closed to properly clean up
    /// the bridge server's document state.
    pub async fn close_document_async(
        &self,
        conn: &super::tokio_connection::TokioAsyncBridgeConnection,
        uri: &str,
    ) {
        // Only send didClose if the document was actually opened (has version tracking)
        if self.get_document_version(uri).is_some() {
            let params = serde_json::json!({
                "textDocument": {
                    "uri": uri
                }
            });
            // Send didClose notification (ignore errors - connection may be dead)
            let _ = conn
                .send_notification("textDocument/didClose", params)
                .await;
        }
        // Always remove version tracking
        self.close_document(uri);
    }

    /// Get the count of tracked document versions (for testing).
    #[cfg(test)]
    pub fn document_versions_count(&self) -> usize {
        self.document_versions.len()
    }

    /// Close bridge documents associated with a specific host document URI.
    ///
    /// This performs scoped cleanup:
    /// 1. Looks up the bridge virtual URIs associated with the host URI
    /// 2. Removes the host URI from the mapping
    /// 3. For each virtual URI that has no more host URIs using it:
    ///    - Sends didClose to the bridge server
    ///    - Removes version tracking
    ///
    /// This ensures that closing one host document doesn't affect bridge state
    /// for other host documents that may be using the same bridge connection.
    ///
    /// # Arguments
    /// * `host_uri` - The host document URI being closed
    pub async fn close_documents_for_host(&self, host_uri: &str) {
        // Get the bridge URIs for this host and remove the mapping atomically
        let bridge_uris = match self.host_to_bridge_uris.remove(host_uri) {
            Some((_, uris)) => uris,
            None => return, // No bridge documents for this host
        };

        // For each bridge URI, check if any other host still uses it
        for bridge_uri in bridge_uris {
            // Check if any other host URI still references this bridge URI
            let still_in_use = self
                .host_to_bridge_uris
                .iter()
                .any(|entry| entry.value().contains(&bridge_uri));

            if !still_in_use {
                // No other host uses this bridge URI, safe to close
                // Find the connection for this bridge URI
                if let Some((conn, _)) = self
                    .connections
                    .iter()
                    .find_map(|entry| {
                        self.virtual_uris
                            .get(entry.key())
                            .filter(|uri| **uri == bridge_uri)
                            .map(|_| (entry.value().clone(), entry.key().clone()))
                    })
                {
                    self.close_document_async(&conn, &bridge_uri).await;
                }
            }
        }
    }

    /// Close all documents in all active connections.
    ///
    /// This sends textDocument/didClose for each virtual URI and clears all version tracking.
    /// Should be called when a host document is closed to clean up bridge state.
    pub async fn close_all_documents(&self) {
        // Collect connection-uri pairs to avoid holding DashMap locks during async operations
        let conn_uri_pairs: Vec<_> = self
            .connections
            .iter()
            .filter_map(|entry| {
                let key = entry.key().clone();
                let conn = entry.value().clone();
                self.virtual_uris.get(&key).map(|uri| (conn, uri.clone()))
            })
            .collect();

        // Send didClose for each virtual URI
        for (conn, virtual_uri) in conn_uri_pairs {
            self.close_document_async(&conn, &virtual_uri).await;
        }
    }
}

/// High-level async bridge request methods.
///
/// These methods handle the full flow: get/create connection, send didOpen if needed,
/// send the request, and return the response.
impl TokioAsyncLanguageServerPool {
    /// Send a hover request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `host_uri` - Host document URI (for tracking host-to-bridge mapping)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Hover position
    pub async fn hover(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        host_uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::Hover> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Sync document with host URI tracking (didOpen on first access, didChange on subsequent)
        self.sync_document_with_host(&conn, &virtual_uri, language_id, content, host_uri)
            .await?;

        // Send hover request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (id, receiver) = conn.send_request("textDocument/hover", params).await.ok()?;

        // Await response asynchronously with timeout
        let result = match tokio::time::timeout(std::time::Duration::from_secs(30), receiver).await
        {
            Ok(result) => result.ok()?,
            Err(_timeout) => {
                // Clean up pending request on timeout
                conn.remove_pending_request(id);
                return None;
            }
        };

        // Parse response
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a goto definition request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `host_uri` - Host document URI (for tracking host-to-bridge mapping)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Definition position
    pub async fn goto_definition(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        host_uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::GotoDefinitionResponse> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Sync document with host URI tracking (didOpen on first access, didChange on subsequent)
        self.sync_document_with_host(&conn, &virtual_uri, language_id, content, host_uri)
            .await?;

        // Send goto definition request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (id, receiver) = conn
            .send_request("textDocument/definition", params)
            .await
            .ok()?;

        // Await response asynchronously with timeout
        let result = match tokio::time::timeout(std::time::Duration::from_secs(30), receiver).await
        {
            Ok(result) => result.ok()?,
            Err(_timeout) => {
                // Clean up pending request on timeout
                conn.remove_pending_request(id);
                return None;
            }
        };

        // Parse response
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a completion request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `host_uri` - Host document URI (for tracking host-to-bridge mapping)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Completion position
    pub async fn completion(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        host_uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::CompletionResponse> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Sync document with host URI tracking (didOpen on first access, didChange on subsequent)
        self.sync_document_with_host(&conn, &virtual_uri, language_id, content, host_uri)
            .await?;

        // Send completion request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (id, receiver) = conn
            .send_request("textDocument/completion", params)
            .await
            .ok()?;

        // Await response asynchronously with timeout
        let result = match tokio::time::timeout(std::time::Duration::from_secs(30), receiver).await
        {
            Ok(result) => result.ok()?,
            Err(_timeout) => {
                // Clean up pending request on timeout
                conn.remove_pending_request(id);
                return None;
            }
        };

        // Parse response
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a signature help request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `host_uri` - Host document URI (for tracking host-to-bridge mapping)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Signature help position
    pub async fn signature_help(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        host_uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::SignatureHelp> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Sync document with host URI tracking (didOpen on first access, didChange on subsequent)
        self.sync_document_with_host(&conn, &virtual_uri, language_id, content, host_uri)
            .await?;

        // Send signature help request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (id, receiver) = conn
            .send_request("textDocument/signatureHelp", params)
            .await
            .ok()?;

        // Await response asynchronously with timeout
        let result = match tokio::time::timeout(std::time::Duration::from_secs(30), receiver).await
        {
            Ok(result) => result.ok()?,
            Err(_timeout) => {
                // Clean up pending request on timeout
                conn.remove_pending_request(id);
                return None;
            }
        };

        // Parse response
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Sync document content with the language server.
    ///
    /// On first access for a URI: sends didOpen with version 1.
    /// On subsequent access: sends didChange with incremented version.
    ///
    /// This replaces `ensure_document_open` to properly handle the LSP protocol
    /// requirement that didOpen is only sent once per document, and subsequent
    /// content updates use didChange with incrementing versions.
    ///
    /// Uses atomic version increment to prevent race conditions where concurrent
    /// calls could send duplicate version numbers.
    pub async fn sync_document(
        &self,
        conn: &super::tokio_connection::TokioAsyncBridgeConnection,
        uri: &str,
        language_id: &str,
        content: &str,
    ) -> Option<()> {
        // Check if document has been opened (version exists)
        let is_first_access = self.get_document_version(uri).is_none();

        if is_first_access {
            // First time - send didOpen with version 1
            let params = serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": content,
                }
            });
            match conn.send_notification("textDocument/didOpen", params).await {
                Ok(()) => {
                    // Atomically set version to 1 (or keep existing if another call won the race)
                    self.increment_document_version(uri);
                    Some(())
                }
                Err(_) => None,
            }
        } else {
            // Document already open - atomically increment and get new version
            let new_version = self.increment_document_version(uri);
            let params = serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "version": new_version,
                },
                "contentChanges": [{ "text": content }]
            });
            conn.send_notification("textDocument/didChange", params)
                .await
                .ok()
        }
    }

    /// Sync document content with the language server, tracking host-to-bridge URI mapping.
    ///
    /// This is similar to sync_document but also records which host document URI
    /// is associated with which bridge virtual URI. This enables scoped cleanup when
    /// a specific host document is closed.
    ///
    /// # Arguments
    /// * `conn` - Bridge connection
    /// * `uri` - Virtual URI (bridge document)
    /// * `language_id` - Language identifier
    /// * `content` - Document content
    /// * `host_uri` - Host document URI that owns this bridge document
    pub async fn sync_document_with_host(
        &self,
        conn: &super::tokio_connection::TokioAsyncBridgeConnection,
        uri: &str,
        language_id: &str,
        content: &str,
        host_uri: &str,
    ) -> Option<()> {
        // Track the host-to-bridge mapping
        self.host_to_bridge_uris
            .entry(host_uri.to_string())
            .or_insert_with(HashSet::new)
            .insert(uri.to_string());

        // Delegate to existing sync_document
        self.sync_document(conn, uri, language_id, content).await
    }

    /// Get the bridge virtual URIs associated with a host document URI.
    ///
    /// Returns None if the host URI has no associated bridge documents.
    pub fn get_bridge_uris_for_host(&self, host_uri: &str) -> Option<HashSet<String>> {
        self.host_to_bridge_uris
            .get(host_uri)
            .map(|uris| uris.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::config::settings::{BridgeServerConfig, WorkspaceType};
    use crate::lsp::bridge::tokio_connection::TokioAsyncBridgeConnection;
    use std::time::Duration;
    use tokio::sync::mpsc;

    fn check_rust_analyzer_available() -> bool {
        std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok()
    }

    #[cfg(unix)]
    fn short_lived_command() -> (&'static str, &'static [&'static str]) {
        ("/bin/sh", &["-c", "exit 0"])
    }

    #[cfg(windows)]
    fn short_lived_command() -> (&'static str, &'static [&'static str]) {
        ("cmd.exe", &["/C", "exit 0"])
    }

    /// Test that TokioAsyncLanguageServerPool::get_connection returns
    /// Arc<TokioAsyncBridgeConnection> after spawn+initialize.
    ///
    /// This is the key test for Subtask 3: get_connection must:
    /// 1. Spawn process using TokioAsyncBridgeConnection::spawn()
    /// 2. Send initialize request
    /// 3. Wait for response
    /// 4. Send initialized notification
    /// 5. Store virtual_uri
    /// 6. Return Arc<TokioAsyncBridgeConnection>
    #[tokio::test]
    async fn get_connection_returns_arc_tokio_connection_after_initialize() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection (should spawn, initialize, and return Arc<TokioAsyncBridgeConnection>)
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");

        // Second call should return the same connection (not spawn new)
        assert!(
            pool.has_connection("rust-analyzer"),
            "Pool should have connection after get"
        );
    }

    /// Test that pool stores virtual_uri after connection is established.
    #[tokio::test]
    async fn pool_stores_virtual_uri_after_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");

        // Virtual URI should be stored and retrievable
        let virtual_uri = pool.get_virtual_uri("rust-analyzer");
        assert!(
            virtual_uri.is_some(),
            "Virtual URI should be stored after connection"
        );

        let uri = virtual_uri.unwrap();
        assert!(
            uri.starts_with("file://"),
            "Virtual URI should be a file:// URI, got: {}",
            uri
        );
        assert!(
            uri.ends_with(".rs"),
            "Virtual URI should end with .rs for rust, got: {}",
            uri
        );
    }

    /// Test that concurrent gets share the same connection.
    #[tokio::test]
    async fn concurrent_gets_share_same_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = std::sync::Arc::new(super::TokioAsyncLanguageServerPool::new(tx));

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get first connection
        let conn1 = pool.get_connection("rust-analyzer", &config).await;
        // Get second connection - should return the same one
        let conn2 = pool.get_connection("rust-analyzer", &config).await;

        assert!(conn1.is_some() && conn2.is_some());
        // Both should be Arc pointers to the same connection
        assert!(
            std::sync::Arc::ptr_eq(&conn1.unwrap(), &conn2.unwrap()),
            "Concurrent gets should return the same connection"
        );
    }

    /// Test that spawn_and_initialize uses async pattern for workspace setup.
    ///
    /// This verifies AC3: setup_workspace calls use spawn_blocking wrapper to avoid
    /// blocking the tokio runtime on synchronous file I/O operations.
    ///
    /// The test reads the source file and verifies the pattern:
    /// - setup_workspace_with_option call must be inside spawn_blocking
    #[test]
    fn spawn_and_initialize_workspace_setup_uses_async_pattern() {
        let source = include_str!("tokio_async_pool.rs");

        // Find the spawn_and_initialize function
        let spawn_and_initialize_start = source
            .find("async fn spawn_and_initialize")
            .expect("spawn_and_initialize function should exist");

        // Extract just the function body (up to the next pub async fn or end of impl)
        let function_start = &source[spawn_and_initialize_start..];
        // Find end of function - look for next function definition or end marker
        let function_end = function_start
            .find("\n    /// ") // Next doc comment at impl level
            .or_else(|| function_start.find("\n    pub async fn")) // Next pub async method
            .or_else(|| function_start.find("\n    #[allow(dead_code)]\n    pub")) // Next method with allow
            .unwrap_or(function_start.len());

        let function_body = &function_start[..function_end];

        // Verify the function body contains spawn_blocking with setup_workspace_with_option
        // The pattern should be tokio::task::spawn_blocking wrapping the setup call
        assert!(
            function_body.contains("spawn_blocking"),
            "spawn_and_initialize should use spawn_blocking for setup_workspace_with_option.\n\
             Function body:\n{}",
            function_body
        );

        // Also verify setup_workspace_with_option is inside the spawn_blocking closure
        let spawn_blocking_pos = function_body.find("spawn_blocking");
        let setup_pos = function_body.find("setup_workspace_with_option");

        assert!(
            spawn_blocking_pos.is_some() && setup_pos.is_some(),
            "Both spawn_blocking and setup_workspace_with_option should exist in spawn_and_initialize"
        );

        // spawn_blocking should appear before setup_workspace_with_option in the code
        // (the setup call is inside the spawn_blocking closure)
        assert!(
            spawn_blocking_pos.unwrap() < setup_pos.unwrap(),
            "spawn_blocking should wrap setup_workspace_with_option call"
        );
    }

    /// Retry hover request until success or max attempts reached.
    ///
    /// rust-analyzer may return None while indexing. This helper retries
    /// up to 10 times with 500ms delay between attempts.
    async fn hover_with_retry(
        pool: &super::TokioAsyncLanguageServerPool,
        config: &BridgeServerConfig,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::Hover> {
        for _ in 0..10 {
            if let Some(hover) = pool
                .hover(
                    "rust-analyzer",
                    config,
                    "file:///test.rs",
                    "rust",
                    content,
                    position,
                )
                .await
            {
                return Some(hover);
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        None
    }

    /// Retry completion request until success or max attempts reached.
    ///
    /// rust-analyzer may return None while indexing. This helper retries
    /// up to 10 times with 500ms delay between attempts.
    async fn completion_with_retry(
        pool: &super::TokioAsyncLanguageServerPool,
        config: &BridgeServerConfig,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::CompletionResponse> {
        for _ in 0..10 {
            if let Some(completion) = pool
                .completion(
                    "rust-analyzer",
                    config,
                    "file:///test.rs",
                    "rust",
                    content,
                    position,
                )
                .await
            {
                return Some(completion);
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        None
    }

    /// Retry signature help request until success or max attempts reached.
    ///
    /// rust-analyzer may return None while indexing. This helper retries
    /// up to 10 times with 500ms delay between attempts.
    async fn signature_help_with_retry(
        pool: &super::TokioAsyncLanguageServerPool,
        config: &BridgeServerConfig,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::SignatureHelp> {
        for _ in 0..10 {
            if let Some(signature_help) = pool
                .signature_help(
                    "rust-analyzer",
                    config,
                    "file:///test.rs",
                    "rust",
                    content,
                    position,
                )
                .await
            {
                return Some(signature_help);
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        None
    }

    /// Extract string content from HoverContents.
    fn hover_content_to_string(contents: tower_lsp::lsp_types::HoverContents) -> String {
        match contents {
            tower_lsp::lsp_types::HoverContents::Markup(m) => m.value,
            tower_lsp::lsp_types::HoverContents::Scalar(s) => match s {
                tower_lsp::lsp_types::MarkedString::String(s) => s,
                tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value,
            },
            tower_lsp::lsp_types::HoverContents::Array(arr) => arr
                .into_iter()
                .map(|s| match s {
                    tower_lsp::lsp_types::MarkedString::String(s) => s,
                    tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Test that hover() returns Hover from rust-analyzer.
    #[tokio::test]
    async fn hover_returns_hover_from_rust_analyzer() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        let content = "fn main() { let x = 42; }";
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        let hover = hover_with_retry(&pool, &config, content, position).await;

        assert!(
            hover.is_some(),
            "hover() should return Some(Hover) for 'main' function"
        );

        let content_str = hover_content_to_string(hover.unwrap().contents);
        assert!(!content_str.is_empty(), "Hover should contain content");
    }

    /// E2E test: hover returns updated content after document edit.
    #[tokio::test]
    async fn hover_returns_updated_content_after_edit() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // First hover: function returns i32
        let hover1 =
            hover_with_retry(&pool, &config, "fn get_value() -> i32 { 42 }", position).await;
        assert!(hover1.is_some(), "First hover should return result");
        let hover1_content = hover_content_to_string(hover1.unwrap().contents);
        assert!(
            hover1_content.contains("i32"),
            "First hover should show i32 return type, got: {}",
            hover1_content
        );

        // Second hover: function returns String
        let hover2 = hover_with_retry(
            &pool,
            &config,
            "fn get_value() -> String { String::new() }",
            position,
        )
        .await;
        assert!(hover2.is_some(), "Second hover should return result");
        let hover2_content = hover_content_to_string(hover2.unwrap().contents);
        assert!(
            hover2_content.contains("String"),
            "Second hover should show String return type (not i32), got: {}",
            hover2_content
        );
        assert!(
            !hover2_content.contains("i32"),
            "Second hover should NOT show i32 (should be updated), got: {}",
            hover2_content
        );

        // Verify version incremented (may be > 2 due to retries)
        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();
        let version = pool.get_document_version(&virtual_uri).unwrap();
        assert!(
            version >= 2,
            "Version should be at least 2 after two hover calls, got: {}",
            version
        );
    }

    /// Test that subsequent access sends didChange with incremented version.
    ///
    /// Subtask 3: When sync_document is called for a URI that has already been
    /// opened, it should send didChange with incremented version.
    #[tokio::test]
    async fn subsequent_access_sends_did_change_with_incremented_version() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection to establish the virtual URI
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();

        // First access - should send didOpen with version 1
        let content1 = "fn main() {}";
        pool.sync_document(&conn, &virtual_uri, "rust", content1)
            .await;
        assert_eq!(
            pool.get_document_version(&virtual_uri),
            Some(1),
            "Version should be 1 after first access"
        );

        // Second access - should send didChange with version 2
        let content2 = "fn main() { let x = 42; }";
        pool.sync_document(&conn, &virtual_uri, "rust", content2)
            .await;
        assert_eq!(
            pool.get_document_version(&virtual_uri),
            Some(2),
            "Version should be 2 after second access (didChange)"
        );

        // Third access - should send didChange with version 3
        let content3 = "fn main() { let x = 100; }";
        pool.sync_document(&conn, &virtual_uri, "rust", content3)
            .await;
        assert_eq!(
            pool.get_document_version(&virtual_uri),
            Some(3),
            "Version should be 3 after third access (didChange)"
        );
    }

    /// Test that first access sends didOpen with version 1.
    ///
    /// Subtask 2: When ensure_document_open is called for a URI that hasn't been
    /// opened yet, it should send didOpen with version 1 and store version 1.
    #[tokio::test]
    async fn first_access_sends_did_open_with_version_1() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection to establish the virtual URI
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();

        // Before first access, no version should exist
        assert!(
            pool.get_document_version(&virtual_uri).is_none(),
            "No version should exist before first access"
        );

        // Call sync_document (new name for ensure_document_open)
        let content = "fn main() {}";
        pool.sync_document(&conn, &virtual_uri, "rust", content)
            .await;

        // After first access, version should be 1
        assert_eq!(
            pool.get_document_version(&virtual_uri),
            Some(1),
            "Version should be 1 after first access (didOpen)"
        );
    }

    /// Test that pool tracks document versions per URI.
    ///
    /// Subtask 1: TokioAsyncLanguageServerPool should have a document_versions field
    /// that tracks the version number for each virtual URI.
    #[tokio::test]
    async fn pool_tracks_document_versions_per_uri() {
        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        // Pool should have document_versions field accessible via method
        // Initially, no URIs should have versions
        assert!(
            pool.get_document_version("file:///test.rs").is_none(),
            "No version should exist for unknown URI"
        );

        // After setting a version, it should be retrievable
        pool.set_document_version("file:///test.rs", 1);
        assert_eq!(
            pool.get_document_version("file:///test.rs"),
            Some(1),
            "Version 1 should be set for URI"
        );

        // Updating version should work
        pool.set_document_version("file:///test.rs", 2);
        assert_eq!(
            pool.get_document_version("file:///test.rs"),
            Some(2),
            "Version should be updated to 2"
        );

        // Different URIs should have independent versions
        pool.set_document_version("file:///other.rs", 5);
        assert_eq!(
            pool.get_document_version("file:///test.rs"),
            Some(2),
            "First URI version should be unchanged"
        );
        assert_eq!(
            pool.get_document_version("file:///other.rs"),
            Some(5),
            "Second URI should have its own version"
        );
    }

    /// When a notification fails to send, the document version should remain unchanged.
    #[tokio::test]
    async fn sync_document_does_not_set_version_when_notification_fails() {
        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let (command, args) = short_lived_command();
        let conn = TokioAsyncBridgeConnection::spawn(command, args, None, None, None)
            .await
            .expect("short-lived command should spawn");

        // Allow the stub process to exit so writes will fail with BrokenPipe.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let uri = "file:///failing.rs";
        let result = pool.sync_document(&conn, uri, "rust", "fn main() {}").await;

        assert!(
            result.is_none(),
            "sync_document should return None when the notification fails"
        );
        assert!(
            pool.get_document_version(uri).is_none(),
            "Version should remain unset when notification fails"
        );
    }

    /// Test that goto_definition() returns Some(GotoDefinitionResponse) from rust-analyzer.
    ///
    /// This verifies PBI-141 AC1: TokioAsyncLanguageServerPool.goto_definition() method
    /// implemented with async request/response pattern.
    #[tokio::test]
    async fn goto_definition_returns_response_from_rust_analyzer() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Code with a simple function definition
        let content = "fn get_value() -> i32 { 42 }\nfn main() { get_value(); }";
        let position = tower_lsp::lsp_types::Position {
            line: 1,
            character: 13, // Position on 'get_value' call
        };

        let definition = pool
            .goto_definition(
                "rust-analyzer",
                &config,
                "file:///test.rs",
                "rust",
                content,
                position,
            )
            .await;

        assert!(
            definition.is_some(),
            "goto_definition() should return Some(GotoDefinitionResponse) for 'get_value' call"
        );
    }

    /// Test that completion() returns Some(CompletionResponse) from rust-analyzer.
    ///
    /// This verifies PBI-142 AC1: TokioAsyncLanguageServerPool.completion() method
    /// implemented with async request/response pattern.
    #[tokio::test]
    async fn completion_returns_response_from_rust_analyzer() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Code with an incomplete method call to trigger completion
        let content = "fn main() { let s = String::n }";
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 30, // Position after 'n' in 'String::n'
        };

        // Use retry helper to handle indexing delays
        let completion = completion_with_retry(&pool, &config, content, position).await;

        assert!(
            completion.is_some(),
            "completion() should return Some(CompletionResponse) for incomplete 'String::n'"
        );
    }

    /// Test that signature_help() returns Some(SignatureHelp) from rust-analyzer.
    ///
    /// This verifies PBI-143 AC1: TokioAsyncLanguageServerPool.signature_help() method
    /// implemented with async request/response pattern.
    #[tokio::test]
    async fn signature_help_returns_response_from_rust_analyzer() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Code with a function call to trigger signature help
        let content = "fn add(a: i32, b: i32) -> i32 { a + b }\nfn main() { add( }";
        let position = tower_lsp::lsp_types::Position {
            line: 1,
            character: 17, // Position inside 'add(' call
        };

        // Use retry helper to handle indexing delays
        let signature_help = signature_help_with_retry(&pool, &config, content, position).await;

        assert!(
            signature_help.is_some(),
            "signature_help() should return Some(SignatureHelp) for 'add(' call"
        );
    }

    /// Test that concurrent sync_document calls produce monotonically increasing versions without duplicates.
    ///
    /// This verifies PBI-151 AC1: Version atomicity - concurrent calls must produce unique sequential versions.
    /// With the old implementation using separate get/set, concurrent calls could read the same version.
    #[tokio::test]
    async fn concurrent_sync_document_produces_unique_sequential_versions() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = std::sync::Arc::new(super::TokioAsyncLanguageServerPool::new(tx));

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get connection and establish virtual URI
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();

        // Send 10 concurrent sync_document calls
        let mut handles = vec![];
        for i in 0..10 {
            let pool_clone = pool.clone();
            let conn_clone = conn.clone();
            let uri_clone = virtual_uri.clone();
            let content = format!("fn main() {{ let x = {}; }}", i);

            let handle = tokio::spawn(async move {
                pool_clone
                    .sync_document(&conn_clone, &uri_clone, "rust", &content)
                    .await;
                // Return the version number after sync
                pool_clone.get_document_version(&uri_clone)
            });
            handles.push(handle);
        }

        // Collect all versions
        let mut versions = vec![];
        for handle in handles {
            if let Ok(Some(version)) = handle.await {
                versions.push(version);
            }
        }

        // Sort versions to check sequence
        versions.sort();

        // Verify we got all 10 versions
        assert_eq!(versions.len(), 10, "Should have 10 version numbers");

        // Verify versions are 1,2,3,...,10 (no duplicates, sequential)
        let expected: Vec<u32> = (1..=10).collect();
        assert_eq!(
            versions, expected,
            "Versions should be sequential 1-10 without duplicates, got: {:?}",
            versions
        );
    }

    /// Test that concurrent get_connection calls spawn exactly one language server process.
    ///
    /// This verifies PBI-151 AC2: Single spawn - concurrent calls must share a single connection.
    /// With the old implementation using separate check/spawn/insert, concurrent calls could both spawn.
    #[tokio::test]
    async fn concurrent_get_connection_spawns_single_process() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = std::sync::Arc::new(super::TokioAsyncLanguageServerPool::new(tx));

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Send 10 concurrent get_connection calls
        let mut handles = vec![];
        for _ in 0..10 {
            let pool_clone = pool.clone();
            let config_clone = config.clone();

            let handle = tokio::spawn(async move {
                pool_clone
                    .get_connection("rust-analyzer", &config_clone)
                    .await
            });
            handles.push(handle);
        }

        // Collect all connections
        let mut connections = vec![];
        for handle in handles {
            if let Ok(Some(conn)) = handle.await {
                connections.push(conn);
            }
        }

        // Verify we got 10 connections
        assert_eq!(connections.len(), 10, "Should have 10 connection handles");

        // Verify all connections point to the same Arc instance
        let first = &connections[0];
        for conn in &connections[1..] {
            assert!(
                std::sync::Arc::ptr_eq(first, conn),
                "All concurrent get_connection calls should return the same Arc instance"
            );
        }
    }

    /// Test that close_documents_for_host only closes bridge documents for specified host URI.
    ///
    /// PBI-156 Subtask 2: When close_documents_for_host is called for a specific host URI,
    /// it should only close the bridge documents associated with that host, not all bridge documents.
    #[tokio::test]
    async fn close_documents_for_host_only_closes_relevant_bridge_documents() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();
        let host_uri_1 = "file:///test/document1.md";
        let host_uri_2 = "file:///test/document2.md";

        // Open documents from two different host URIs using the same virtual URI
        let content1 = "fn main() {}";
        pool.sync_document_with_host(&conn, &virtual_uri, "rust", content1, host_uri_1)
            .await;

        let content2 = "fn other() {}";
        pool.sync_document_with_host(&conn, &virtual_uri, "rust", content2, host_uri_2)
            .await;

        // Both host URIs should be mapped
        assert!(
            pool.get_bridge_uris_for_host(host_uri_1).is_some(),
            "Host URI 1 should be mapped"
        );
        assert!(
            pool.get_bridge_uris_for_host(host_uri_2).is_some(),
            "Host URI 2 should be mapped"
        );

        // Close documents for host_uri_1 only
        pool.close_documents_for_host(host_uri_1).await;

        // Host URI 1 mapping should be removed
        assert!(
            pool.get_bridge_uris_for_host(host_uri_1).is_none(),
            "Host URI 1 mapping should be removed after close"
        );

        // Host URI 2 mapping should still exist
        assert!(
            pool.get_bridge_uris_for_host(host_uri_2).is_some(),
            "Host URI 2 mapping should still exist"
        );

        // Document version should still exist (because host_uri_2 still uses it)
        assert!(
            pool.get_document_version(&virtual_uri).is_some(),
            "Document version should still exist because host_uri_2 still uses it"
        );
    }

    /// Test that sync_document tracks host-to-bridge URI mapping.
    ///
    /// PBI-156 Subtask 1: When sync_document is called with a host URI,
    /// it should record the mapping from host URI to virtual URI.
    #[tokio::test]
    async fn sync_document_tracks_host_to_bridge_uri_mapping() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();
        let host_uri = "file:///test/document.md";

        // Sync document with host URI
        let content = "fn main() {}";
        pool.sync_document_with_host(&conn, &virtual_uri, "rust", content, host_uri)
            .await;

        // Verify the mapping is tracked
        let bridge_uris = pool.get_bridge_uris_for_host(host_uri);
        assert!(
            bridge_uris.is_some(),
            "Should track bridge URIs for host URI"
        );
        assert!(
            bridge_uris.unwrap().contains(&virtual_uri),
            "Should map host URI to virtual URI"
        );
    }

    /// Test that close_document_async sends didClose notification and removes version tracking.
    ///
    /// PBI-154 AC1-3: When close_document_async is called:
    /// - textDocument/didClose notification is sent to bridge server
    /// - document_versions entry is removed
    #[tokio::test]
    async fn close_document_async_sends_did_close_and_removes_version() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();

        // Open document first
        let content = "fn main() {}";
        pool.sync_document(&conn, &virtual_uri, "rust", content)
            .await;

        // Verify document is tracked
        assert!(
            pool.get_document_version(&virtual_uri).is_some(),
            "Document version should be tracked after sync"
        );

        // Close document - this should send didClose and remove version tracking
        pool.close_document_async(&conn, &virtual_uri).await;

        // Verify version tracking is removed
        assert!(
            pool.get_document_version(&virtual_uri).is_none(),
            "Document version should be removed after close"
        );
    }
}
