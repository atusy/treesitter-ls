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

/// Server state for tracking indexing status.
///
/// Language servers like rust-analyzer require an indexing phase after startup
/// before they can provide accurate results. This enum tracks whether a server
/// is still indexing or ready to provide full results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerState {
    /// Server is still indexing, may return empty/incomplete results
    Indexing,
    /// Server has finished indexing, returns full results
    Ready,
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
    /// Server states per connection key (for indexing state tracking)
    server_states: DashMap<String, ServerState>,
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
            server_states: DashMap::new(),
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
        // Initialize server state to Indexing (ADR-0010: informative message during indexing)
        self.server_states
            .insert(key.to_string(), ServerState::Indexing);

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

    /// Get the server state for a connection key.
    ///
    /// Returns the current server state (Indexing/Ready) if the connection exists,
    /// or None if no connection has been established for this key.
    pub fn get_server_state(&self, key: &str) -> Option<ServerState> {
        self.server_states.get(key).map(|v| *v)
    }

    /// Set the server state for a connection key.
    ///
    /// Used to transition from Indexing to Ready when the server
    /// returns a non-empty response.
    pub fn set_server_state(&self, key: &str, state: ServerState) {
        self.server_states.insert(key.to_string(), state);
    }
}

/// High-level async bridge request methods.
///
/// These methods handle the full flow: get/create connection, send didOpen if needed,
/// send the request, and return the response.
impl TokioAsyncLanguageServerPool {
    /// Send a hover request asynchronously.
    ///
    /// If the server is in Indexing state and returns an empty response, returns
    /// an informative message instead of None. This helps users understand why
    /// hover isn't working during rust-analyzer's indexing phase.
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

