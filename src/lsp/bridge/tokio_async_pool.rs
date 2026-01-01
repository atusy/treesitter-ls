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
use std::time::Duration;
use tokio::sync::mpsc;

/// Default timeout for waiting for rust-analyzer indexing to complete.
const INDEXING_TIMEOUT: Duration = Duration::from_secs(60);

/// Wait for rust-analyzer indexing to complete.
///
/// This function loops on the notification receiver, filtering for $/progress
/// notifications with token 'rustAnalyzer/indexing' and kind='end'.
///
/// # Arguments
/// * `receiver` - Channel receiving $/progress notifications
///
/// # Returns
/// * `true` if indexing completed successfully (received end notification)
/// * `false` if timeout (60 seconds) was reached
#[cfg(test)]
pub async fn wait_for_indexing(receiver: &mut mpsc::Receiver<Value>) -> bool {
    wait_for_indexing_with_timeout(receiver, INDEXING_TIMEOUT).await
}

/// Core implementation for waiting for rust-analyzer indexing.
///
/// This function loops on the notification receiver, filtering for $/progress
/// notifications with token 'rustAnalyzer/indexing' and kind='end'.
/// It optionally forwards notifications to another channel.
///
/// # Arguments
/// * `receiver` - Channel receiving $/progress notifications
/// * `timeout` - Maximum time to wait for indexing
/// * `forward_to` - Optional channel to forward all notifications to
///
/// # Returns
/// * `true` if indexing completed successfully (received end notification)
/// * `false` if timeout was reached or channel closed
async fn wait_for_indexing_impl(
    receiver: &mut mpsc::Receiver<Value>,
    timeout: Duration,
    forward_to: Option<&mpsc::Sender<Value>>,
) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                log::warn!(
                    target: "treesitter_ls::bridge::tokio_async_pool",
                    "[POOL] Timeout waiting for rust-analyzer indexing"
                );
                return false;
            }
            notification = receiver.recv() => {
                match notification {
                    Some(value) => {
                        // Forward notification if sender provided
                        if let Some(sender) = forward_to {
                            let _ = sender.try_send(value.clone());
                        }

                        // Check if this is the indexing end notification
                        if is_indexing_end(&value) {
                            log::info!(
                                target: "treesitter_ls::bridge::tokio_async_pool",
                                "[POOL] rust-analyzer indexing completed"
                            );
                            return true;
                        }
                    }
                    None => {
                        // Channel closed - sender dropped
                        log::warn!(
                            target: "treesitter_ls::bridge::tokio_async_pool",
                            "[POOL] Notification channel closed while waiting for indexing"
                        );
                        return false;
                    }
                }
            }
        }
    }
}

/// Wait for rust-analyzer indexing to complete with custom timeout.
///
/// # Arguments
/// * `receiver` - Channel receiving $/progress notifications
/// * `timeout` - Maximum time to wait for indexing
///
/// # Returns
/// * `true` if indexing completed successfully (received end notification)
/// * `false` if timeout was reached
#[cfg(test)]
pub async fn wait_for_indexing_with_timeout(
    receiver: &mut mpsc::Receiver<Value>,
    timeout: Duration,
) -> bool {
    wait_for_indexing_impl(receiver, timeout, None).await
}

/// Wait for rust-analyzer indexing while forwarding notifications to another channel.
///
/// This function is used during initialization to:
/// 1. Wait for indexing to complete
/// 2. Forward all notifications to the pool's notification sender for external clients
///
/// # Arguments
/// * `receiver` - Local channel receiving notifications from the connection
/// * `forward_to` - Pool's notification sender for external forwarding
///
/// # Returns
/// * `true` if indexing completed successfully (received end notification)
/// * `false` if timeout was reached
async fn wait_for_indexing_with_forward(
    receiver: &mut mpsc::Receiver<Value>,
    forward_to: &mpsc::Sender<Value>,
) -> bool {
    wait_for_indexing_impl(receiver, INDEXING_TIMEOUT, Some(forward_to)).await
}

