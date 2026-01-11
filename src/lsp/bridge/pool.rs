//! Language server pool for downstream language servers.
//!
//! This module provides the LanguageServerPool which manages connections to
//! downstream language servers per ADR-0016 (Server Pool Coordination).
//!
//! Phase 1: Single-LS-per-Language routing (language → single server).

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tower_lsp::lsp_types::Url;

/// Timeout for LSP initialize handshake (ADR-0018 Tier 0: 30-60s recommended).
///
/// If a downstream language server does not respond to the initialize request
/// within this duration, the connection attempt fails with a timeout error.
const INIT_TIMEOUT_SECS: u64 = 30;

/// State of a downstream language server connection.
///
/// Tracks the lifecycle of the LSP handshake per ADR-0015:
/// - Initializing: spawn started, awaiting initialize response
/// - Ready: initialize/initialized handshake complete, can accept requests
/// - Failed: initialization failed (timeout, error, etc.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Server spawned, initialize request sent, awaiting response
    Initializing,
    /// Initialize/initialized handshake complete, ready for requests
    Ready,
    /// Initialization failed (timeout, error, server crash)
    Failed,
}

use super::connection::AsyncBridgeConnection;

/// Represents an opened virtual document for tracking.
///
/// Used for didClose propagation when host document closes.
/// Each OpenedVirtualDoc represents a virtual document that was opened
/// via didOpen on a downstream language server.
#[derive(Debug, Clone)]
pub(crate) struct OpenedVirtualDoc {
    /// The injection language (e.g., "lua", "python")
    pub(crate) language: String,
    /// The virtual document URI string
    pub(crate) virtual_uri: String,
}

/// Handle wrapping a connection with its state (ADR-0015 per-connection state).
///
/// Each connection has its own lifecycle state that transitions:
/// - Initializing: spawn started, awaiting initialize response
/// - Ready: initialize/initialized handshake complete
/// - Failed: initialization failed (timeout, error, etc.)
///
/// This design ensures state is atomically tied to the connection,
/// preventing race conditions between state checks and connection access.
pub(crate) struct ConnectionHandle {
    /// Connection state - uses std::sync::RwLock for fast, synchronous state checks
    state: std::sync::RwLock<ConnectionState>,
    /// Async connection to downstream language server
    connection: tokio::sync::Mutex<AsyncBridgeConnection>,
}

impl ConnectionHandle {
    /// Create a new ConnectionHandle with initial Initializing state.
    pub(crate) fn new(connection: AsyncBridgeConnection) -> Self {
        Self {
            state: std::sync::RwLock::new(ConnectionState::Initializing),
            connection: tokio::sync::Mutex::new(connection),
        }
    }

    /// Get the current connection state.
    ///
    /// Uses std::sync::RwLock for fast, non-blocking read access.
    pub(crate) fn state(&self) -> ConnectionState {
        *self.state.read().expect("state lock poisoned")
    }

    /// Set the connection state.
    ///
    /// Used during initialization to transition to Ready or Failed.
    pub(crate) fn set_state(&self, state: ConnectionState) {
        *self.state.write().expect("state lock poisoned") = state;
    }

    /// Get access to the underlying async connection.
    ///
    /// Returns the tokio::sync::MutexGuard for async I/O operations.
    pub(crate) async fn connection(&self) -> tokio::sync::MutexGuard<'_, AsyncBridgeConnection> {
        self.connection.lock().await
    }
}

/// Pool of connections to downstream language servers (ADR-0016).
///
/// Implements Phase 1: Single-LS-per-Language routing where each injection
/// language maps to exactly one downstream server.
///
/// Provides lazy initialization of connections and handles the LSP handshake
/// (initialize/initialized) for each language server.
///
/// Connection state is embedded in each ConnectionHandle (ADR-0015 per-connection state),
/// eliminating the previous architectural flaw of having a separate state map.
pub(crate) struct LanguageServerPool {
    /// Map of language -> connection handle (wraps connection with its state)
    connections: Mutex<HashMap<String, Arc<ConnectionHandle>>>,
    /// Map of language -> (virtual document URI -> version)
    /// Tracks which documents have been opened and their current version number
    document_versions: Mutex<HashMap<String, HashMap<String, i32>>>,
    /// Tracks which virtual documents were opened for each host document.
    /// Key: host document URI, Value: list of opened virtual documents
    /// Used for didClose propagation when host document closes.
    host_to_virtual: Mutex<HashMap<Url, Vec<OpenedVirtualDoc>>>,
}