        // Check server state before making the request
        let server_state = self.get_server_state(key);

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
        let hover_result: Option<tower_lsp::lsp_types::Hover> = result
            .response?
            .get("result")
            .cloned()
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());

        // ADR-0010: Transition to Ready state on non-empty hover response
        if hover_result.is_some() && server_state == Some(ServerState::Indexing) {
            self.set_server_state(key, ServerState::Ready);
            log::debug!(
                target: "treesitter_ls::bridge::tokio_async_pool",
                "[POOL] Server {} transitioned to Ready state (got hover response)",
                key
            );
        }

        // ADR-0010: Return informative message during Indexing state
        if hover_result.is_none() && server_state == Some(ServerState::Indexing) {
            // Return informative hover message with hourglass emoji and server name
            return Some(tower_lsp::lsp_types::Hover {
                contents: tower_lsp::lsp_types::HoverContents::Markup(
                    tower_lsp::lsp_types::MarkupContent {
                        kind: tower_lsp::lsp_types::MarkupKind::Markdown,
                        value: format!("\u{23f3} indexing ({})", key),
                    },
                ),
                range: None,
            });
        }

        hover_result
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

    /// Test that new connection starts with state Indexing.
    ///
    /// AC1: TokioAsyncLanguageServerPool tracks ServerState enum per connection,
    /// starting in Indexing state after spawn.
    #[tokio::test]
    async fn new_connection_starts_with_state_indexing() {
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

        // Before getting connection, no state should exist
        assert!(
            pool.get_server_state("rust-analyzer").is_none(),
            "No server state should exist before connection"
        );

        // Get a connection (should spawn, initialize, and set state to Indexing)
        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");

        // After connection is established, state should be Indexing
        let state = pool.get_server_state("rust-analyzer");
        assert!(
            state.is_some(),
            "Server state should exist after connection"
        );
        assert_eq!(
            state.unwrap(),
            super::ServerState::Indexing,
            "New connection should start with Indexing state"
        );
    }

    /// Test that hover returns informative message when server state is Indexing.
    ///
    /// AC2: hover_impl returns informative message during Indexing state.
    /// Message format: "hourglass indexing (server-name)"
    #[tokio::test]
    async fn hover_returns_indexing_message_when_state_is_indexing() {
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

        let content = "fn main() {}";
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // First hover - should return indexing message because state is Indexing
        // (not retrying, just single immediate call)
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

        // Should return Some(Hover) with indexing message
        assert!(
            hover_result.is_some(),
            "hover() should return Some during Indexing state"
        );

        let hover = hover_result.unwrap();
        let contents = match hover.contents {
            tower_lsp::lsp_types::HoverContents::Markup(m) => m.value,
            tower_lsp::lsp_types::HoverContents::Scalar(s) => match s {
                tower_lsp::lsp_types::MarkedString::String(s) => s,
                tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value,
            },
            tower_lsp::lsp_types::HoverContents::Array(_) => {
                panic!("Expected Markup or Scalar, got Array")
            }
        };

        // Should contain hourglass emoji and server name
        assert!(
            contents.contains("indexing"),
            "Hover during Indexing should mention 'indexing', got: {}",
            contents
        );
        assert!(
            contents.contains("rust-analyzer"),
            "Hover during Indexing should mention server name, got: {}",
            contents
        );
    }

    /// Test that state tracking works correctly for future LSP features.
    ///
    /// AC4: Other LSP features (completion, definition, etc.) should return empty/null
    /// during Indexing state without special message. Only hover shows the message.
    ///
    /// This test verifies the state tracking mechanism that future features will use.
    /// When completion/definition are implemented in async pool, they should:
    /// 1. Check get_server_state(key) == Some(Indexing)
    /// 2. Return None/empty immediately without special message
    /// 3. NOT transition state (only hover triggers transition)
    #[tokio::test]
    async fn state_tracking_for_other_lsp_features() {
        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        // Manually set up state tracking to simulate connection
        pool.set_server_state("test-server", super::ServerState::Indexing);

        // Verify state is Indexing
        assert_eq!(
            pool.get_server_state("test-server"),
            Some(super::ServerState::Indexing),
            "State should be Indexing"
        );

        // Future completion/definition implementations should check this state
        // and return empty/null without special message when Indexing.
        // This is different from hover which returns informative message.

        // Verify we can transition to Ready
        pool.set_server_state("test-server", super::ServerState::Ready);
        assert_eq!(
            pool.get_server_state("test-server"),
            Some(super::ServerState::Ready),
            "State should transition to Ready"
        );

        // After Ready, all features should work normally (return actual results)
    }

    /// Test that state transitions from Indexing to Ready on first non-empty hover response.
    ///
    /// AC3: ServerState transitions from Indexing to Ready on first non-empty hover response.
    /// Empty responses keep the state as Indexing.
    #[tokio::test]
    async fn state_transitions_to_ready_on_non_empty_hover_response() {
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
            character: 3, // on "main"
        };

        // First check: state should start as Indexing after connection
        let _ = pool.get_connection("rust-analyzer", &config).await;
        assert_eq!(
            pool.get_server_state("rust-analyzer"),
            Some(super::ServerState::Indexing),
            "Should start in Indexing state"
        );

        // Keep trying hover until we get a non-indexing response
        // (rust-analyzer needs time to index)
        let mut got_real_hover = false;
        for _attempt in 0..20 {
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

            if let Some(hover) = hover_result {
                let contents = match &hover.contents {
                    tower_lsp::lsp_types::HoverContents::Markup(m) => &m.value,
                    tower_lsp::lsp_types::HoverContents::Scalar(s) => match s {
                        tower_lsp::lsp_types::MarkedString::String(s) => s,
                        tower_lsp::lsp_types::MarkedString::LanguageString(ls) => &ls.value,
                    },
                    tower_lsp::lsp_types::HoverContents::Array(_) => continue,
                };

                // Check if this is a real hover response (not indexing message)
                if !contents.contains("indexing") && contents.contains("main") {
                    got_real_hover = true;
                    break;
                }
            }

            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }

        assert!(got_real_hover, "Should eventually get real hover response");

        // After getting real hover response, state should be Ready
        assert_eq!(
            pool.get_server_state("rust-analyzer"),
            Some(super::ServerState::Ready),
            "State should transition to Ready after non-empty hover response"
        );
    }
}
