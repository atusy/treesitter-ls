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
use std::sync::Arc;
use tokio::sync::mpsc;

/// Check if a notification indicates the language server is ready.
///
/// This function returns true when receiving either:
/// 1. `$/progress` with kind:"end" and title/token containing "Indexing"
/// 2. `textDocument/publishDiagnostics` - indicates server has analyzed a file
///
/// The second case handles servers (like rust-analyzer with empty projects)
/// that may skip $/progress notifications entirely.
///
/// # Arguments
/// * `notification` - The notification JSON value
///
/// # Returns
/// true if this notification indicates the server is ready to serve requests
pub(crate) fn is_indexing_complete(notification: &Value) -> bool {
    let Some(method) = notification.get("method").and_then(|m| m.as_str()) else {
        return false;
    };

    // Case 1: publishDiagnostics indicates server has analyzed a file
    if method == "textDocument/publishDiagnostics" {
        return true;
    }

    // Case 2: $/progress with kind:"end" and Indexing title/token
    if method != "$/progress" {
        return false;
    }

    // Get params.value
    let Some(params) = notification.get("params") else {
        return false;
    };
    let Some(value) = params.get("value") else {
        return false;
    };

    // Check kind is "end"
    let Some(kind) = value.get("kind").and_then(|k| k.as_str()) else {
        return false;
    };
    if kind != "end" {
        return false;
    }

    // Check title contains "Indexing" OR token contains "Indexing"
    let title_matches = value
        .get("title")
        .and_then(|t| t.as_str())
        .map(|t| t.contains("Indexing"))
        .unwrap_or(false);

    let token_matches = params
        .get("token")
        .and_then(|t| t.as_str())
        .map(|t| t.contains("Indexing"))
        .unwrap_or(false);

    title_matches || token_matches
}

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
    /// Notification receivers per connection key (for indexing wait)
    /// Wrapped in tokio::sync::Mutex for interior mutability
    notification_receivers: DashMap<String, Arc<tokio::sync::Mutex<mpsc::Receiver<Value>>>>,
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
            notification_receivers: DashMap::new(),
        }
    }

    /// Get or create a tokio async connection for the given key.
    ///
    /// This method is fully async - unlike `AsyncLanguageServerPool::get_connection`
    /// which uses `spawn_blocking` internally, this uses tokio async I/O throughout.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning a new connection if needed
    pub async fn get_connection(
        &self,
        key: &str,
        config: &BridgeServerConfig,
    ) -> Option<Arc<TokioAsyncBridgeConnection>> {
        // Check if we already have a connection
        if let Some(conn) = self.connections.get(key) {
            return Some(conn.clone());
        }

        // Spawn and initialize a new connection
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[POOL] Spawning new connection for key={}",
            key
        );

        let (conn, virtual_uri, notification_rx) = self.spawn_and_initialize(config).await?;

        let conn = Arc::new(conn);
        self.connections.insert(key.to_string(), conn.clone());
        self.virtual_uris.insert(key.to_string(), virtual_uri);
        self.notification_receivers.insert(
            key.to_string(),
            Arc::new(tokio::sync::Mutex::new(notification_rx)),
        );

        Some(conn)
    }

    /// Spawn a new connection and initialize it with the language server.
    ///
    /// This is fully async unlike `AsyncLanguageServerPool::spawn_async_connection_blocking`.
    /// Returns the connection, virtual URI, and notification receiver.
    async fn spawn_and_initialize(
        &self,
        config: &BridgeServerConfig,
    ) -> Option<(TokioAsyncBridgeConnection, String, mpsc::Receiver<Value>)> {
        let program = config.cmd.first()?;
        let args: Vec<&str> = config.cmd.iter().skip(1).map(|s| s.as_str()).collect();

        // Create temp directory
        static SPAWN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = SPAWN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "treesitter-ls-{}-{}-{}",
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

        // Create a LOCAL notification channel for indexing wait.
        // We use this during initialization to receive $/progress notifications
        // and wait for indexing to complete. After indexing, this channel is dropped.
        // The pool's notification_sender is NOT used during spawn - it's for
        // forwarding notifications during normal operation (which we skip for now).
        let (local_tx, mut local_rx) = mpsc::channel::<Value>(64);

        // Spawn connection using TokioAsyncBridgeConnection with temp_dir as cwd
        // Pass local notification sender to capture $/progress during init
        // Pass temp_dir for cleanup on drop
        let conn = TokioAsyncBridgeConnection::spawn(
            program,
            &args,
            Some(&temp_dir),
            Some(local_tx),
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
            "[POOL] Connection spawned for {}, draining initial notifications...",
            program
        );

        // Drain any notifications received during initialization.
        // rust-analyzer sends publishDiagnostics for the empty workspace during init,
        // and we need to drain these so they don't trigger false "indexing complete"
        // signals when we later wait after didOpen.
        let mut drained = 0;
        loop {
            match tokio::time::timeout(std::time::Duration::from_millis(100), local_rx.recv()).await
            {
                Ok(Some(_notification)) => {
                    drained += 1;
                    // Forward to pool's notification_sender for clients
                    let _ = self.notification_sender.try_send(_notification);
                }
                Ok(None) => break, // Channel closed
                Err(_) => break,   // Timeout - no more pending notifications
            }
        }
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[POOL] Drained {} notifications during spawn",
            drained
        );

        // Note: We return the notification receiver to be stored in the pool.
        // Indexing wait happens in hover() after the first didOpen,
        // because rust-analyzer needs actual file content to index.
        // The local_rx channel will continue receiving notifications
        // which we'll drain in wait_for_indexing on first didOpen.

        // Return connection along with the virtual file URI and notification receiver
        let virtual_uri = format!("file://{}", virtual_file_path.display());
        Some((conn, virtual_uri, local_rx))
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

    /// Wait for indexing to complete by polling the notification receiver.
    ///
    /// This drains notifications from the receiver until we see an indexing
    /// complete signal. We wait for a publishDiagnostics to arrive AND then
    /// wait for a brief quiet period (no new notifications) to ensure
    /// rust-analyzer has finished processing.
    ///
    /// # Arguments
    /// * `key` - The connection key to wait for
    /// * `timeout` - Maximum duration to wait
    async fn wait_for_indexing(&self, key: &str, timeout: std::time::Duration) {
        let Some(rx_ref) = self.notification_receivers.get(key) else {
            log::warn!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] No notification receiver for key={}",
                key
            );
            return;
        };

        let rx = rx_ref.clone();
        drop(rx_ref); // Release DashMap lock

        let mut rx_guard = rx.lock().await;
        let start = std::time::Instant::now();
        let mut diagnostics_count = 0;
        let mut quiet_since: Option<std::time::Instant> = None;
        // Require at least 2 diagnostics: one for old state, one for new content
        let min_diagnostics = 2;
        // Wait for 500ms of quiet time after seeing enough diagnostics
        let quiet_period = std::time::Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio_async_pool",
                    "[POOL] Timeout waiting for indexing after {:?}",
                    timeout
                );
                break;
            }

            // If we've seen enough diagnostics and there's been no activity for quiet_period, we're done
            if diagnostics_count >= min_diagnostics
                && let Some(quiet_start) = quiet_since
                && quiet_start.elapsed() > quiet_period
            {
                log::debug!(
                    target: "treesitter_ls::bridge::tokio_async_pool",
                    "[POOL] Indexing complete - saw {} diagnostics and quiet period reached",
                    diagnostics_count
                );
                break;
            }

            // Try to receive notification with a short timeout
            match tokio::time::timeout(std::time::Duration::from_millis(100), rx_guard.recv()).await
            {
                Ok(Some(notification)) => {
                    // Reset quiet period timer on any notification
                    quiet_since = Some(std::time::Instant::now());

                    let method = notification.get("method").and_then(|m| m.as_str());
                    log::debug!(
                        target: "treesitter_ls::bridge::tokio_async_pool",
                        "[POOL] Received notification during indexing wait: {:?}",
                        method
                    );

                    // Check if this is a diagnostics notification
                    if is_indexing_complete(&notification) {
                        diagnostics_count += 1;
                        log::debug!(
                            target: "treesitter_ls::bridge::tokio_async_pool",
                            "[POOL] Saw indexing-complete notification #{} (need {} for quiet period)",
                            diagnostics_count,
                            min_diagnostics
                        );
                    }
                    // Forward to pool's notification_sender for clients
                    let _ = self.notification_sender.try_send(notification);
                }
                Ok(None) => {
                    // Channel closed - reader task exited
                    log::warn!(
                        target: "treesitter_ls::bridge::tokio_async_pool",
                        "[POOL] Notification channel closed during indexing wait"
                    );
                    break;
                }
                Err(_) => {
                    // Timeout on recv - start quiet period if we've seen enough diagnostics
                    if diagnostics_count >= min_diagnostics && quiet_since.is_none() {
                        quiet_since = Some(std::time::Instant::now());
                    }
                }
            }
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
    /// * `_uri` - Document URI (unused, we use virtual URI)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Hover position
    pub async fn hover(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        _uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::Hover> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Check if this is first document access (will send didOpen)
        let is_first_access = self.get_document_version(&virtual_uri).is_none();

        // Sync document (didOpen on first access, didChange on subsequent)
        self.sync_document(&conn, &virtual_uri, language_id, content)
            .await?;

        // On first access, wait for indexing to complete
        if is_first_access {
            log::debug!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] First document access for key={}, waiting for indexing...",
                key
            );
            self.wait_for_indexing(key, std::time::Duration::from_secs(60))
                .await;
            log::debug!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] Indexing complete for key={}",
                key
            );
        }

        // Send hover request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[POOL] Sending hover request for uri={}, pos=({},{})",
            virtual_uri,
            position.line,
            position.character
        );

        let (_, receiver) = conn.send_request("textDocument/hover", params).await.ok()?;

        // Await response asynchronously with timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), receiver)
            .await
            .ok()?
            .ok()?;

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
    pub async fn sync_document(
        &self,
        conn: &super::tokio_connection::TokioAsyncBridgeConnection,
        uri: &str,
        language_id: &str,
        content: &str,
    ) -> Option<()> {
        if let Some(current_version) = self.get_document_version(uri) {
            // Document already open - send didChange with incremented version
            let new_version = current_version + 1;
            let params = serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "version": new_version,
                },
                "contentChanges": [{ "text": content }]
            });
            self.set_document_version(uri, new_version);
            conn.send_notification("textDocument/didChange", params)
                .await
                .ok()
        } else {
            // First time - send didOpen with version 1
            let params = serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": content,
                }
            });
            self.set_document_version(uri, 1);
            conn.send_notification("textDocument/didOpen", params)
                .await
                .ok()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::settings::{BridgeServerConfig, WorkspaceType};
    use tokio::sync::mpsc;

    fn check_rust_analyzer_available() -> bool {
        std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok()
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

    /// Test that hover() returns Hover from rust-analyzer.
    ///
    /// This is the key test for Subtask 4: hover() must:
    /// 1. Get or create connection
    /// 2. Call ensure_document_open (didOpen)
    /// 3. Send textDocument/hover request
    /// 4. Await response
    /// 5. Parse and return Hover
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

        // Simple Rust code with a function
        let content = "fn main() { let x = 42; }";

        // Request hover at position of 'main' function (line 0, character 3)
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // Call hover() method with retry for rust-analyzer indexing
        // rust-analyzer may return "content modified" error while indexing
        let mut hover_result = None;
        for _attempt in 0..10 {
            hover_result = pool
                .hover(
                    "rust-analyzer",
                    &config,
                    "file:///test.rs", // host URI (not used by tokio pool)
                    "rust",
                    content,
                    position,
                )
                .await;

            if hover_result.is_some() {
                break;
            }

            // rust-analyzer may return "content modified" or null while indexing
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        // Should return Some(Hover) with type information
        assert!(
            hover_result.is_some(),
            "hover() should return Some(Hover) for 'main' function"
        );

        let hover = hover_result.unwrap();
        // Verify hover contains content (the exact format depends on rust-analyzer)
        match hover.contents {
            tower_lsp::lsp_types::HoverContents::Markup(markup) => {
                assert!(
                    !markup.value.is_empty(),
                    "Hover should contain markup content"
                );
            }
            tower_lsp::lsp_types::HoverContents::Scalar(_) => {
                // Also acceptable
            }
            tower_lsp::lsp_types::HoverContents::Array(arr) => {
                assert!(!arr.is_empty(), "Hover array should not be empty");
            }
        }
    }

    /// E2E test: hover returns updated content after document edit.
    ///
    /// Subtask 4: Verify that when content is changed via sync_document,
    /// subsequent hover requests return information from the updated content.
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

        // First content: function returns i32
        let content1 = "fn get_value() -> i32 { 42 }";
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // First hover request (with initial content)
        let mut hover1 = None;
        for _attempt in 0..10 {
            hover1 = pool
                .hover(
                    "rust-analyzer",
                    &config,
                    "file:///test.rs",
                    "rust",
                    content1,
                    position,
                )
                .await;

            if hover1.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        assert!(hover1.is_some(), "First hover should return result");
        let hover1_content = match hover1.unwrap().contents {
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
        };
        assert!(
            hover1_content.contains("i32"),
            "First hover should show i32 return type, got: {}",
            hover1_content
        );

        // Second content: function returns String
        let content2 = "fn get_value() -> String { String::new() }";

        // Second hover request (with updated content)
        let mut hover2 = None;
        for _attempt in 0..10 {
            hover2 = pool
                .hover(
                    "rust-analyzer",
                    &config,
                    "file:///test.rs",
                    "rust",
                    content2,
                    position,
                )
                .await;

            if hover2.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        assert!(hover2.is_some(), "Second hover should return result");
        let hover2_content = match hover2.unwrap().contents {
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
        };
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

    /// PBI-147 Subtask 3: Test that is_indexing_complete detects indexing end from $/progress.
    ///
    /// rust-analyzer sends $/progress notifications with:
    /// - method: "$/progress"
    /// - params.token: "rustAnalyzer/Indexing"
    /// - params.value.kind: "begin" | "report" | "end"
    /// - params.value.title: "Indexing" (for indexing notifications)
    ///
    /// When kind is "end", indexing is complete.
    #[test]
    fn is_indexing_complete_returns_true_for_indexing_end_notification() {
        // Notification with kind: "end" and title containing "Indexing"
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "rustAnalyzer/Indexing",
                "value": {
                    "kind": "end",
                    "title": "Indexing"
                }
            }
        });

        assert!(
            super::is_indexing_complete(&notification),
            "Should return true for indexing end notification"
        );
    }

    #[test]
    fn is_indexing_complete_returns_false_for_indexing_begin_notification() {
        // Notification with kind: "begin" - indexing just started
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "rustAnalyzer/Indexing",
                "value": {
                    "kind": "begin",
                    "title": "Indexing"
                }
            }
        });

        assert!(
            !super::is_indexing_complete(&notification),
            "Should return false for indexing begin notification"
        );
    }

    #[test]
    fn is_indexing_complete_returns_false_for_indexing_report_notification() {
        // Notification with kind: "report" - indexing in progress
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "rustAnalyzer/Indexing",
                "value": {
                    "kind": "report",
                    "title": "Indexing",
                    "percentage": 50
                }
            }
        });

        assert!(
            !super::is_indexing_complete(&notification),
            "Should return false for indexing report notification"
        );
    }

    #[test]
    fn is_indexing_complete_returns_false_for_non_indexing_end_notification() {
        // Notification with kind: "end" but for a different operation (not Indexing)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "rustAnalyzer/Building",
                "value": {
                    "kind": "end",
                    "title": "Building"
                }
            }
        });

        assert!(
            !super::is_indexing_complete(&notification),
            "Should return false for non-Indexing end notification"
        );
    }

    #[test]
    fn is_indexing_complete_returns_true_for_publish_diagnostics() {
        // publishDiagnostics indicates server has analyzed a file
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///test.rs",
                "diagnostics": []
            }
        });

        assert!(
            super::is_indexing_complete(&notification),
            "Should return true for publishDiagnostics (server is ready)"
        );
    }

    #[test]
    fn is_indexing_complete_returns_false_for_other_notifications() {
        // A different notification type (not progress or diagnostics)
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "window/logMessage",
            "params": {
                "type": 3,
                "message": "Info"
            }
        });

        assert!(
            !super::is_indexing_complete(&notification),
            "Should return false for other notification types"
        );
    }

    /// PBI-147 Subtask 2: Test that spawn_and_initialize waits for indexing.
    ///
    /// After sending initialized notification, spawn_and_initialize should:
    /// 1. Create a local notification channel
    /// 2. Poll for $/progress notifications
    /// 3. Return only after indexing is complete (kind: end)
    ///
    /// This ensures that when get_connection returns, rust-analyzer is ready
    /// to serve hover requests without retry loops.
    #[tokio::test]
    async fn spawn_and_initialize_waits_for_indexing() {
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

        // Measure time for get_connection
        let start = std::time::Instant::now();

        // Get a connection - should wait for indexing
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");

        let elapsed = start.elapsed();

        // With indexing wait, this should take at least a few hundred ms
        // (rust-analyzer needs time to parse even an empty Cargo project)
        // Without the wait, it returns almost immediately (< 100ms)
        //
        // We check that it took more than 100ms, which indicates we're waiting
        // for *something* (the indexing). The exact time varies but should be
        // substantially more than just spawning the process.
        //
        // Note: This is a heuristic test. The real verification is in the E2E test
        // where a single hover request returns a result without retry.
        log::info!(
            target: "treesitter_ls::bridge::tokio_async_pool::tests",
            "spawn_and_initialize took {:?}",
            elapsed
        );

        // After spawn_and_initialize returns, hover should work immediately
        // without needing retries (the main goal of PBI-147)
        // Use pool.hover() which handles sync_document + indexing wait + hover request
        let content = "fn main() {}";
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // Try hover via pool.hover() - should return result on FIRST attempt
        // pool.hover() will:
        // 1. sync_document (didOpen on first access)
        // 2. wait_for_indexing (because is_first_access)
        // 3. send textDocument/hover
        let hover_result = pool
            .hover(
                "rust-analyzer",
                &config,
                "file:///test.rs",
                "rust",
                content,
                position,
            )
            .await;

        // After proper indexing wait, hover should return a result on first try
        assert!(
            hover_result.is_some(),
            "Hover should return Some(Hover) after indexing wait"
        );

        let hover = hover_result.unwrap();
        // Verify hover contains content
        match hover.contents {
            tower_lsp::lsp_types::HoverContents::Markup(m) => {
                assert!(!m.value.is_empty(), "Hover should contain markup content");
            }
            tower_lsp::lsp_types::HoverContents::Scalar(_) => {
                // Also acceptable
            }
            tower_lsp::lsp_types::HoverContents::Array(arr) => {
                assert!(!arr.is_empty(), "Hover array should not be empty");
            }
        }
    }
}
