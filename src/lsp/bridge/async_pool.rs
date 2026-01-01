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
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tokio::sync::mpsc;
use tower_lsp::lsp_types::*;

/// Wrapper that holds an async connection along with its workspace info.
///
/// This is needed because the connection needs to know the virtual file path
/// for sending didOpen/didChange notifications.
pub struct AsyncConnectionWithInfo {
    /// The async connection for sending requests
    pub connection: AsyncBridgeConnection,
    /// Path to the virtual file in the temp workspace (e.g., src/main.rs)
    pub virtual_file_path: PathBuf,
    /// Current document version for tracking didOpen/didChange
    document_version: std::sync::atomic::AtomicI32,
    /// Hash of last opened content (for detecting content changes and needing index wait)
    last_content_hash: std::sync::atomic::AtomicU64,
}

impl AsyncConnectionWithInfo {
    /// Create a new connection wrapper.
    pub fn new(connection: AsyncBridgeConnection, virtual_file_path: PathBuf) -> Self {
        Self {
            connection,
            virtual_file_path,
            document_version: std::sync::atomic::AtomicI32::new(0),
            last_content_hash: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Get the URI for the virtual file.
    pub fn virtual_file_uri(&self) -> String {
        format!("file://{}", self.virtual_file_path.display())
    }

    /// Send didOpen notification with content.
    ///
    /// This method writes the content to the virtual file on disk first,
    /// then sends the didOpen notification. Writing to disk is required
    /// because rust-analyzer reads files directly in Cargo workspaces.
    ///
    /// Returns `Ok(true)` if the content is new/changed and the server needs indexing time,
    /// `Ok(false)` if the content is the same as before (no indexing needed).
    pub fn did_open(&self, language_id: &str, content: &str) -> Result<bool, String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use std::sync::atomic::Ordering;

        // Compute content hash to detect changes
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let new_hash = hasher.finish();

        let old_hash = self.last_content_hash.swap(new_hash, Ordering::SeqCst);
        let content_changed = old_hash != new_hash;

        // Write content to the virtual file on disk (required for rust-analyzer)
        std::fs::write(&self.virtual_file_path, content)
            .map_err(|e| format!("Failed to write virtual file: {}", e))?;

        let uri = self.virtual_file_uri();
        let version = self
            .document_version
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);

        // Always send didOpen for now (language server handles duplicate opens)
        // A more sophisticated approach would track open state per connection
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": version,
                "text": content,
            }
        });
        self.connection
            .send_notification("textDocument/didOpen", params)?;

        Ok(content_changed)
    }

    /// Send didOpen and wait for server indexing if content changed.
    ///
    /// This is the preferred method for handlers that need the server to be ready
    /// before sending requests (e.g., definition, references, type_definition).
    /// Waits for the server to signal readiness via `publishDiagnostics` notification.
    pub async fn did_open_and_wait(&self, language_id: &str, content: &str) -> Result<(), String> {
        let content_changed = self.did_open(language_id, content)?;

        if content_changed {
            // Reset indexing state and wait for publishDiagnostics
            self.connection.reset_indexing_state();

            // Poll for indexing completion with timeout
            // rust-analyzer sends publishDiagnostics after it finishes indexing
            let start = std::time::Instant::now();
            let timeout = std::time::Duration::from_secs(10);
            let poll_interval = std::time::Duration::from_millis(50);

            while !self.connection.is_ready() {
                if start.elapsed() > timeout {
                    log::warn!(
                        target: "treesitter_ls::bridge::async_pool",
                        "[POOL] Timeout waiting for indexing after {:?}",
                        timeout
                    );
                    break;
                }
                tokio::time::sleep(poll_interval).await;
            }

            log::debug!(
                target: "treesitter_ls::bridge::async_pool",
                "[POOL] Indexing complete after {:?}",
                start.elapsed()
            );
        }

        Ok(())
    }

    /// Send a textDocument/documentHighlight request and await the response.
    pub async fn document_highlight(&self, position: Position) -> Option<Vec<DocumentHighlight>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/documentHighlight", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/hover request and await the response.
    pub async fn hover(&self, position: Position) -> Option<Hover> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/hover", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/signatureHelp request and await the response.
    pub async fn signature_help(&self, position: Position) -> Option<SignatureHelp> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/signatureHelp", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/completion request and await the response.
    pub async fn completion(&self, position: Position) -> Option<CompletionResponse> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/completion", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/definition request and await the response.
    pub async fn goto_definition(&self, position: Position) -> Option<GotoDefinitionResponse> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/definition", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/references request and await the response.
    pub async fn references(
        &self,
        position: Position,
        include_declaration: bool,
    ) -> Option<Vec<Location>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
            "context": { "includeDeclaration": include_declaration },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/references", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/implementation request and await the response.
    pub async fn implementation(&self, position: Position) -> Option<GotoDefinitionResponse> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/implementation", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/typeDefinition request and await the response.
    pub async fn type_definition(&self, position: Position) -> Option<GotoDefinitionResponse> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/typeDefinition", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/rename request and await the response.
    pub async fn rename(&self, position: Position, new_name: &str) -> Option<WorkspaceEdit> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
            "newName": new_name,
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/rename", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/codeAction request and await the response.
    pub async fn code_action(&self, range: Range) -> Option<CodeActionResponse> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character },
            },
            "context": { "diagnostics": [] },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/codeAction", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/formatting request and await the response.
    pub async fn formatting(&self) -> Option<Vec<TextEdit>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "options": {
                "tabSize": 4,
                "insertSpaces": true,
            },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/formatting", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/inlayHint request and await the response.
    pub async fn inlay_hint(&self, range: Range) -> Option<Vec<InlayHint>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": range.start.line, "character": range.start.character },
                "end": { "line": range.end.line, "character": range.end.character },
            },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/inlayHint", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/foldingRange request and await the response.
    pub async fn folding_range(&self) -> Option<Vec<FoldingRange>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/foldingRange", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/documentLink request and await the response.
    pub async fn document_link(&self) -> Option<Vec<DocumentLink>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/documentLink", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/declaration request and await the response.
    pub async fn declaration(&self, position: Position) -> Option<GotoDefinitionResponse> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/declaration", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/prepareCallHierarchy request and await the response.
    pub async fn prepare_call_hierarchy(
        &self,
        position: Position,
    ) -> Option<Vec<CallHierarchyItem>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/prepareCallHierarchy", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a callHierarchy/incomingCalls request and await the response.
    pub async fn incoming_calls(
        &self,
        item: &CallHierarchyItem,
    ) -> Option<Vec<CallHierarchyIncomingCall>> {
        let params = serde_json::json!({
            "item": item,
        });

        let (_, receiver) = self
            .connection
            .send_request("callHierarchy/incomingCalls", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a callHierarchy/outgoingCalls request and await the response.
    pub async fn outgoing_calls(
        &self,
        item: &CallHierarchyItem,
    ) -> Option<Vec<CallHierarchyOutgoingCall>> {
        let params = serde_json::json!({
            "item": item,
        });

        let (_, receiver) = self
            .connection
            .send_request("callHierarchy/outgoingCalls", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a textDocument/prepareTypeHierarchy request and await the response.
    pub async fn prepare_type_hierarchy(
        &self,
        position: Position,
    ) -> Option<Vec<TypeHierarchyItem>> {
        let uri = self.virtual_file_uri();
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = self
            .connection
            .send_request("textDocument/prepareTypeHierarchy", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a typeHierarchy/supertypes request and await the response.
    pub async fn supertypes(&self, item: &TypeHierarchyItem) -> Option<Vec<TypeHierarchyItem>> {
        let params = serde_json::json!({
            "item": item,
        });

        let (_, receiver) = self
            .connection
            .send_request("typeHierarchy/supertypes", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }

    /// Send a typeHierarchy/subtypes request and await the response.
    pub async fn subtypes(&self, item: &TypeHierarchyItem) -> Option<Vec<TypeHierarchyItem>> {
        let params = serde_json::json!({
            "item": item,
        });

        let (_, receiver) = self
            .connection
            .send_request("typeHierarchy/subtypes", params)
            .ok()?;

        let result = receiver.await.ok()?;
        result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok())
    }
}

/// Pool of async language server connections for concurrent request handling.
///
/// Unlike `LanguageServerPool`, this pool allows multiple concurrent requests
/// to share the same connection. Each connection has a background reader that
/// routes responses by request ID.
#[derive(Clone)]
pub struct AsyncLanguageServerPool {
    /// Active connections by key (server name) - includes connection + workspace info
    connections: DashMap<String, Arc<AsyncConnectionWithInfo>>,
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
    ) -> Option<Arc<AsyncConnectionWithInfo>> {
        // Check if we already have a connection
        if let Some(conn) = self.connections.get(key) {
            log::debug!(
                target: "treesitter_ls::bridge::async_pool",
                "[POOL] Reusing existing connection for key={}",
                key
            );
            return Some(conn.clone());
        }

        // Spawn a new connection in a blocking task (since it does blocking I/O)
        log::info!(
            target: "treesitter_ls::bridge::async_pool",
            "[POOL] Spawning new connection for key={}",
            key
        );

        let config_clone = config.clone();
        let notif_sender = self.notification_sender.clone();

        let conn = tokio::task::spawn_blocking(move || {
            Self::spawn_async_connection_blocking(&config_clone, notif_sender)
        })
        .await
        .ok()??;

        let conn = Arc::new(conn);
        self.connections.insert(key.to_string(), conn.clone());

        Some(conn)
    }

    /// Spawn a new async connection from config (blocking version for spawn_blocking).
    fn spawn_async_connection_blocking(
        config: &BridgeServerConfig,
        notification_sender: mpsc::Sender<Value>,
    ) -> Option<AsyncConnectionWithInfo> {
        let program = config.cmd.first()?;

        // Create temp directory
        static SPAWN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = SPAWN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "treesitter-ls-{}-{}-{}",
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
            "[POOL] Connection spawned for {} with virtual file {}",
            program,
            virtual_file_path.display()
        );

        Some(AsyncConnectionWithInfo::new(conn, virtual_file_path))
    }

    /// Check if the pool has a connection for the given key.
    pub fn has_connection(&self, key: &str) -> bool {
        self.connections.contains_key(key)
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
}
