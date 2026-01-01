//! Async language server connection pool.
//!
//! This module provides `AsyncLanguageServerPool` which manages `AsyncBridgeConnection`
//! instances for concurrent LSP request handling.
//!
//! # Key Difference from LanguageServerPool
//!
//! Unlike `LanguageServerPool` which uses a take/return pattern (where only one
//! caller can use a connection at a time), `AsyncLanguageServerPool` allows
//! multiple concurrent requests to share the same connection.
//!
//! Each connection has a background reader that routes responses by request ID,
//! so concurrent callers don't block each other.

use super::async_connection::AsyncBridgeConnection;
use super::workspace::{language_to_extension, setup_workspace_with_option};
use crate::config::settings::BridgeServerConfig;
use dashmap::DashMap;
use serde_json::Value;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_lsp::lsp_types::*;

/// Pool of async language server connections for concurrent request handling.
///
/// Unlike `LanguageServerPool`, this pool allows multiple concurrent requests
/// to share the same connection. Each connection has a background reader that
/// routes responses by request ID.
pub struct AsyncLanguageServerPool {
    /// Active connections by key (server name)
    connections: DashMap<String, Arc<AsyncBridgeConnection>>,
    /// Virtual file URIs per connection key (for textDocument requests)
    virtual_uris: DashMap<String, String>,
    /// Channel for forwarding $/progress notifications
    notification_sender: mpsc::Sender<Value>,
}

impl AsyncLanguageServerPool {
    /// Create a new pool with a notification channel.
    ///
    /// # Arguments
    /// * `notification_sender` - Channel for forwarding $/progress notifications
    pub fn new(notification_sender: mpsc::Sender<Value>) -> Self {
        Self {
            connections: DashMap::new(),
            virtual_uris: DashMap::new(),
            notification_sender,
        }
    }

    /// Get or create an async connection for the given key.
    ///
    /// Unlike `LanguageServerPool::take_connection`, this returns a shared reference
    /// that multiple callers can use concurrently. The connection is not "taken out"
    /// of the pool - it stays in the pool and serves all concurrent requests.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning a new connection if needed
    pub async fn get_connection(
        &self,
        key: &str,
        config: &BridgeServerConfig,
    ) -> Option<Arc<AsyncBridgeConnection>> {
        // Check if we already have a connection
        if let Some(conn) = self.connections.get(key) {
            return Some(conn.clone());
        }

        // Spawn a new connection in a blocking task (since it does blocking I/O)
        log::debug!(
            target: "treesitter_ls::bridge::async_pool",
            "[POOL] Spawning new connection for key={}",
            key
        );

        let config_clone = config.clone();
        let notif_sender = self.notification_sender.clone();

        let (conn, virtual_uri) = tokio::task::spawn_blocking(move || {
            Self::spawn_async_connection_blocking(&config_clone, notif_sender)
        })
        .await
        .ok()??;

        let conn = Arc::new(conn);
        self.connections.insert(key.to_string(), conn.clone());
        // Store the virtual URI for this connection (used by textDocument methods)
        self.virtual_uris.insert(key.to_string(), virtual_uri);

        Some(conn)
    }

    /// Spawn a new async connection from config (blocking version for spawn_blocking).
    ///
    /// Returns both the connection and the virtual file URI for textDocument requests.
    fn spawn_async_connection_blocking(
        config: &BridgeServerConfig,
        notification_sender: mpsc::Sender<Value>,
    ) -> Option<(AsyncBridgeConnection, String)> {
        let program = config.cmd.first()?;

        // Create temp directory
        static SPAWN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = SPAWN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "treesitter-ls-async-{}-{}-{}",
            program,
            std::process::id(),
            counter
        ));
        std::fs::create_dir_all(&temp_dir).ok()?;

        // Determine extension and setup workspace
        let extension = config
            .languages
            .first()
            .map(|lang| language_to_extension(lang))
            .unwrap_or("rs");

        let virtual_file_path =
            setup_workspace_with_option(&temp_dir, config.workspace_type, extension)?;

        let root_uri = format!("file://{}", temp_dir.display());

        // Build command
        let mut cmd = Command::new(program);
        cmd.current_dir(&temp_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        if config.cmd.len() > 1 {
            cmd.args(&config.cmd[1..]);
        }

        let mut process = cmd.spawn().ok()?;

        let stdin = process.stdin.take()?;
        let stdout = process.stdout.take()?;

        // Create async connection
        let conn = AsyncBridgeConnection::new(stdin, stdout, notification_sender);

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

        // Send initialize and wait for response (blocking during init is OK)
        let (_, receiver) = conn.send_request("initialize", init_params).ok()?;

        // Wait for init response synchronously
        let init_response = receiver.blocking_recv().ok()?;
        init_response.response.as_ref()?;

        // Send initialized notification
        conn.send_notification("initialized", serde_json::json!({}))
            .ok()?;

        log::info!(
            target: "treesitter_ls::bridge::async_pool",
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
}

/// High-level async bridge request methods.
///
/// These methods handle the full flow: get/create connection, send didOpen if needed,
/// send the request, and return the response.
impl AsyncLanguageServerPool {
    /// Send a hover request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `uri` - Document URI
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
        position: Position,
    ) -> Option<Hover> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI from config
        let virtual_uri = self.get_virtual_uri(key)?;

        // Send didOpen/didChange
        self.ensure_document_open(&conn, &virtual_uri, language_id, content)
            .await?;

        // Send hover request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = conn.send_request("textDocument/hover", params).ok()?;

        // Await response asynchronously
        let result = receiver.await.ok()?;

        // Parse response
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Get the virtual file URI for a connection.
    ///
    /// Returns the stored virtual file URI that was created when the connection
    /// was spawned. This URI is used for textDocument/* requests.
    pub fn get_virtual_uri(&self, key: &str) -> Option<String> {
        self.virtual_uris.get(key).map(|r| r.clone())
    }

    /// Ensure a document is open in the language server.
    async fn ensure_document_open(
        &self,
        conn: &AsyncBridgeConnection,
        uri: &str,
        language_id: &str,
        content: &str,
    ) -> Option<()> {
        // TODO: Track document versions per connection
        // For now, always send didOpen
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": content,
            }
        });

        conn.send_notification("textDocument/didOpen", params).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::WorkspaceType;

    fn check_rust_analyzer_available() -> bool {
        std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok()
    }

    #[tokio::test]
    async fn async_pool_can_get_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = AsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // Get a connection (now async)
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");

        // Second call should return the same connection (not spawn new)
        assert!(
            pool.has_connection("rust-analyzer"),
            "Pool should have connection after get"
        );
    }

    #[tokio::test]
    async fn async_pool_concurrent_requests_share_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = Arc::new(AsyncLanguageServerPool::new(tx));

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
            Arc::ptr_eq(&conn1.unwrap(), &conn2.unwrap()),
            "Concurrent gets should return the same connection"
        );
    }

    #[tokio::test]
    async fn async_pool_stores_virtual_uri_after_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = AsyncLanguageServerPool::new(tx);

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
}
