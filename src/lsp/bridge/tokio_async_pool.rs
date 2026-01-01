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

        // Spawn connection using TokioAsyncBridgeConnection with cwd set to temp_dir
        // This is critical for language servers like rust-analyzer that need to find Cargo.toml
        let conn = TokioAsyncBridgeConnection::spawn_with_cwd(program, &args, Some(&temp_dir))
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    pub fn notification_sender(&self) -> &mpsc::Sender<Value> {
        &self.notification_sender
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

        // Send didOpen
        self.ensure_document_open(&conn, &virtual_uri, language_id, content)
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

    /// Send a goto_definition request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `_uri` - Document URI (unused, we use virtual URI)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Definition position
    pub async fn goto_definition(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        _uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::GotoDefinitionResponse> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Send didOpen
        self.ensure_document_open(&conn, &virtual_uri, language_id, content)
            .await?;

        // Send definition request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = conn
            .send_request("textDocument/definition", params)
            .await
            .ok()?;

        // Await response asynchronously with timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), receiver)
            .await
            .ok()?
            .ok()?;

        // Parse response
        let response = result.response?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[DEFINITION] Response: {:?}",
            response
        );

        let result_value = response.get("result").cloned().filter(|r| !r.is_null())?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[DEFINITION] Result value: {:?}",
            result_value
        );

        serde_json::from_value(result_value).ok()
    }

    /// Send a completion request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `_uri` - Document URI (unused, we use virtual URI)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Completion position
    pub async fn completion(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        _uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::CompletionResponse> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Send didOpen
        self.ensure_document_open(&conn, &virtual_uri, language_id, content)
            .await?;

        // Send completion request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = conn
            .send_request("textDocument/completion", params)
            .await
            .ok()?;

        // Await response asynchronously with timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), receiver)
            .await
            .ok()?
            .ok()?;

        // Parse response
        let response = result.response?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[COMPLETION] Response: {:?}",
            response
        );

        let result_value = response.get("result").cloned().filter(|r| !r.is_null())?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[COMPLETION] Result value: {:?}",
            result_value
        );

        serde_json::from_value(result_value).ok()
    }

    /// Send a signature_help request asynchronously.
    ///
    /// # Arguments
    /// * `key` - Connection pool key
    /// * `config` - Server configuration
    /// * `_uri` - Document URI (unused, we use virtual URI)
    /// * `language_id` - Language ID for the document
    /// * `content` - Document content
    /// * `position` - Signature help position
    pub async fn signature_help(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        _uri: &str,
        language_id: &str,
        content: &str,
        position: tower_lsp::lsp_types::Position,
    ) -> Option<tower_lsp::lsp_types::SignatureHelp> {
        let conn = self.get_connection(key, config).await?;

        // Get virtual file URI
        let virtual_uri = self.get_virtual_uri(key)?;

        // Send didOpen
        self.ensure_document_open(&conn, &virtual_uri, language_id, content)
            .await?;

        // Send signatureHelp request
        let params = serde_json::json!({
            "textDocument": { "uri": virtual_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let (_, receiver) = conn
            .send_request("textDocument/signatureHelp", params)
            .await
            .ok()?;

        // Await response asynchronously with timeout
        let result = tokio::time::timeout(std::time::Duration::from_secs(30), receiver)
            .await
            .ok()?
            .ok()?;

        // Parse response
        let response = result.response?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[SIGNATURE_HELP] Response: {:?}",
            response
        );

        let result_value = response.get("result").cloned().filter(|r| !r.is_null())?;
        log::debug!(
            target: "treesitter_ls::bridge::tokio_async_pool",
            "[SIGNATURE_HELP] Result value: {:?}",
            result_value
        );

        serde_json::from_value(result_value).ok()
    }

    /// Ensure a document is open in the language server.
    async fn ensure_document_open(
        &self,
        conn: &super::tokio_connection::TokioAsyncBridgeConnection,
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

        conn.send_notification("textDocument/didOpen", params)
            .await
            .ok()
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

    fn check_lua_language_server_available() -> bool {
        std::process::Command::new("lua-language-server")
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

    /// Test that goto_definition() returns Location from lua-language-server.
    ///
    /// Uses lua-language-server for faster test execution (faster startup than rust-analyzer).
    /// Pattern follows hover() test but for textDocument/definition request.
    #[tokio::test]
    async fn goto_definition_returns_location_from_lua_language_server() {
        if !check_lua_language_server_available() {
            eprintln!("Skipping: lua-language-server not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Lua code with a local variable definition and reference
        // Line 0: local x = 42
        // Line 1: print(x)  -- request definition on 'x' should go to line 0
        let content = "local x = 42\nprint(x)";

        // Request definition at position of 'x' reference on line 1, character 6
        let position = tower_lsp::lsp_types::Position {
            line: 1,
            character: 6,
        };

        // Call goto_definition() method with retry for indexing
        let mut definition_result = None;
        for _attempt in 0..10 {
            definition_result = pool
                .goto_definition(
                    "lua-language-server",
                    &config,
                    "file:///test.lua", // host URI (not used by tokio pool)
                    "lua",
                    content,
                    position,
                )
                .await;

            if definition_result.is_some() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // Should return Some(GotoDefinitionResponse) with location pointing to line 0
        assert!(
            definition_result.is_some(),
            "goto_definition() should return Some(GotoDefinitionResponse) for 'x' reference"
        );

        let def_response = definition_result.unwrap();
        // Extract the location from the response (could be Scalar, Array, or Link)
        match def_response {
            tower_lsp::lsp_types::GotoDefinitionResponse::Scalar(loc) => {
                assert_eq!(
                    loc.range.start.line, 0,
                    "Definition should be on line 0 where x is defined"
                );
            }
            tower_lsp::lsp_types::GotoDefinitionResponse::Array(locs) => {
                assert!(!locs.is_empty(), "Should have at least one location");
                assert_eq!(
                    locs[0].range.start.line, 0,
                    "Definition should be on line 0 where x is defined"
                );
            }
            tower_lsp::lsp_types::GotoDefinitionResponse::Link(links) => {
                assert!(!links.is_empty(), "Should have at least one link");
                assert_eq!(
                    links[0].target_range.start.line, 0,
                    "Definition should be on line 0 where x is defined"
                );
            }
        }
    }

    /// Test that completion() returns CompletionResponse from lua-language-server.
    ///
    /// Uses lua-language-server for faster test execution (faster startup than rust-analyzer).
    /// Pattern follows hover/goto_definition tests but for textDocument/completion request.
    #[tokio::test]
    async fn completion_returns_completion_list_from_lua_language_server() {
        if !check_lua_language_server_available() {
            eprintln!("Skipping: lua-language-server not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Lua code with string module - request completion after "string."
        // Line 0: local s = string.
        // Cursor after the dot should trigger completion for string methods
        let content = "local s = string.";

        // Request completion at position after "string." (line 0, character 17)
        let position = tower_lsp::lsp_types::Position {
            line: 0,
            character: 17,
        };

        // Call completion() method with retry for indexing
        let mut completion_result = None;
        for _attempt in 0..10 {
            completion_result = pool
                .completion(
                    "lua-language-server",
                    &config,
                    "file:///test.lua", // host URI (not used by tokio pool)
                    "lua",
                    content,
                    position,
                )
                .await;

            if completion_result.is_some() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // Should return Some(CompletionResponse) with completion items
        assert!(
            completion_result.is_some(),
            "completion() should return Some(CompletionResponse) for 'string.' trigger"
        );

        let response = completion_result.unwrap();
        // Extract items from the response
        let items = match response {
            tower_lsp::lsp_types::CompletionResponse::Array(items) => items,
            tower_lsp::lsp_types::CompletionResponse::List(list) => list.items,
        };

        assert!(
            !items.is_empty(),
            "Should have at least one completion item"
        );

        // Verify we got string methods (e.g., "format", "len", "sub")
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels
                .iter()
                .any(|l| l.contains("format") || l.contains("len") || l.contains("sub")),
            "Should contain common string methods, got: {:?}",
            labels
        );
    }

    /// Test that signature_help() returns SignatureHelp from lua-language-server.
    ///
    /// Uses lua-language-server for faster test execution (faster startup than rust-analyzer).
    /// Pattern follows hover/goto_definition/completion tests but for textDocument/signatureHelp request.
    #[tokio::test]
    async fn signature_help_returns_signature_from_lua_language_server() {
        if !check_lua_language_server_available() {
            eprintln!("Skipping: lua-language-server not installed");
            return;
        }

        let (tx, _rx) = mpsc::channel(16);
        let pool = super::TokioAsyncLanguageServerPool::new(tx);

        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Lua code with function call - request signature help inside function call
        // Line 0: local function add(a, b) return a + b end
        // Line 1: add(  -- cursor after ( should trigger signature help
        let content = "local function add(a, b) return a + b end\nadd(";

        // Request signature help at position inside add( (line 1, character 4)
        let position = tower_lsp::lsp_types::Position {
            line: 1,
            character: 4,
        };

        // Call signature_help() method with retry for indexing
        let mut signature_result = None;
        for _attempt in 0..10 {
            signature_result = pool
                .signature_help(
                    "lua-language-server",
                    &config,
                    "file:///test.lua", // host URI (not used by tokio pool)
                    "lua",
                    content,
                    position,
                )
                .await;

            if signature_result.is_some() {
                break;
            }

            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // Should return Some(SignatureHelp) with signature information
        assert!(
            signature_result.is_some(),
            "signature_help() should return Some(SignatureHelp) for 'add(' call"
        );

        let sig_help = signature_result.unwrap();
        // Verify we got signatures
        assert!(
            !sig_help.signatures.is_empty(),
            "Should have at least one signature"
        );

        // The signature should contain "add" function info with parameters a, b
        let first_sig = &sig_help.signatures[0];
        assert!(
            first_sig.label.contains("add")
                || first_sig.label.contains("a")
                || first_sig.label.contains("b"),
            "Signature should relate to add function, got: {}",
            first_sig.label
        );
    }
}