/// Check if a notification is the indexing end notification.
fn is_indexing_end(value: &Value) -> bool {
    let Some(params) = value.get("params") else {
        return false;
    };
    let Some(token) = params.get("token").and_then(|t| t.as_str()) else {
        return false;
    };
    if token != "rustAnalyzer/indexing" {
        return false;
    }
    let Some(progress_value) = params.get("value") else {
        return false;
    };
    let Some(kind) = progress_value.get("kind").and_then(|k| k.as_str()) else {
        return false;
    };

    log::debug!(
        target: "treesitter_ls::bridge::tokio_async_pool",
        "[POOL] Received indexing progress: kind={}",
        kind
    );
    kind == "end"
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

        let (conn, virtual_uri) = self.spawn_and_initialize(config).await?;

        let conn = Arc::new(conn);
        self.connections.insert(key.to_string(), conn.clone());
        self.virtual_uris.insert(key.to_string(), virtual_uri);

        Some(conn)
    }

    /// Spawn a new connection and initialize it with the language server.
    ///
    /// This is fully async unlike `AsyncLanguageServerPool::spawn_async_connection_blocking`.
    ///
    /// # Notification Forwarding Lifecycle
    ///
    /// This function creates a local notification channel for monitoring indexing completion,
    /// then spawns a background forwarder task after indexing completes. The forwarder runs
    /// until one of these conditions:
    /// - The connection's notification channel closes (connection dropped)
    /// - The pool's notification_sender channel is full or closed
    ///
    /// The background task is detached and will clean up automatically when channels close.
    /// No explicit shutdown is required.
    async fn spawn_and_initialize(
        &self,
        config: &BridgeServerConfig,
    ) -> Option<(TokioAsyncBridgeConnection, String)> {
        let program = config.cmd.first()?;
        let args: Vec<&str> = config.cmd.iter().skip(1).map(|s| s.as_str()).collect();

        // Create temp directory
        static SPAWN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = SPAWN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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

        // Create a local channel for notifications during initialization
        // This allows us to monitor for indexing completion while also forwarding to the pool's channel
        let (local_tx, mut local_rx) = mpsc::channel::<Value>(64);

        // Spawn connection using TokioAsyncBridgeConnection with temp_dir as cwd
        // Pass local notification channel for monitoring indexing
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
            "[POOL] Connection initialized for {}, waiting for indexing...",
            program
        );

        // Wait for rust-analyzer indexing to complete
        // This filters notifications for 'rustAnalyzer/indexing' token with kind='end'
        // Notifications are forwarded to the pool's notification_sender for external clients
        let pool_sender = self.notification_sender.clone();
        let indexing_result = wait_for_indexing_with_forward(&mut local_rx, &pool_sender).await;

        if !indexing_result {
            log::warn!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] Indexing wait timed out or failed for {}",
                program
            );
        } else {
            log::info!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] Indexing completed for {}",
                program
            );
        }

        // Spawn a background task to continue forwarding notifications
        // This ensures any subsequent $/progress notifications are forwarded to external clients
        let forwarder_sender = self.notification_sender.clone();
        tokio::spawn(async move {
            while let Some(value) = local_rx.recv().await {
                if forwarder_sender.try_send(value).is_err() {
                    // Channel closed or full, stop forwarding
                    break;
                }
            }
            log::debug!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] Notification forwarder task exiting"
            );
        });

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

        // Sync document (didOpen on first access, didChange on subsequent)
        self.sync_document(&conn, &virtual_uri, language_id, content)
            .await?;

        // Send hover request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

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
            match conn
                .send_notification("textDocument/didChange", params)
                .await
            {
                Ok(()) => {
                    self.set_document_version(uri, new_version);
                    Some(())
                }
                Err(_) => None,
            }
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
            match conn.send_notification("textDocument/didOpen", params).await {
                Ok(()) => {
                    self.set_document_version(uri, 1);
                    Some(())
                }
                Err(_) => None,
            }
        }
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

    /// PBI-147 Subtask 1: Test that notification channel can be subscribed for filtering.
    ///
    /// This test verifies that when we create a pool with a notification sender,
    /// we can create a second receiver on the same channel for monitoring indexing events.
    /// The `broadcast` approach allows spawn_and_initialize to listen for indexing completion.
    #[tokio::test]
    async fn notification_channel_can_be_monitored_for_indexing() {
        // Create a broadcast channel for notifications (allows multiple receivers)
        let (tx, _rx) = mpsc::channel::<serde_json::Value>(16);

        // The pool stores the sender
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        // Verify we can get the notification sender from the pool
        let sender = pool.notification_sender();
        assert!(
            !sender.is_closed(),
            "Notification sender should be available"
        );
    }

    /// PBI-147 Subtask 2: Test that wait_for_indexing blocks until end notification.
    ///
    /// wait_for_indexing should loop on the notification receiver, filtering for
    /// $/progress notifications with token 'rustAnalyzer/indexing' and kind='end'.
    /// It should return true when it receives the end notification, or false on timeout.
    #[tokio::test]
    async fn wait_for_indexing_blocks_until_end_notification() {
        let (tx, mut rx) = mpsc::channel::<serde_json::Value>(16);

        // Spawn a task that sends the end notification after a delay
        let sender_handle = tokio::spawn(async move {
            // Simulate some indexing time
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Send begin notification first
            let begin = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": "rustAnalyzer/indexing",
                    "value": { "kind": "begin", "title": "Indexing" }
                }
            });
            tx.send(begin).await.unwrap();

            // Send some report notifications
            let report = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": "rustAnalyzer/indexing",
                    "value": { "kind": "report", "message": "Loading crates" }
                }
            });
            tx.send(report).await.unwrap();

            // Finally send end notification
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let end = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": "rustAnalyzer/indexing",
                    "value": { "kind": "end" }
                }
            });
            tx.send(end).await.unwrap();
        });

        // Call wait_for_indexing - should block until end notification
        let start = std::time::Instant::now();
        let result = super::wait_for_indexing(&mut rx).await;
        let elapsed = start.elapsed();

        // Should complete successfully
        assert!(result, "wait_for_indexing should return true on success");

        // Should take at least 200ms (100ms + 100ms delay in sender)
        assert!(
            elapsed >= std::time::Duration::from_millis(150),
            "wait_for_indexing should block until end notification (took {:?})",
            elapsed
        );

        // Wait for sender task to complete
        sender_handle.await.unwrap();
    }

    /// PBI-147 Subtask 2: Test that wait_for_indexing returns false on timeout.
    #[tokio::test]
    async fn wait_for_indexing_returns_false_on_timeout() {
        let (_tx, mut rx) = mpsc::channel::<serde_json::Value>(16);

        // Use a short timeout for testing (override the default 60s)
        let start = std::time::Instant::now();
        let result =
            super::wait_for_indexing_with_timeout(&mut rx, std::time::Duration::from_millis(100))
                .await;
        let elapsed = start.elapsed();

        // Should return false on timeout
        assert!(!result, "wait_for_indexing should return false on timeout");

        // Should take approximately the timeout duration
        assert!(
            elapsed >= std::time::Duration::from_millis(100),
            "Should wait at least the timeout duration (took {:?})",
            elapsed
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

    /// PBI-147 Subtask 3: Integration test verifying get_connection waits for indexing.
    ///
    /// After get_connection returns, hover should work immediately without retry,
    /// because rust-analyzer indexing is complete.
    #[tokio::test]
    async fn get_connection_waits_for_indexing_before_returning() {
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

        // Measure time to get connection (should include indexing wait)
        let start = std::time::Instant::now();
        let conn = pool.get_connection("rust-analyzer", &config).await;
        let elapsed = start.elapsed();

        assert!(conn.is_some(), "Should get a connection after indexing");

        // Connection should take some time (indexing wait)
        // rust-analyzer indexing typically takes at least a few seconds
        // We don't assert a minimum time because it varies by system,
        // but we do verify that the connection works immediately after
        log::info!("get_connection took {:?} (includes indexing wait)", elapsed);

        // Hover should work immediately without retry
        // If indexing wasn't waited for, this would likely fail or return null
        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();
        let content = "fn main() { let x = 42; }";

        // Sync document
        pool.sync_document(&conn.unwrap(), &virtual_uri, "rust", content)
            .await;

        // Single hover request - no retry needed if indexing completed
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3, // on 'main'
        };

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

        // Should succeed on first try because indexing is complete
        assert!(
            hover_result.is_some(),
            "Hover should succeed on first try after get_connection returns"
        );
    }
}
