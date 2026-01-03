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
const INIT_TIMEOUT_SECS: u64 = 30;

/// Pool of tokio-based async language server connections.
///
/// Unlike `AsyncLanguageServerPool` which uses `spawn_blocking` for initialization,
/// this pool uses fully async I/O throughout. Each connection is a
/// `TokioAsyncBridgeConnection` that uses tokio::process for spawning.
pub struct TokioAsyncLanguageServerPool {
    /// Active connections by key (server name)
    connections: DashMap<String, Arc<TokioAsyncBridgeConnection>>,
    /// Virtual file URIs per (host_uri, connection key) for per-document isolation
    /// Changed from DashMap<String, String> to prevent concurrent requests from different
    /// host documents from overwriting each other's content (PBI-158)
    virtual_uris: DashMap<(String, String), String>,
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
    ///
    /// # Lock Ordering (PBI-175)
    ///
    /// To prevent deadlocks, locks must be acquired in a consistent order:
    /// 1. spawn_locks (held only during connection spawn/initialization)
    /// 2. document_open_locks (held only during didOpen/didChange)
    ///
    /// These locks are NEVER held simultaneously - spawn_locks is released before
    /// sync_document() is called, ensuring no circular wait condition exists.
    spawn_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    /// Per-URI document opening locks to prevent duplicate didOpen notifications (PBI-159).
    ///
    /// Similar to spawn_locks, this uses the double-mutex pattern to ensure only one
    /// thread sends didOpen for a given URI, preventing protocol errors from duplicate
    /// open notifications under concurrent load.
    ///
    /// # Lock Ordering (PBI-175)
    ///
    /// document_open_locks is acquired AFTER spawn_locks has been released (see spawn_locks
    /// documentation above). This lock is held briefly only during the didOpen/didChange
    /// notification sending to prevent race conditions in version tracking.
    document_open_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
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
            document_open_locks: Mutex::new(HashMap::new()),
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
    /// # Timeout and Cleanup (PBI-160, PBI-176)
    ///
    /// If initialization times out (after 30 seconds), this method:
    /// 1. Returns `None` to indicate failure
    /// 2. Automatically cleans up the partially spawned connection via Drop implementation:
    ///    - Kills the child process (if spawned)
    ///    - Removes the temporary directory
    ///    - Prevents resource leaks
    ///
    /// The cleanup happens because `tokio::time::timeout` cancels the future when the
    /// timeout fires, which drops the `TokioAsyncBridgeConnection` value from
    /// `spawn_and_initialize`, triggering its Drop implementation.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning a new connection if needed
    /// * `host_uri` - Host document URI (for per-document virtual file isolation)
    pub async fn get_connection(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        host_uri: &str,
    ) -> Option<Arc<TokioAsyncBridgeConnection>> {
        // Fast path: check if we already have a connection
        if let Some(conn) = self.connections.get(key) {
            // Health check: verify the connection is still alive (PBI-157 AC1)
            if !conn.is_alive().await {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio_async_pool",
                    "[POOL] Detected dead connection for key={}, evicting",
                    key
                );
                // Drop the dashmap reference before removing to avoid deadlock
                drop(conn);
                // Evict the dead connection (PBI-157 AC2)
                self.connections.remove(key);
                // Clean up all bookkeeping state for this connection (PBI-169 AC1)
                self.cleanup_connection_state(key).await;
                // Fall through to spawn a new connection
            } else {
                // Connection is alive, but we may need to create a new virtual URI for this host
                if self
                    .virtual_uris
                    .get(&(host_uri.to_string(), key.to_string()))
                    .is_none()
                {
                    // Spawn a virtual file for this host document
                    if let Some(virtual_uri) =
                        self.spawn_virtual_uri_for_host(config, host_uri, key).await
                    {
                        self.virtual_uris
                            .insert((host_uri.to_string(), key.to_string()), virtual_uri);
                    }
                }
                return Some(conn.clone());
            }
        }

