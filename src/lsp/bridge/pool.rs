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
}

impl LanguageServerPool {
    /// Create a new language server pool.
    pub(crate) fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            document_versions: Mutex::new(HashMap::new()),
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
    pub(super) async fn should_send_didopen(&self, language: &str, virtual_uri: &str) -> bool {
        let mut versions = self.document_versions.lock().await;
        let docs = versions.entry(language.to_string()).or_default();
        if docs.contains_key(virtual_uri) {
            false
        } else {
            docs.insert(virtual_uri.to_string(), 1);
            true
        }
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