impl LanguageServerPool {
    /// Create a new language server pool.
    pub(crate) fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            document_versions: Mutex::new(HashMap::new()),
            host_to_virtual: Mutex::new(HashMap::new()),
        }
    }

    /// Increment the version of a virtual document and return the new version.
    ///
    /// Returns None if the document has not been opened.
    pub(super) async fn increment_document_version(
        &self,
        language: &str,
        virtual_uri: &str,
    ) -> Option<i32> {
        let mut versions = self.document_versions.lock().await;
        if let Some(docs) = versions.get_mut(language)
            && let Some(version) = docs.get_mut(virtual_uri)
        {
            *version += 1;
            return Some(*version);
        }
        None
    }

    /// Check if document is opened and mark it as opened atomically.
    ///
    /// Returns true if the document was NOT previously opened (i.e., didOpen should be sent).
    /// Returns false if the document was already opened (i.e., skip didOpen).
    ///
    /// When returning true, also records the mapping from host_uri to the virtual document
    /// in host_to_virtual. This mapping is used for didClose propagation when the host
    /// document is closed.
    pub(super) async fn should_send_didopen(
        &self,
        host_uri: &Url,
        language: &str,
        virtual_uri: &str,
    ) -> bool {
        let mut versions = self.document_versions.lock().await;
        let docs = versions.entry(language.to_string()).or_default();
        if docs.contains_key(virtual_uri) {
            false
        } else {
            docs.insert(virtual_uri.to_string(), 1);

            // Record the host -> virtual mapping for didClose propagation
            let mut host_map = self.host_to_virtual.lock().await;
            host_map
                .entry(host_uri.clone())
                .or_default()
                .push(OpenedVirtualDoc {
                    language: language.to_string(),
                    virtual_uri: virtual_uri.to_string(),
                });

            true
        }
    }

    /// Close all virtual documents associated with a host document.
    ///
    /// When a host document (e.g., markdown file) is closed, this method:
    /// 1. Looks up all virtual documents that were opened for the host
    /// 2. Sends didClose notification for each virtual document
    /// 3. Removes the virtual documents from document_versions tracking
    /// 4. Removes the host entry from host_to_virtual
    ///
    /// The connection to downstream language servers remains open - only the
    /// virtual documents are closed.
    ///
    /// Returns the list of closed virtual documents (useful for logging).
    pub(crate) async fn close_host_document(&self, host_uri: &Url) -> Vec<OpenedVirtualDoc> {
        // 1. Remove and get all virtual docs for this host
        let virtual_docs = {
            let mut host_map = self.host_to_virtual.lock().await;
            host_map.remove(host_uri).unwrap_or_default()
        };

        if virtual_docs.is_empty() {
            return vec![];
        }

        // 2. For each virtual doc: send didClose and remove from document_versions
        for doc in &virtual_docs {
            // Send didClose notification (best effort - ignore errors)
            let _ = self
                .send_didclose_notification(&doc.language, &doc.virtual_uri)
                .await;

            // Remove from document_versions
            let mut versions = self.document_versions.lock().await;
            if let Some(docs) = versions.get_mut(&doc.language) {
                docs.remove(&doc.virtual_uri);
            }
        }

        virtual_docs
    }

    /// Send a didClose notification for a virtual document.
    ///
    /// This method sends a didClose notification to the downstream language server
    /// for the specified virtual document URI. The connection is NOT closed after
    /// sending - it remains available for other documents.
    ///
    /// Returns Ok(()) if the notification was sent successfully, or if no connection
    /// exists for the language (nothing to do).
    ///
    /// # Arguments
    /// * `language` - The injection language (e.g., "lua")
    /// * `virtual_uri` - The virtual document URI string to close
    pub(crate) async fn send_didclose_notification(
        &self,
        language: &str,
        virtual_uri: &str,
    ) -> io::Result<()> {
        // Get the connection for this language (if it exists and is Ready)
        let connections = self.connections.lock().await;
        let Some(handle) = connections.get(language) else {
            // No connection for this language - nothing to do
            return Ok(());
        };

        // Only send if connection is Ready
        if handle.state() != ConnectionState::Ready {
            return Ok(());
        }

        let handle = Arc::clone(handle);
        drop(connections); // Release lock before I/O

        // Build and send the didClose notification
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "textDocument/didClose",
            "params": {
                "textDocument": {
                    "uri": virtual_uri
                }
            }
        });

        let mut conn = handle.connection().await;
        conn.write_message(&notification).await
    }

    /// Get or create a connection for the specified language.
    ///
    /// If no connection exists, spawns the language server and performs
    /// the LSP initialize/initialized handshake with default timeout.
    ///
    /// Returns the ConnectionHandle which wraps both the connection and its state.
    pub(super) async fn get_or_create_connection(
        &self,
        language: &str,
        server_config: &crate::config::settings::BridgeServerConfig,
    ) -> io::Result<Arc<ConnectionHandle>> {
        self.get_or_create_connection_with_timeout(
            language,
            server_config,
            Duration::from_secs(INIT_TIMEOUT_SECS),
        )
        .await
    }

    /// Get or create a connection for the specified language with custom timeout.
    ///
    /// If no connection exists, spawns the language server and performs
    /// the LSP initialize/initialized handshake. The timeout applies to the
    /// entire initialization process (write request + read response loop).
    ///
    /// Returns the ConnectionHandle which wraps both the connection and its state.
    /// State transitions are atomic with connection creation (ADR-0015).
    async fn get_or_create_connection_with_timeout(
        &self,
        language: &str,
        server_config: &crate::config::settings::BridgeServerConfig,
        timeout: Duration,
    ) -> io::Result<Arc<ConnectionHandle>> {
        let mut connections = self.connections.lock().await;

        // Check if we already have a connection for this language
        if let Some(handle) = connections.get(language) {
            // Check state atomically with connection lookup (fixes race condition)
            match handle.state() {
                ConnectionState::Initializing => {
                    return Err(io::Error::other("bridge: downstream server initializing"));
                }
                ConnectionState::Failed => {
                    // Remove failed connection, allow respawn on next attempt
                    connections.remove(language);
                    drop(connections);
                    // Recursive call to spawn fresh connection (boxed to avoid infinite future size)
                    return Box::pin(self.get_or_create_connection_with_timeout(
                        language,
                        server_config,
                        timeout,
                    ))
                    .await;
                }
                ConnectionState::Ready => {
                    return Ok(Arc::clone(handle));
                }
            }
        }

        // Spawn new connection - state starts as Initializing via ConnectionHandle::new()
        let conn = AsyncBridgeConnection::spawn(server_config.cmd.clone()).await?;
        let handle = Arc::new(ConnectionHandle::new(conn));

        // Insert handle into map while still initializing
        // This allows other requests to see the Initializing state
        connections.insert(language.to_string(), Arc::clone(&handle));

        // Drop the connections lock before doing I/O
        drop(connections);

        // Get mutable access to connection for initialization handshake
        let mut conn_guard = handle.connection().await;

        // Perform LSP initialize handshake with timeout
        // Note: The initialize request ID is internal (not client-facing),
        // so we use a fixed value rather than the upstream request ID.
        let init_result = tokio::time::timeout(timeout, async {
            let init_request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 0,
                "method": "initialize",
                "params": {
                    "processId": std::process::id(),
                    "rootUri": null,
                    "capabilities": {},
                    "initializationOptions": server_config.initialization_options
                }
            });

            conn_guard.write_message(&init_request).await?;

            // Read initialize response (skip any notifications)
            loop {
                let msg = conn_guard.read_message().await?;
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
            conn_guard.write_message(&initialized).await?;

            Ok::<_, io::Error>(())
        })
        .await;

        // Drop the connection guard before handling result
        // This is required by the borrow checker - we can't return handle while conn_guard exists
        drop(conn_guard);

        // Handle initialization result
        match init_result {
            Ok(Ok(())) => {
                // Init succeeded - set state to Ready (atomic with handle)
                handle.set_state(ConnectionState::Ready);
                Ok(handle)
            }
            Ok(Err(e)) => {
                // Init failed with io::Error - set state to Failed
                handle.set_state(ConnectionState::Failed);
                Err(e)
            }
            Err(_elapsed) => {
                // Timeout occurred - set state to Failed
                handle.set_state(ConnectionState::Failed);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Initialize timeout: downstream server unresponsive",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::BridgeServerConfig;
    use std::time::Duration;

    /// Test that ConnectionHandle wraps connection with state (ADR-0015).
    /// State should start as Initializing, and can transition via set_state().
    #[tokio::test]
    async fn connection_handle_wraps_connection_with_state() {
        // Create a mock server process to get a real connection
        let conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        // Wrap in ConnectionHandle
        let handle = ConnectionHandle::new(conn);

        // Initial state should be Initializing
        assert_eq!(
            handle.state(),
            ConnectionState::Initializing,
            "Initial state should be Initializing"
        );

        // Can transition to Ready
        handle.set_state(ConnectionState::Ready);
        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "State should transition to Ready"
        );

        // Can transition to Failed
        handle.set_state(ConnectionState::Failed);
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "State should transition to Failed"
        );

        // Can access connection
        let _conn_guard = handle.connection().await;
        // Connection is accessible (test passes if no panic)
    }

    /// Test that LanguageServerPool starts with no connections.
    /// Connections (and their states) are created lazily on first request.
    #[tokio::test]
    async fn pool_starts_with_no_connections() {
        let pool = LanguageServerPool::new();

        // Verify initial state: no connections exist
        let connections = pool.connections.lock().await;
        assert!(
            connections.is_empty(),
            "Connections map should exist and be empty initially"
        );
        assert!(
            !connections.contains_key("test"),
            "Connection should not exist before connection attempt"
        );
    }

    /// Helper to create a ConnectionHandle in a specific state for testing.
    async fn create_handle_with_state(state: ConnectionState) -> Arc<ConnectionHandle> {
        // Create a mock server process to get a real connection
        let conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");
        let handle = Arc::new(ConnectionHandle::new(conn));
        handle.set_state(state);
        handle
    }

    /// Test that requests during Initializing state return error immediately (non-blocking).
    /// Verifies both hover and completion fail fast with exact error message per ADR-0015.
    #[tokio::test]
    async fn request_during_init_returns_error_immediately() {
        use std::sync::Arc;
        use tower_lsp::lsp_types::{Position, Url};

        let pool = Arc::new(LanguageServerPool::new());
        let config = BridgeServerConfig {
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Insert a ConnectionHandle with Initializing state
        {
            let handle = create_handle_with_state(ConnectionState::Initializing).await;
            pool.connections
                .lock()
                .await
                .insert("lua".to_string(), handle);
        }

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Test hover request - should fail immediately
        let start = std::time::Instant::now();
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
                1, // upstream_request_id
            )
            .await;
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "Should not block"
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "bridge: downstream server initializing"
        );

        // Test completion request - same behavior
        let result = pool
            .send_completion_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
                1, // upstream_request_id
            )
            .await;
        assert_eq!(
            result.unwrap_err().to_string(),
            "bridge: downstream server initializing"
        );
    }

    /// Test that requests during Failed state trigger retry with a new server.
    ///
    /// When a connection is in Failed state and a request comes in, the retry mechanism:
    /// 1. Removes the failed connection from cache
    /// 2. Spawns a fresh server process
    /// 3. Succeeds if the new server initializes correctly
    ///
    /// This test uses lua-language-server which should initialize successfully.
    #[tokio::test]
    async fn request_during_failed_triggers_retry_with_new_server() {
        use std::sync::Arc;
        use tower_lsp::lsp_types::{Position, Url};

        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let pool = Arc::new(LanguageServerPool::new());
        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Insert a ConnectionHandle with Failed state
        {
            let handle = create_handle_with_state(ConnectionState::Failed).await;
            pool.connections
                .lock()
                .await
                .insert("lua".to_string(), handle);
        }

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Test hover request - should trigger retry and succeed with new server
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
                1, // upstream_request_id
            )
            .await;
        assert!(
            result.is_ok(),
            "Request should succeed after retry spawns new server: {:?}",
            result.err()
        );

        // Verify the connection is now Ready
        {
            let connections = pool.connections.lock().await;
            let handle = connections.get("lua").expect("Should have connection");
            assert_eq!(
                handle.state(),
                ConnectionState::Ready,
                "Connection should be Ready after retry"
            );
        }

        // Test completion request - should also succeed (connection is now Ready)
        let result = pool
            .send_completion_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
                2, // upstream_request_id
            )
            .await;
        assert!(
            result.is_ok(),
            "Completion request should succeed: {:?}",
            result.err()
        );
    }

    /// Test that requests succeed when ConnectionState is Ready.
    /// This is a regression test to ensure the init check doesn't block valid requests.
    #[tokio::test]
    async fn request_succeeds_when_state_is_ready() {
        use std::sync::Arc;
        use tower_lsp::lsp_types::{Position, Url};

        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let pool = Arc::new(LanguageServerPool::new());
        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // First request triggers initialization
        // After init completes, state should be Ready and request should succeed
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
                1, // upstream_request_id
            )
            .await;

        // Verify request succeeded (not blocked by init check)
        assert!(
            result.is_ok(),
            "Request should succeed after init completes: {:?}",
            result.err()
        );

        // Verify state is Ready after successful init (via ConnectionHandle)
        {
            let connections = pool.connections.lock().await;
            let handle = connections.get("lua").expect("Connection should exist");
            assert_eq!(
                handle.state(),
                ConnectionState::Ready,
                "State should be Ready after successful init"
            );
        }

        // Second request should also succeed (state remains Ready)
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('world')",
                2, // upstream_request_id
            )
            .await;

        assert!(
            result.is_ok(),
            "Subsequent request should also succeed: {:?}",
            result.err()
        );
    }

    /// Test that ConnectionState transitions to Failed on timeout
    #[tokio::test]
    async fn connection_state_transitions_to_failed_on_timeout() {
        let pool = LanguageServerPool::new();
        let config = BridgeServerConfig {
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["test".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Attempt connection with short timeout (will fail)
        let _ = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;

        // After timeout, state should be Failed (via ConnectionHandle)
        let connections = pool.connections.lock().await;
        let handle = connections
            .get("test")
            .expect("Connection handle should exist after timeout");
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "State should be Failed after timeout"
        );
    }

    /// Test that initialization times out when downstream server doesn't respond.
    ///
    /// This test uses a mock server that reads input but never writes output,
    /// simulating an unresponsive downstream language server.
    /// The initialization handshake should timeout.
    #[tokio::test]
    async fn init_times_out_when_server_unresponsive() {
        let pool = LanguageServerPool::new();
        let config = BridgeServerConfig {
            // sh -c 'cat > /dev/null': reads stdin but writes nothing to stdout
            // This simulates an unresponsive server that never sends a response
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["test".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let start = std::time::Instant::now();

        // Use get_or_create_connection_with_timeout for testing with short timeout
        let result = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;

        let elapsed = start.elapsed();

        // Should timeout quickly (within our 100ms timeout + buffer)
        assert!(
            elapsed < Duration::from_millis(500),
            "Should timeout quickly. Elapsed: {:?}",
            elapsed
        );

        // Should return an error
        assert!(result.is_err(), "Connection should fail with timeout error");
    }

    /// Test that timeout returns io::Error with io::ErrorKind::TimedOut.
    ///
    /// This verifies the error is properly typed so callers can distinguish
    /// timeout errors from other I/O errors.
    #[tokio::test]
    async fn init_timeout_returns_timed_out_error_kind() {
        let pool = LanguageServerPool::new();
        let config = BridgeServerConfig {
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["test".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let result = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;

        match result {
            Ok(_) => panic!("Should return an error"),
            Err(err) => {
                // Verify error kind is TimedOut
                assert_eq!(
                    err.kind(),
                    io::ErrorKind::TimedOut,
                    "Error kind should be TimedOut, got: {:?}",
                    err.kind()
                );

                // Verify error message is descriptive
                let msg = err.to_string();
                assert!(
                    msg.contains("timeout") || msg.contains("unresponsive"),
                    "Error message should mention timeout: {}",
                    msg
                );
            }
        }
    }

    /// Test that failed connection is removed from cache and new server is spawned on retry.
    ///
    /// When a connection is in Failed state, the next request should:
    /// 1. Remove the failed connection from the cache
    /// 2. Spawn a fresh server process
    /// 3. Return success if the new server initializes correctly
    ///
    /// This test verifies the retry mechanism using a two-phase approach:
    /// - Phase 1: Insert a Failed connection handle for "lua"
    /// - Phase 2: Call get_or_create_connection, expect it to spawn new server
    #[tokio::test]
    async fn failed_connection_retry_removes_cache_and_spawns_new_server() {
        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let pool = LanguageServerPool::new();

        // Phase 1: Insert a Failed connection handle
        {
            let handle = create_handle_with_state(ConnectionState::Failed).await;
            pool.connections
                .lock()
                .await
                .insert("lua".to_string(), handle);
        }

        // Verify Failed state is in cache
        {
            let connections = pool.connections.lock().await;
            let handle = connections.get("lua").expect("Should have cached handle");
            assert_eq!(handle.state(), ConnectionState::Failed, "Should be Failed");
        }

        // Phase 2: Request connection - should remove failed entry and spawn new server
        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let result = pool
            .get_or_create_connection_with_timeout("lua", &config, Duration::from_secs(30))
            .await;

        // Should succeed with a new Ready connection
        assert!(
            result.is_ok(),
            "Should spawn new server after failed entry removed: {:?}",
            result.err()
        );

        // Verify new connection is Ready (not Failed)
        let handle = result.unwrap();
        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "New connection should be Ready"
        );

        // Verify the old Failed handle was replaced in cache
        {
            let connections = pool.connections.lock().await;
            let cached_handle = connections.get("lua").expect("Should have cached handle");
            assert_eq!(
                cached_handle.state(),
                ConnectionState::Ready,
                "Cached handle should be the new Ready one"
            );
        }
    }

    /// Test recovery after initialization timeout.
    ///
    /// This integration test verifies the full recovery flow:
    /// 1. First attempt uses unresponsive server -> times out, enters Failed state
    /// 2. Second attempt with working server -> retry removes failed entry, spawns new server
    /// 3. New server initializes successfully -> connection becomes Ready
    ///
    /// This simulates real-world scenario where a language server crashes or hangs,
    /// and user's subsequent request triggers recovery with a working server.
    #[tokio::test]
    async fn recovery_works_after_initialization_timeout() {
        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let pool = LanguageServerPool::new();

        // Phase 1: First attempt with unresponsive server - should timeout
        let unresponsive_config = BridgeServerConfig {
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let result = pool
            .get_or_create_connection_with_timeout(
                "lua",
                &unresponsive_config,
                Duration::from_millis(100),
            )
            .await;
        assert!(result.is_err(), "First attempt should timeout");
        let err = result.err().expect("Should have error");
        assert_eq!(
            err.kind(),
            io::ErrorKind::TimedOut,
            "Error should be TimedOut"
        );

        // Verify Failed state is cached
        {
            let connections = pool.connections.lock().await;
            let handle = connections.get("lua").expect("Should have cached handle");
            assert_eq!(
                handle.state(),
                ConnectionState::Failed,
                "Should be Failed after timeout"
            );
        }

        // Phase 2: Second attempt with working server - should recover
        let working_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let result = pool
            .get_or_create_connection_with_timeout("lua", &working_config, Duration::from_secs(30))
            .await;

        // Should succeed - retry removed failed entry and spawned new server
        assert!(
            result.is_ok(),
            "Second attempt should succeed after recovery: {:?}",
            result.err()
        );

        // Verify new connection is Ready
        let handle = result.unwrap();
        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "Recovered connection should be Ready"
        );

        // Verify cache contains the Ready connection
        {
            let connections = pool.connections.lock().await;
            let cached_handle = connections.get("lua").expect("Should have cached handle");
            assert_eq!(
                cached_handle.state(),
                ConnectionState::Ready,
                "Cached handle should be Ready after recovery"
            );
        }
    }

    /// Test that OpenedVirtualDoc struct exists with required fields.
    ///
    /// The struct should have:
    /// - language: String (the injection language, e.g., "lua")
    /// - virtual_uri: String (the virtual document URI)
    #[tokio::test]
    async fn opened_virtual_doc_struct_has_required_fields() {
        use super::OpenedVirtualDoc;

        let doc = OpenedVirtualDoc {
            language: "lua".to_string(),
            virtual_uri: "file:///.treesitter-ls/abc123/lua-0.lua".to_string(),
        };

        assert_eq!(doc.language, "lua");
        assert_eq!(doc.virtual_uri, "file:///.treesitter-ls/abc123/lua-0.lua");
    }

    /// Test that LanguageServerPool has host_to_virtual field.
    ///
    /// The field should be initialized as empty HashMap and accessible.
    #[tokio::test]
    async fn pool_has_host_to_virtual_field() {
        let pool = LanguageServerPool::new();

        // Access the host_to_virtual field to verify it exists
        let host_map = pool.host_to_virtual.lock().await;
        assert!(
            host_map.is_empty(),
            "host_to_virtual should be empty on new pool"
        );
    }

    /// Test that should_send_didopen records host to virtual mapping.
    ///
    /// When should_send_didopen returns true (meaning didOpen should be sent),
    /// it should also record the mapping from host URI to the opened virtual document.
    #[tokio::test]
    async fn should_send_didopen_records_host_to_virtual_mapping() {
        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = "file:///.treesitter-ls/abc123/lua-0.lua";

        // First call should return true (document not opened yet)
        let result = pool
            .should_send_didopen(&host_uri, "lua", virtual_uri)
            .await;
        assert!(result, "First call should return true");

        // Verify the host_to_virtual mapping was recorded
        let host_map = pool.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(virtual_docs.len(), 1);
        assert_eq!(virtual_docs[0].language, "lua");
        assert_eq!(virtual_docs[0].virtual_uri, virtual_uri);
    }

    /// Test that should_send_didopen records multiple virtual docs for same host.
    ///
    /// A markdown file may have multiple Lua code blocks, each creating a separate
    /// virtual document. All should be tracked under the same host URI.
    #[tokio::test]
    async fn should_send_didopen_records_multiple_virtual_docs_for_same_host() {
        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Open first Lua block
        let result = pool
            .should_send_didopen(&host_uri, "lua", "file:///.treesitter-ls/abc123/lua-0.lua")
            .await;
        assert!(result, "First Lua block should return true");

        // Open second Lua block
        let result = pool
            .should_send_didopen(&host_uri, "lua", "file:///.treesitter-ls/abc123/lua-1.lua")
            .await;
        assert!(result, "Second Lua block should return true");

        // Verify both are tracked under the same host
        let host_map = pool.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(virtual_docs.len(), 2);
        assert_eq!(
            virtual_docs[0].virtual_uri,
            "file:///.treesitter-ls/abc123/lua-0.lua"
        );
        assert_eq!(
            virtual_docs[1].virtual_uri,
            "file:///.treesitter-ls/abc123/lua-1.lua"
        );
    }

    /// Test that should_send_didopen does not duplicate mapping on second call.
    ///
    /// When should_send_didopen returns false (document already opened),
    /// it should NOT add a duplicate entry to host_to_virtual.
    #[tokio::test]
    async fn should_send_didopen_does_not_duplicate_mapping() {
        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = "file:///.treesitter-ls/abc123/lua-0.lua";

        // First call - should return true and record mapping
        let result = pool
            .should_send_didopen(&host_uri, "lua", virtual_uri)
            .await;
        assert!(result, "First call should return true");

        // Second call for same virtual doc - should return false
        let result = pool
            .should_send_didopen(&host_uri, "lua", virtual_uri)
            .await;
        assert!(!result, "Second call should return false");

        // Verify only one entry exists (no duplicate)
        let host_map = pool.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(
            virtual_docs.len(),
            1,
            "Should only have one entry, not duplicates"
        );
    }

    /// Test that send_didclose_notification sends notification without closing connection.
    ///
    /// After sending didClose, the connection should still be in Ready state and
    /// can be used for other requests.
    #[tokio::test]
    async fn send_didclose_notification_keeps_connection_open() {
        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let pool = std::sync::Arc::new(LanguageServerPool::new());
        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = tower_lsp::lsp_types::Position {
            line: 3,
            character: 5,
        };

        // First, send a hover request to establish connection and open a virtual doc
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "lua-0",
                3,
                "print('hello')",
                1,
            )
            .await;
        assert!(result.is_ok(), "Hover request should succeed");

        // Get the virtual URI that was opened
        let virtual_uri = "file:///.treesitter-ls/abc123/lua-0.lua";

        // Send didClose notification
        let result = pool.send_didclose_notification("lua", virtual_uri).await;
        assert!(
            result.is_ok(),
            "send_didclose_notification should succeed: {:?}",
            result.err()
        );

        // Verify connection is still Ready
        {
            let connections = pool.connections.lock().await;
            let handle = connections.get("lua").expect("Connection should exist");
            assert_eq!(
                handle.state(),
                ConnectionState::Ready,
                "Connection should remain Ready after didClose"
            );
        }
    }

    /// Test that close_host_document sends didClose for all virtual documents.
    ///
    /// When a host document is closed, all its virtual documents should receive
    /// didClose notifications and be cleaned up from tracking structures.
    #[tokio::test]
    async fn close_host_document_sends_didclose_for_all_virtual_docs() {
        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let pool = std::sync::Arc::new(LanguageServerPool::new());
        let config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // First, send hover requests to establish connection and open virtual docs
        // Use positions that are within the code block (position.line >= region_start_line)
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                tower_lsp::lsp_types::Position {
                    line: 4,
                    character: 5,
                },
                "lua",
                "lua-0",
                3, // region starts at line 3, position is at line 4, so virtual line = 1
                "print('hello')",
                1,
            )
            .await;
        assert!(result.is_ok(), "First hover request should succeed");

        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                tower_lsp::lsp_types::Position {
                    line: 8,
                    character: 5,
                },
                "lua",
                "lua-1",
                7, // region starts at line 7, position is at line 8, so virtual line = 1
                "print('world')",
                2,
            )
            .await;
        assert!(result.is_ok(), "Second hover request should succeed");

        // Verify we have 2 virtual docs tracked for this host
        {
            let host_map = pool.host_to_virtual.lock().await;
            let virtual_docs = host_map.get(&host_uri).expect("Should have virtual docs");
            assert_eq!(virtual_docs.len(), 2, "Should have 2 virtual docs");
        }

        // Close the host document
        let closed_docs = pool.close_host_document(&host_uri).await;

        // Verify we got back the closed docs
        assert_eq!(closed_docs.len(), 2, "Should return 2 closed docs");

        // Verify host_to_virtual is cleaned up
        {
            let host_map = pool.host_to_virtual.lock().await;
            assert!(
                !host_map.contains_key(&host_uri),
                "host_to_virtual should be cleaned up"
            );
        }

        // Verify document_versions is cleaned up
        {
            let versions = pool.document_versions.lock().await;
            if let Some(docs) = versions.get("lua") {
                for doc in &closed_docs {
                    assert!(
                        !docs.contains_key(&doc.virtual_uri),
                        "document_versions should not contain closed doc: {}",
                        doc.virtual_uri
                    );
                }
            }
        }

        // Verify connection is still Ready (not closed)
        {
            let connections = pool.connections.lock().await;
            let handle = connections.get("lua").expect("Connection should exist");
            assert_eq!(
                handle.state(),
                ConnectionState::Ready,
                "Connection should remain Ready after close_host_document"
            );
        }
    }

    /// Test that ConnectionHandle IS cached after timeout with Failed state.
    ///
    /// When initialization times out, the ConnectionHandle is stored in the pool
    /// with Failed state. This allows subsequent requests to fail fast without
    /// attempting to spawn a new connection.
    #[tokio::test]
    async fn connection_handle_cached_with_failed_state_after_timeout() {
        let pool = LanguageServerPool::new();
        let config = BridgeServerConfig {
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["test".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // First attempt - should timeout
        let result = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;
        assert!(result.is_err(), "First attempt should fail with timeout");

        // ConnectionHandle should be cached with Failed state
        let connections = pool.connections.lock().await;
        assert!(
            connections.contains_key("test"),
            "ConnectionHandle should be cached after timeout"
        );
        let handle = connections.get("test").unwrap();
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "Cached handle should have Failed state"
        );
    }
}