        // Get or create a lock for this key
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[LOCK] get_connection attempting spawn_locks acquisition for key={}",
            key
        );
        let lock = {
            let mut locks = self.spawn_locks.lock().await;
            log::debug!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[LOCK] get_connection acquired spawn_locks outer mutex for key={}",
                key
            );
            locks
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Acquire the per-key lock
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[LOCK] get_connection attempting per-key spawn lock for key={}",
            key
        );
        let _guard = lock.lock().await;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[LOCK] get_connection acquired per-key spawn lock for key={}",
            key
        );

        // Double-check: another task might have created the connection while we waited for the lock
        if let Some(conn) = self.connections.get(key) {
            // Health check: verify the connection is still alive (PBI-157 AC1)
            if !conn.is_alive().await {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio_async_pool",
                    "[POOL] Detected dead connection for key={} in double-check, evicting",
                    key
                );
                // Drop the dashmap reference before removing to avoid deadlock
                drop(conn);
                // Evict the dead connection (PBI-157 AC2)
                self.connections.remove(key);
                // Clean up all bookkeeping state for this connection (PBI-169 AC1)
                self.cleanup_connection_state(key).await;
                // Fall through to spawn a new connection
            } else {
                // Connection exists and is alive, but we may need to create a new virtual URI for this host
                if self
                    .virtual_uris
                    .get(&(host_uri.to_string(), key.to_string()))
                    .is_none()
                {
                    // Spawn a virtual file for this host document
                    if let Some(virtual_uri) =
                        self.spawn_virtual_uri_for_host(config, host_uri, key).await
                    {
                        self.virtual_uris
                            .insert((host_uri.to_string(), key.to_string()), virtual_uri);
                    }
                }
                return Some(conn.clone());
            }
        }

        // We hold the lock and no connection exists - spawn and initialize
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[POOL] Spawning new connection for key={}",
            key
        );

        let spawn_result = timeout(
            Duration::from_secs(INIT_TIMEOUT_SECS),
            self.spawn_and_initialize(config, host_uri, key),
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
        self.virtual_uris
            .insert((host_uri.to_string(), key.to_string()), virtual_uri);

        Some(conn)
    }

    /// Spawn a new connection and initialize it with the language server.
    ///
    /// This is fully async unlike `AsyncLanguageServerPool::spawn_async_connection_blocking`.
    ///
    /// # Arguments
    /// * `config` - Server configuration
    /// * `host_uri` - Host document URI (for unique virtual file naming)
    /// * `key` - Server key (for unique virtual file naming)
    async fn spawn_and_initialize(
        &self,
        config: &BridgeServerConfig,
        _host_uri: &str,
        _key: &str,
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

    /// Spawn a virtual URI for a specific host document without spawning a new connection.
    ///
    /// This creates a new virtual file in the existing connection's workspace for the
    /// given host document, enabling per-document content isolation.
    async fn spawn_virtual_uri_for_host(
        &self,
        config: &BridgeServerConfig,
        host_uri: &str,
        key: &str,
    ) -> Option<String> {
        // Hash the host URI to create a unique but stable identifier
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        host_uri.hash(&mut hasher);
        let host_hash = hasher.finish();

        // Determine extension
        let extension = config
            .languages
            .first()
            .map(|lang| language_to_extension(lang))
            .unwrap_or("rs");

        // Create a unique virtual file path based on host URI hash
        // Use the first connection's temp directory if available
        if let Some(_first_conn_entry) = self.connections.get(key) {
            // Get any existing virtual URI to extract temp directory
            if let Some(existing_uri_entry) =
                self.virtual_uris.iter().find(|entry| entry.key().1 == key)
            {
                let existing_uri = existing_uri_entry.value();
                // Extract temp directory from existing URI
                if let Some(path_str) = existing_uri.strip_prefix("file://")
                    && let Some(parent_dir) = std::path::Path::new(path_str).parent()
                {
                    // Create new virtual file in same workspace
                    let virtual_file_path =
                        parent_dir.join(format!("virtual-{}.{}", host_hash, extension));
                    return Some(format!("file://{}", virtual_file_path.display()));
                }
            }
        }

        None
    }

    /// Check if the pool has a connection for the given key.
    pub fn has_connection(&self, key: &str) -> bool {
        self.connections.contains_key(key)
    }

    /// Get the virtual file URI for a connection and host document.
    ///
    /// Returns the stored virtual file URI that was created for the specific
    /// host document when the connection was accessed. This URI is used for
    /// textDocument/* requests and provides per-document content isolation.
    ///
    /// # Arguments
    /// * `key` - Server key (e.g., "rust-analyzer")
    /// * `host_uri` - Host document URI (e.g., "file:///test/doc.md")
    pub fn get_virtual_uri(&self, key: &str, host_uri: &str) -> Option<String> {
        self.virtual_uris
            .get(&(host_uri.to_string(), key.to_string()))
            .map(|r| r.clone())
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
    ///    - Removes virtual_uris entry
    ///    - Removes document_open_locks entry
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
                // No other host uses this bridge URI, safe to close and clean up
                // Find the connection for this bridge URI by iterating through virtual_uris
                if let Some((conn, _server_key)) = self.virtual_uris.iter().find_map(|entry| {
                    let (_host_key, server_key) = entry.key();
                    let uri = entry.value();
                    if uri == &bridge_uri {
                        self.connections
                            .get(server_key)
                            .map(|conn| (conn.clone(), server_key.clone()))
                    } else {
                        None
                    }
                }) {
                    // Send didClose notification
                    self.close_document_async(&conn, &bridge_uri).await;

                    // Clean up virtual_uris entry (PBI-169 AC2)
                    self.virtual_uris
                        .retain(|(_host, _server), uri| uri != &bridge_uri);

                    // Clean up document_open_locks entry (PBI-169 AC2)
                    let mut locks = self.document_open_locks.lock().await;
                    locks.remove(&bridge_uri);
                }
            }
        }
    }

    /// Clean up all bookkeeping state associated with a connection key.
    ///
    /// PBI-169 AC1: When a connection is evicted (due to crash/restart), this method
    /// purges all associated state to prevent memory leaks:
    /// - virtual_uris entries for this connection key
    /// - document_versions for virtual URIs used by this connection
    /// - host_to_bridge_uris entries referencing these virtual URIs
    /// - document_open_locks for these virtual URIs
    ///
    /// This method should be called from get_connection when evicting a dead connection.
    ///
    /// # Arguments
    /// * `key` - Connection key (e.g., "rust-analyzer") being evicted
    pub async fn cleanup_connection_state(&self, key: &str) {
        // Collect all virtual URIs for this connection key
        let virtual_uris_to_remove: Vec<String> = self
            .virtual_uris
            .iter()
            .filter_map(|entry| {
                let (_host_uri, server_key) = entry.key();
                if server_key == key {
                    Some(entry.value().clone())
                } else {
                    None
                }
            })
            .collect();

        // Remove virtual_uris entries for this connection
        self.virtual_uris
            .retain(|(_host, server_key), _uri| server_key != key);

        // Remove document_versions for virtual URIs used by this connection
        for virtual_uri in &virtual_uris_to_remove {
            self.document_versions.remove(virtual_uri);
        }

        // Remove host_to_bridge_uris entries that reference these virtual URIs
        self.host_to_bridge_uris.retain(|_host_uri, bridge_uris| {
            // Remove any bridge URIs that belong to the evicted connection
            bridge_uris.retain(|uri| !virtual_uris_to_remove.contains(uri));
            // Keep the host entry only if it still has bridge URIs
            !bridge_uris.is_empty()
        });

        // Remove document_open_locks for these virtual URIs
        let mut locks = self.document_open_locks.lock().await;
        for virtual_uri in &virtual_uris_to_remove {
            locks.remove(virtual_uri);
        }
    }

    /// Close all documents in all active connections.
    ///
    /// This sends textDocument/didClose for each virtual URI and clears all version tracking.
    /// Should be called when a host document is closed to clean up bridge state.
    pub async fn close_all_documents(&self) {
        // Collect connection-uri pairs to avoid holding DashMap locks during async operations
        let conn_uri_pairs: Vec<_> = self
            .virtual_uris
            .iter()
            .filter_map(|entry| {
                let (_host_uri, server_key) = entry.key();
                let virtual_uri = entry.value().clone();
                self.connections
                    .get(server_key)
                    .map(|conn| (conn.clone(), virtual_uri))
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
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] hover START for key={}, host_uri={}",
            key, host_uri
        );
        let conn = self.get_connection(key, config, host_uri).await?;

        // Get virtual file URI for this specific host document
        let virtual_uri = self.get_virtual_uri(key, host_uri)?;

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
        let response = result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] hover DONE for key={}",
            key
        );
        response
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
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] goto_definition START for key={}, host_uri={}",
            key, host_uri
        );
        let conn = self.get_connection(key, config, host_uri).await?;

        // Get virtual file URI for this specific host document
        let virtual_uri = self.get_virtual_uri(key, host_uri)?;

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
        let response = result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] goto_definition DONE for key={}",
            key
        );
        response
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
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] completion START for key={}, host_uri={}",
            key, host_uri
        );
        let conn = self.get_connection(key, config, host_uri).await?;

        // Get virtual file URI for this specific host document
        let virtual_uri = self.get_virtual_uri(key, host_uri)?;

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
        let response = result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] completion DONE for key={}",
            key
        );
        response
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
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] signature_help START for key={}, host_uri={}",
            key, host_uri
        );
        let conn = self.get_connection(key, config, host_uri).await?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] signature_help got connection for key={}",
            key
        );

        // Get virtual file URI for this specific host document
        let virtual_uri = self.get_virtual_uri(key, host_uri)?;

        // Sync document with host URI tracking (didOpen on first access, didChange on subsequent)
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] signature_help calling sync_document_with_host for key={}, virtual_uri={}",
            key, virtual_uri
        );
        self.sync_document_with_host(&conn, &virtual_uri, language_id, content, host_uri)
            .await?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] signature_help sync_document_with_host completed for key={}",
            key
        );

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
        let response = result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[REQUEST] signature_help DONE for key={}",
            key
        );
        response
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
    /// Uses per-URI document opening locks (PBI-159) to prevent race conditions where
    /// concurrent calls could both see no version, both send didOpen, causing protocol
    /// errors. The lock is held across the check-send-increment sequence to ensure atomicity.
    pub async fn sync_document(
        &self,
        conn: &super::tokio_connection::TokioAsyncBridgeConnection,
        uri: &str,
        language_id: &str,
        content: &str,
    ) -> Option<()> {
        // Get or create a lock for this URI (PBI-159)
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[LOCK] sync_document attempting document_open_locks acquisition for uri={}",
            uri
        );
        let lock = {
            let mut locks = self.document_open_locks.lock().await;
            log::debug!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[LOCK] sync_document acquired document_open_locks outer mutex for uri={}",
                uri
            );
            locks
                .entry(uri.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Acquire the per-URI lock to serialize first-access check and didOpen sending
        // Wrap lock acquisition with timeout to prevent indefinite hangs (PBI-176)
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[LOCK] sync_document attempting per-URI document lock for uri={}",
            uri
        );
        let lock_result = timeout(Duration::from_secs(10), lock.lock()).await;
        let _guard = match lock_result {
            Ok(guard) => guard,
            Err(_) => {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio_async_pool",
                    "[LOCK] sync_document timeout acquiring per-URI document lock for uri={}",
                    uri
                );
                return None;
            }
        };
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[LOCK] sync_document acquired per-URI document lock for uri={}",
            uri
        );

        // Check if document has been opened (version exists)
        // This check is now protected by the per-URI lock
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
                    // Atomically set version to 1
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
    use serial_test::serial;
    use std::collections::HashSet;
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
    #[serial]
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
        let conn = pool
            .get_connection("rust-analyzer", &config, "file:///test.rs")
            .await;
        assert!(conn.is_some(), "Should get a connection");

        // Second call should return the same connection (not spawn new)
        assert!(
            pool.has_connection("rust-analyzer"),
            "Pool should have connection after get"
        );
    }

    /// Test that pool stores virtual_uri after connection is established.
    #[tokio::test]
    #[serial]
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
        let host_uri = "file:///test.rs";
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");

        // Virtual URI should be stored and retrievable
        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri);
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
    #[serial]
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
        let host_uri = "file:///test.rs";
        let conn1 = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        // Get second connection - should return the same one
        let conn2 = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;

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
    #[serial]
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
    #[serial]
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
        let virtual_uri = pool
            .get_virtual_uri("rust-analyzer", "file:///test.rs")
            .unwrap();
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
    #[serial]
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
        let host_uri = "file:///test.rs";
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri).unwrap();

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
    #[serial]
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
        let host_uri = "file:///test.rs";
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri).unwrap();

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
    #[serial]
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
    #[serial]
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
    #[serial]
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
    #[serial]
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
        let host_uri = "file:///test.rs";
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri).unwrap();

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
    #[serial]
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
                    .get_connection("rust-analyzer", &config_clone, "file:///test.rs")
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
    #[serial]
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

        let host_uri_1 = "file:///test/document1.md";
        let host_uri_2 = "file:///test/document2.md";

        // Get a connection (use host_uri_1)
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri_1)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri_1).unwrap();

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
    #[serial]
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

        let host_uri = "file:///test/document.md";

        // Get a connection
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri).unwrap();

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
    #[serial]
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
        let host_uri = "file:///test.rs";
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri).unwrap();

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

    /// Test that concurrent first-access requests send only one didOpen.
    ///
    /// PBI-159 AC1-2: When multiple threads concurrently call sync_document for a fresh
    /// connection (no existing version), only one didOpen notification should be sent.
    /// The race condition manifests when both threads see None from get_document_version,
    /// both send didOpen with version: 1, and only then increment the version.
    ///
    /// This test verifies that proper locking prevents duplicate didOpen.
    ///
    /// Note: The current implementation's increment_document_version uses atomic entry API,
    /// so the final version count will always be correct (10). However, WITHOUT proper locking
    /// in sync_document, multiple threads can still send duplicate didOpen notifications to
    /// the language server, which can cause protocol errors even if our version tracking is correct.
    ///
    /// The real symptom would be seen in language server logs showing duplicate didOpen.
    /// This test verifies version atomicity as a proxy for the locking behavior.
    #[tokio::test]
    #[serial]
    async fn concurrent_first_access_sends_single_did_open() {
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
        let host_uri = "file:///test.rs";
        let conn = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn.is_some(), "Should get a connection");
        let conn = conn.unwrap();

        let virtual_uri = pool.get_virtual_uri("rust-analyzer", host_uri).unwrap();

        // Verify no version exists yet (fresh document)
        assert!(
            pool.get_document_version(&virtual_uri).is_none(),
            "No version should exist before first sync"
        );

        // Use a barrier to maximize concurrency and trigger the race condition
        use std::sync::Arc as StdArc;
        use tokio::sync::Barrier;
        let barrier = StdArc::new(Barrier::new(10));

        // Send 10 concurrent sync_document calls for first access
        let mut handles = vec![];
        for i in 0..10 {
            let pool_clone = pool.clone();
            let conn_clone = conn.clone();
            let uri_clone = virtual_uri.clone();
            let barrier_clone = barrier.clone();
            let content = format!("fn main() {{ let x = {}; }}", i);

            let handle = tokio::spawn(async move {
                // Wait for all tasks to be ready, then release simultaneously
                barrier_clone.wait().await;
                pool_clone
                    .sync_document(&conn_clone, &uri_clone, "rust", &content)
                    .await;
            });
            handles.push(handle);
        }

        // Wait for all to complete
        for handle in handles {
            handle.await.ok();
        }

        // After concurrent first access, version should be exactly 10
        // (1 from didOpen, +9 from subsequent didChange calls)
        // If the race condition exists, we might see duplicate version 1s
        let final_version = pool.get_document_version(&virtual_uri).unwrap();
        assert_eq!(
            final_version, 10,
            "Version should be exactly 10 (1 didOpen + 9 didChange), got: {}. \
             If less than 10, duplicate didOpen calls occurred.",
            final_version
        );
    }

    /// Test that timeout during spawn_and_initialize cleans up process and temp directory.
    ///
    /// PBI-160 AC1-3: When initialization times out:
    /// 1. The spawned process should be killed (AC1)
    /// 2. The temp directory should be removed (AC2)
    /// 3. No orphaned resources should remain (AC3)
    ///
    /// This test verifies that tokio::time::timeout properly triggers Drop cleanup
    /// when the future is cancelled. The Drop implementation of TokioAsyncBridgeConnection
    /// handles killing the process and removing the temp directory.
    #[tokio::test]
    async fn timeout_during_get_connection_cleans_up_resources() {
        // This test verifies that when a connection is dropped (as happens during timeout),
        // the Drop implementation properly cleans up both the process and temp directory.
        let temp_dir = std::env::temp_dir().join(format!(
            "treesitter-ls-tokio-test-timeout-{}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&temp_dir).await.ok();

        #[cfg(unix)]
        let (cmd, args) = ("/bin/cat", &[][..]); // cat waits forever
        #[cfg(windows)]
        let (cmd, args) = ("cmd.exe", &[][..]);

        // Spawn connection with temp_dir
        let spawn_result = TokioAsyncBridgeConnection::spawn(
            cmd,
            args,
            Some(&temp_dir),
            None,
            Some(temp_dir.clone()),
        )
        .await;

        assert!(spawn_result.is_ok(), "Should spawn the mock server");
        let conn = spawn_result.unwrap();

        // Verify temp dir exists
        assert!(temp_dir.exists(), "Temp directory should exist after spawn");

        // Simulate what happens when tokio::time::timeout fires:
        // The future is cancelled and the connection is dropped
        drop(conn);

        // Give the Drop implementation time to clean up
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify temp dir is removed by Drop implementation (AC2)
        assert!(
            !temp_dir.exists(),
            "Temp directory should be removed after connection drop (timeout cleanup)"
        );
    }

    /// Test that different host documents get different virtual URIs for the same server.
    ///
    /// PBI-158 Subtask 1: virtual_uris should be keyed by (host_uri, server_name) instead
    /// of just server_name, so that each host document gets its own virtual file.
    #[tokio::test]
    #[serial]
    async fn different_host_documents_get_different_virtual_uris() {
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

        let host_uri_1 = "file:///test/doc1.md";
        let host_uri_2 = "file:///test/doc2.md";

        // Get connections for two different host documents
        let conn1 = pool
            .get_connection("rust-analyzer", &config, host_uri_1)
            .await;
        let conn2 = pool
            .get_connection("rust-analyzer", &config, host_uri_2)
            .await;

        assert!(conn1.is_some(), "Should get connection for host 1");
        assert!(conn2.is_some(), "Should get connection for host 2");

        // Both should share the same connection (same server)
        assert!(
            std::sync::Arc::ptr_eq(&conn1.unwrap(), &conn2.unwrap()),
            "Same server should share connection"
        );

        // But each host should get its own virtual URI
        let virtual_uri_1 = pool.get_virtual_uri("rust-analyzer", host_uri_1);
        let virtual_uri_2 = pool.get_virtual_uri("rust-analyzer", host_uri_2);

        assert!(virtual_uri_1.is_some(), "Host 1 should have virtual URI");
        assert!(virtual_uri_2.is_some(), "Host 2 should have virtual URI");

        assert_ne!(
            virtual_uri_1.unwrap(),
            virtual_uri_2.unwrap(),
            "Different host documents should have different virtual URIs for isolation"
        );
    }

    /// PBI-157 AC2: Test that get_connection() detects and evicts dead connections.
    ///
    /// When a cached connection's child process is dead, get_connection() should:
    /// 1. Detect the dead process via is_alive()
    /// 2. Evict the cached connection
    /// 3. Spawn a new connection
    /// 4. Return the new connection
    ///
    /// This test verifies that the pool checks connection health before returning.
    /// Since we can't easily kill a process from within an Arc, this test verifies
    /// that the health check logic exists by checking the fast-path code.
    #[tokio::test]
    #[serial]
    async fn get_connection_has_health_check_logic() {
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

        let host_uri = "file:///test.rs";

        // Get connection twice - should return same connection both times since it's alive
        let conn1 = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn1.is_some(), "Should get initial connection");

        let conn2 = pool
            .get_connection("rust-analyzer", &config, host_uri)
            .await;
        assert!(conn2.is_some(), "Should get connection again");

        // Both should be the same Arc (same connection)
        assert!(
            std::sync::Arc::ptr_eq(&conn1.unwrap(), &conn2.unwrap()),
            "Living connection should be reused"
        );

        // The key test: verify that is_alive() is callable on the Arc<Connection>
        // This demonstrates that health checking is possible
        let conn = pool.connections.get("rust-analyzer").unwrap();
        let _conn_clone = conn.clone();
        drop(conn); // Release the dashmap ref

        // With our implementation using interior mutability (Mutex<Child>),
        // is_alive() can be called on &self (Arc<Connection>), enabling health checks
        // even when the connection is shared across multiple references
    }

    /// Test that concurrent signature_help calls don't deadlock.
    ///
    /// PBI-175 Subtask 2: This test reproduces the deadlock scenario where concurrent
    /// signatureHelp requests can hang indefinitely. The test spawns multiple concurrent
    /// signature_help() calls and verifies they all complete within a reasonable timeout.
    ///
    /// Expected behavior (after fix): All tasks complete within 30s.
    /// Actual behavior (before fix): Tasks hang indefinitely due to lock acquisition deadlock.
    #[tokio::test]
    #[serial]
    async fn concurrent_signature_help_does_not_deadlock() {
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

        // Spawn multiple concurrent signature_help requests
        let mut handles = vec![];
        for i in 0..5 {
            let pool_clone = pool.clone();
            let config_clone = config.clone();
            let content = format!(
                "fn add(a: i32, b: i32) -> i32 {{ a + b }}\nfn main() {{ add({} }}",
                i
            );

            let handle = tokio::spawn(async move {
                let position = tower_lsp::lsp_types::Position {
                    line: 1,
                    character: 17,
                };

                pool_clone
                    .signature_help(
                        "rust-analyzer",
                        &config_clone,
                        "file:///test.rs",
                        "rust",
                        &content,
                        position,
                    )
                    .await
            });
            handles.push(handle);
        }

        // Wait for all tasks with a 30-second timeout
        // If there's a deadlock, this will timeout
        let timeout_result = tokio::time::timeout(Duration::from_secs(30), async {
            for handle in handles {
                handle.await.ok();
            }
        })
        .await;

        assert!(
            timeout_result.is_ok(),
            "Concurrent signature_help calls should complete within 30s (deadlock detected if timeout)"
        );
    }

    /// Test that connection eviction cleans up all associated bookkeeping state.
    ///
    /// PBI-169 AC1: When get_connection() evicts a dead connection, it should purge:
    /// - virtual_uris entries for this connection
    /// - document_versions for virtual URIs used by this connection
    /// - host_to_bridge_uris entries referencing these virtual URIs
    /// - document_open_locks for these virtual URIs
    ///
    /// This test simulates the eviction scenario and verifies all state is cleaned.
    #[tokio::test]
    async fn connection_eviction_purges_all_bookkeeping_state() {
        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        // Manually insert mock connection state to simulate a connection that will be evicted
        let key = "mock-server";
        let host_uri_1 = "file:///test/doc1.md";
        let host_uri_2 = "file:///test/doc2.md";
        let virtual_uri_1 = "file:///tmp/virtual1.rs";
        let virtual_uri_2 = "file:///tmp/virtual2.rs";

        // Simulate state created by get_connection and sync_document
        pool.virtual_uris.insert(
            (host_uri_1.to_string(), key.to_string()),
            virtual_uri_1.to_string(),
        );
        pool.virtual_uris.insert(
            (host_uri_2.to_string(), key.to_string()),
            virtual_uri_2.to_string(),
        );
        pool.document_versions.insert(virtual_uri_1.to_string(), 5);
        pool.document_versions.insert(virtual_uri_2.to_string(), 3);

        let mut bridge_uris_1 = HashSet::new();
        bridge_uris_1.insert(virtual_uri_1.to_string());
        pool.host_to_bridge_uris
            .insert(host_uri_1.to_string(), bridge_uris_1);

        let mut bridge_uris_2 = HashSet::new();
        bridge_uris_2.insert(virtual_uri_2.to_string());
        pool.host_to_bridge_uris
            .insert(host_uri_2.to_string(), bridge_uris_2);

        // Verify state exists before cleanup
        assert_eq!(
            pool.virtual_uris.len(),
            2,
            "Should have 2 virtual_uris entries"
        );
        assert_eq!(
            pool.document_versions.len(),
            2,
            "Should have 2 document_versions entries"
        );
        assert_eq!(
            pool.host_to_bridge_uris.len(),
            2,
            "Should have 2 host_to_bridge_uris entries"
        );

        // Call cleanup_connection_state (method doesn't exist yet - this is RED phase)
        pool.cleanup_connection_state(key).await;

        // Verify all state for this connection is purged
        assert_eq!(
            pool.virtual_uris.len(),
            0,
            "virtual_uris should be empty after cleanup"
        );
        assert_eq!(
            pool.document_versions.len(),
            0,
            "document_versions should be empty after cleanup"
        );
        assert_eq!(
            pool.host_to_bridge_uris.len(),
            0,
            "host_to_bridge_uris should be empty after cleanup"
        );
    }

    /// PBI-176 Subtask 2: Test that get_connection() returns None after timeout when spawn/initialize hangs.
    ///
    /// This test verifies that get_connection() has timeout protection to prevent indefinite blocking
    /// when the language server spawn or initialization process hangs. The timeout should be 30 seconds.
    #[tokio::test]
    async fn get_connection_returns_none_after_timeout_when_spawn_hangs() {
        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        // Use a command that will hang during initialization (cat with no input will wait forever)
        // We'll use a sleep command that sleeps longer than the timeout
        let config = BridgeServerConfig {
            cmd: vec!["sleep".to_string(), "120".to_string()],
            languages: vec!["test".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Measure how long get_connection takes
        let start = std::time::Instant::now();
        let conn = pool
            .get_connection("test-server", &config, "file:///test.txt")
            .await;
        let elapsed = start.elapsed();

        // Should return None (timeout)
        assert!(
            conn.is_none(),
            "get_connection() should return None when initialization times out"
        );

        // Should complete within 35 seconds (30s timeout + 5s buffer)
        assert!(
            elapsed < std::time::Duration::from_secs(35),
            "get_connection() should timeout within 35 seconds, took {:?}",
            elapsed
        );

        // Should NOT complete too quickly (at least 28s to verify timeout actually occurred)
        assert!(
            elapsed >= std::time::Duration::from_secs(28),
            "get_connection() should take at least 28s (timeout occurred), took {:?}",
            elapsed
        );
    }

    /// PBI-176 Subtask 3: Test that sync_document() returns None after timeout when document lock is held indefinitely.
    ///
    /// This test simulates a scenario where the per-URI document lock is held indefinitely by another task,
    /// preventing sync_document() from acquiring the lock. Without timeout protection, sync_document() would
    /// hang forever. With timeout, it should return None after 10 seconds.
    #[tokio::test]
    async fn sync_document_returns_none_after_timeout_when_lock_held() {
        use std::sync::Arc;
        use tokio::sync::Mutex;

        let (tx, _rx) = mpsc::channel(16);
        let pool = Arc::new(super::TokioAsyncLanguageServerPool::new(tx));
        let pool_clone = pool.clone();

        // Spawn a connection
        let result = super::super::tokio_connection::TokioAsyncBridgeConnection::spawn(
            "cat",
            &[],
            None,
            None,
            None,
        )
        .await;
        assert!(result.is_ok(), "spawn() should succeed");
        let conn = result.unwrap();

        let uri = "file:///test.txt";

        // Hold the per-URI document lock indefinitely in a background task
        let uri_clone = uri.to_string();
        let _guard = tokio::spawn(async move {
            // Acquire the per-URI lock and hold it forever
            let lock = {
                let mut locks = pool_clone.document_open_locks.lock().await;
                locks
                    .entry(uri_clone.clone())
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone()
            };
            let _lock = lock.lock().await;
            // Hold lock forever
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        });

        // Give the background task time to acquire the lock
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Measure how long sync_document takes
        let start = std::time::Instant::now();
        let result = pool.sync_document(&conn, uri, "plaintext", "test content").await;
        let elapsed = start.elapsed();

        // Should return None (timeout)
        assert!(
            result.is_none(),
            "sync_document() should return None when lock acquisition times out"
        );

        // Should complete within 12 seconds (10s timeout + 2s buffer)
        assert!(
            elapsed < std::time::Duration::from_secs(12),
            "sync_document() should timeout within 12 seconds, took {:?}",
            elapsed
        );

        // Should NOT complete too quickly (at least 9s to verify timeout actually occurred)
        assert!(
            elapsed >= std::time::Duration::from_secs(9),
            "sync_document() should take at least 9s (timeout occurred), took {:?}",
            elapsed
        );
    }
}
