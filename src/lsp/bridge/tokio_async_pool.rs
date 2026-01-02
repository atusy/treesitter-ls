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
use tower_lsp::lsp_types::{Hover, HoverContents, MarkedString};

/// Check if hover content is non-empty.
///
/// A Hover object may exist but contain empty content (e.g., empty MarkupContent
/// or empty array). This function checks that the content is actually meaningful.
fn is_hover_content_non_empty(hover: &Hover) -> bool {
    match &hover.contents {
        HoverContents::Markup(m) => !m.value.is_empty(),
        HoverContents::Scalar(s) => match s {
            MarkedString::String(s) => !s.is_empty(),
            MarkedString::LanguageString(ls) => !ls.value.is_empty(),
        },
        HoverContents::Array(arr) => !arr.is_empty(),
    }
}

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
        let has_non_empty_content = hover_result
            .as_ref()
            .is_some_and(is_hover_content_non_empty);
        if has_non_empty_content && server_state == Some(ServerState::Indexing) {
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

    // =========================================================================
    // Test Helpers
    // =========================================================================

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

    fn rust_analyzer_config() -> BridgeServerConfig {
        BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        }
    }

    fn create_pool() -> super::TokioAsyncLanguageServerPool {
        let (tx, _rx) = mpsc::channel(16);
        super::TokioAsyncLanguageServerPool::new(tx)
    }

    /// Extract text content from HoverContents.
    fn extract_hover_content(hover: &tower_lsp::lsp_types::Hover) -> String {
        match &hover.contents {
            tower_lsp::lsp_types::HoverContents::Markup(m) => m.value.clone(),
            tower_lsp::lsp_types::HoverContents::Scalar(s) => match s {
                tower_lsp::lsp_types::MarkedString::String(s) => s.clone(),
                tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value.clone(),
            },
            tower_lsp::lsp_types::HoverContents::Array(arr) => arr
                .iter()
                .map(|s| match s {
                    tower_lsp::lsp_types::MarkedString::String(s) => s.clone(),
                    tower_lsp::lsp_types::MarkedString::LanguageString(ls) => ls.value.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    /// Retry hover until we get content matching the predicate, or timeout.
    async fn wait_for_hover<F>(
        pool: &super::TokioAsyncLanguageServerPool,
        config: &BridgeServerConfig,
        content: &str,
        position: tower_lsp::lsp_types::Position,
        max_attempts: usize,
        predicate: F,
    ) -> Option<String>
    where
        F: Fn(&str) -> bool,
    {
        for _attempt in 0..max_attempts {
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
                let hover_content = extract_hover_content(&hover);
                if predicate(&hover_content) {
                    return Some(hover_content);
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        None
    }

    // =========================================================================
    // Connection Pool Tests
    // =========================================================================

    #[tokio::test]
    async fn get_connection_returns_arc_after_initialize() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();

        let conn = pool.get_connection("rust-analyzer", &config).await;
        assert!(conn.is_some(), "Should get a connection");
        assert!(
            pool.has_connection("rust-analyzer"),
            "Pool should have connection after get"
        );
    }

    #[tokio::test]
    async fn pool_stores_virtual_uri_after_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();

        pool.get_connection("rust-analyzer", &config).await;

        let uri = pool
            .get_virtual_uri("rust-analyzer")
            .expect("Virtual URI should be stored");
        assert!(
            uri.starts_with("file://"),
            "Should be file:// URI, got: {}",
            uri
        );
        assert!(
            uri.ends_with(".rs"),
            "Should end with .rs for rust, got: {}",
            uri
        );
    }

    #[tokio::test]
    async fn concurrent_gets_share_same_connection() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = std::sync::Arc::new(create_pool());
        let config = rust_analyzer_config();

        let conn1 = pool.get_connection("rust-analyzer", &config).await;
        let conn2 = pool.get_connection("rust-analyzer", &config).await;

        assert!(conn1.is_some() && conn2.is_some());
        assert!(
            std::sync::Arc::ptr_eq(&conn1.unwrap(), &conn2.unwrap()),
            "Concurrent gets should return the same connection"
        );
    }

    // =========================================================================
    // Workspace Setup Tests
    // =========================================================================

    #[test]
    fn spawn_and_initialize_uses_spawn_blocking_for_workspace_setup() {
        let source = include_str!("tokio_async_pool.rs");
        let fn_start = source
            .find("async fn spawn_and_initialize")
            .expect("spawn_and_initialize function should exist");

        let function_body = &source[fn_start..];
        let fn_end = function_body
            .find("\n    /// ")
            .or_else(|| function_body.find("\n    pub async fn"))
            .unwrap_or(function_body.len());
        let function_body = &function_body[..fn_end];

        let spawn_pos = function_body.find("spawn_blocking");
        let setup_pos = function_body.find("setup_workspace_with_option");

        assert!(spawn_pos.is_some() && setup_pos.is_some());
        assert!(
            spawn_pos.unwrap() < setup_pos.unwrap(),
            "spawn_blocking should wrap setup_workspace_with_option"
        );
    }

    // =========================================================================
    // Hover Tests
    // =========================================================================

    #[tokio::test]
    async fn hover_returns_content_from_rust_analyzer() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        let hover_content = wait_for_hover(
            &pool,
            &config,
            "fn main() { let x = 42; }",
            position,
            10,
            |content| !content.is_empty(),
        )
        .await;

        assert!(hover_content.is_some(), "hover() should return content");
    }

    #[tokio::test]
    async fn hover_reflects_document_changes() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // First content with i32 return type
        let hover1 = wait_for_hover(
            &pool,
            &config,
            "fn get_value() -> i32 { 42 }",
            position,
            20,
            |c| !c.contains("indexing") && c.contains("i32"),
        )
        .await
        .expect("Should get i32 hover");

        assert!(
            hover1.contains("i32"),
            "First hover should show i32, got: {}",
            hover1
        );

        // Second content with String return type
        let hover2 = wait_for_hover(
            &pool,
            &config,
            "fn get_value() -> String { String::new() }",
            position,
            20,
            |c| !c.contains("indexing") && c.contains("String"),
        )
        .await
        .expect("Should get String hover");

        assert!(
            hover2.contains("String"),
            "Second hover should show String, got: {}",
            hover2
        );
        assert!(
            !hover2.contains("i32"),
            "Should not show old i32 type, got: {}",
            hover2
        );

        // Verify version incremented
        let virtual_uri = pool.get_virtual_uri("rust-analyzer").unwrap();
        let version = pool.get_document_version(&virtual_uri).unwrap();
        assert!(
            version >= 2,
            "Version should be at least 2, got: {}",
            version
        );
    }

    // =========================================================================
    // Document Sync Tests
    // =========================================================================

    #[tokio::test]
    async fn sync_document_increments_version_on_each_call() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();
        let conn = pool.get_connection("rust-analyzer", &config).await.unwrap();
        let uri = pool.get_virtual_uri("rust-analyzer").unwrap();

        pool.sync_document(&conn, &uri, "rust", "fn main() {}")
            .await;
        assert_eq!(pool.get_document_version(&uri), Some(1));

        pool.sync_document(&conn, &uri, "rust", "fn main() { let x = 42; }")
            .await;
        assert_eq!(pool.get_document_version(&uri), Some(2));

        pool.sync_document(&conn, &uri, "rust", "fn main() { let x = 100; }")
            .await;
        assert_eq!(pool.get_document_version(&uri), Some(3));
    }

    #[tokio::test]
    async fn first_sync_document_sets_version_to_1() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();
        let conn = pool.get_connection("rust-analyzer", &config).await.unwrap();
        let uri = pool.get_virtual_uri("rust-analyzer").unwrap();

        assert!(
            pool.get_document_version(&uri).is_none(),
            "No version before first access"
        );

        pool.sync_document(&conn, &uri, "rust", "fn main() {}")
            .await;

        assert_eq!(
            pool.get_document_version(&uri),
            Some(1),
            "Version should be 1 after didOpen"
        );
    }

    #[tokio::test]
    async fn document_versions_are_independent_per_uri() {
        let pool = create_pool();

        assert!(pool.get_document_version("file:///test.rs").is_none());

        pool.set_document_version("file:///test.rs", 1);
        assert_eq!(pool.get_document_version("file:///test.rs"), Some(1));

        pool.set_document_version("file:///test.rs", 2);
        assert_eq!(pool.get_document_version("file:///test.rs"), Some(2));

        pool.set_document_version("file:///other.rs", 5);
        assert_eq!(pool.get_document_version("file:///test.rs"), Some(2));
        assert_eq!(pool.get_document_version("file:///other.rs"), Some(5));
    }

    #[tokio::test]
    async fn sync_document_does_not_set_version_on_failure() {
        let pool = create_pool();

        let (command, args) = short_lived_command();
        let conn = TokioAsyncBridgeConnection::spawn(command, args, None, None, None)
            .await
            .expect("short-lived command should spawn");

        // Allow process to exit so writes fail
        tokio::time::sleep(Duration::from_millis(50)).await;

        let uri = "file:///failing.rs";
        let result = pool.sync_document(&conn, uri, "rust", "fn main() {}").await;

        assert!(
            result.is_none(),
            "sync_document should return None on failure"
        );
        assert!(
            pool.get_document_version(uri).is_none(),
            "Version should remain unset"
        );
    }

    // =========================================================================
    // Server State Tests (ADR-0010)
    // =========================================================================

    #[tokio::test]
    async fn new_connection_starts_in_indexing_state() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();

        assert!(pool.get_server_state("rust-analyzer").is_none());

        pool.get_connection("rust-analyzer", &config).await;

        assert_eq!(
            pool.get_server_state("rust-analyzer"),
            Some(super::ServerState::Indexing)
        );
    }

    #[tokio::test]
    async fn hover_shows_indexing_message_during_indexing_state() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        // Single immediate hover call (no retries) to catch indexing state
        let hover = pool
            .hover(
                "rust-analyzer",
                &config,
                "file:///test.rs",
                "rust",
                "fn main() {}",
                position,
            )
            .await
            .expect("Should return hover during Indexing");

        let content = extract_hover_content(&hover);
        assert!(
            content.contains("indexing"),
            "Should mention indexing, got: {}",
            content
        );
        assert!(
            content.contains("rust-analyzer"),
            "Should mention server name, got: {}",
            content
        );
    }

    #[tokio::test]
    async fn server_state_can_transition_to_ready() {
        let pool = create_pool();

        pool.set_server_state("test-server", super::ServerState::Indexing);
        assert_eq!(
            pool.get_server_state("test-server"),
            Some(super::ServerState::Indexing)
        );

        pool.set_server_state("test-server", super::ServerState::Ready);
        assert_eq!(
            pool.get_server_state("test-server"),
            Some(super::ServerState::Ready)
        );
    }

    #[tokio::test]
    async fn hover_transitions_state_to_ready_on_real_response() {
        if !check_rust_analyzer_available() {
            eprintln!("Skipping: rust-analyzer not installed");
            return;
        }

        let pool = create_pool();
        let config = rust_analyzer_config();
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 3,
        };

        pool.get_connection("rust-analyzer", &config).await;
        assert_eq!(
            pool.get_server_state("rust-analyzer"),
            Some(super::ServerState::Indexing)
        );

        // Wait for real hover response
        let hover_content = wait_for_hover(
            &pool,
            &config,
            "fn main() { let x = 42; }",
            position,
            20,
            |c| !c.contains("indexing") && c.contains("main"),
        )
        .await;

        assert!(hover_content.is_some(), "Should eventually get real hover");
        assert_eq!(
            pool.get_server_state("rust-analyzer"),
            Some(super::ServerState::Ready)
        );
    }
}
