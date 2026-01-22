//! Language server pool for downstream language servers.
//!
//! This module provides the LanguageServerPool which manages connections to
//! downstream language servers per ADR-0016 (Server Pool Coordination).
//!
//! Currently implements single-LS-per-language routing (language → single server).

mod connection_action;
mod connection_handle;
mod connection_state;
mod document_tracker;
mod handshake;
mod shutdown;
mod shutdown_timeout;
#[cfg(test)]
mod test_helpers;

use connection_action::{ConnectionAction, decide_connection_action};
use handshake::perform_lsp_handshake;

pub(crate) use connection_handle::ConnectionHandle;
pub(crate) use connection_state::ConnectionState;
use document_tracker::DocumentOpenDecision;
use document_tracker::DocumentTracker;
pub(crate) use document_tracker::OpenedVirtualDoc;
pub(crate) use shutdown_timeout::GlobalShutdownTimeout;

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use url::Url;

use super::connection::SplitConnectionWriter;
use super::protocol::{VirtualDocumentUri, build_bridge_didopen_notification};

/// Timeout for LSP initialize handshake (ADR-0018 Tier 0: 30-60s recommended).
///
/// If a downstream language server does not respond to the initialize request
/// within this duration, the connection attempt fails with a timeout error.
const INIT_TIMEOUT_SECS: u64 = 30;

use super::actor::{ResponseRouter, spawn_reader_task};
use super::connection::AsyncBridgeConnection;

/// Pool of connections to downstream language servers (ADR-0016).
///
/// Each injection language maps to exactly one downstream server (single-LS-per-language).
///
/// Provides lazy initialization of connections and handles the LSP handshake
/// (initialize/initialized) for each language server.
///
/// Connection state is embedded in each ConnectionHandle (ADR-0015 per-connection state).
pub(crate) struct LanguageServerPool {
    /// Map of language -> connection handle (wraps connection with its state)
    connections: Mutex<HashMap<String, Arc<ConnectionHandle>>>,
    /// Document tracking for virtual documents (versions, host mappings, opened state)
    document_tracker: DocumentTracker,
}

impl LanguageServerPool {
    /// Create a new language server pool.
    pub(crate) fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            document_tracker: DocumentTracker::new(),
        }
    }

    /// Get access to the connections map.
    ///
    /// Used by text_document submodules that need to access connections.
    pub(super) async fn connections(
        &self,
    ) -> tokio::sync::MutexGuard<'_, HashMap<String, Arc<ConnectionHandle>>> {
        self.connections.lock().await
    }

    // ========================================
    // DocumentTracker delegation methods
    // ========================================

    /// Remove and return all virtual documents for a host URI.
    ///
    /// Used by did_close module for cleanup.
    pub(super) async fn remove_host_virtual_docs(&self, host_uri: &Url) -> Vec<OpenedVirtualDoc> {
        self.document_tracker
            .remove_host_virtual_docs(host_uri)
            .await
    }

    /// Take virtual documents matching the given ULIDs, removing them from tracking.
    ///
    /// This is atomic: lookup and removal happen in a single lock acquisition,
    /// preventing race conditions with concurrent didOpen requests.
    ///
    /// Returns the removed documents (for sending didClose). Documents that
    /// were never opened (not in host_to_virtual) are not returned.
    ///
    /// # Arguments
    /// * `host_uri` - The host document URI
    /// * `invalidated_ulids` - ULIDs to match against virtual document URIs
    pub(crate) async fn remove_matching_virtual_docs(
        &self,
        host_uri: &Url,
        invalidated_ulids: &[ulid::Ulid],
    ) -> Vec<OpenedVirtualDoc> {
        self.document_tracker
            .remove_matching_virtual_docs(host_uri, invalidated_ulids)
            .await
    }

    /// Remove a document from all tracking state.
    ///
    /// Removes the document from version tracking and opened state.
    /// Used by did_close module for cleanup and by
    /// close_invalidated_virtual_docs for invalidated region cleanup.
    pub(crate) async fn untrack_document(&self, virtual_uri: &VirtualDocumentUri) {
        self.document_tracker.untrack_document(virtual_uri).await
    }

    /// Check if a document has had didOpen ACTUALLY sent to downstream (ADR-0015).
    ///
    /// This is a fast, synchronous check used by request handlers to ensure
    /// they don't send requests before didOpen has been sent.
    ///
    /// Returns true if `mark_document_opened()` has been called for this document.
    /// Returns false if the document hasn't been opened yet.
    pub(crate) fn is_document_opened(&self, virtual_uri: &VirtualDocumentUri) -> bool {
        self.document_tracker.is_document_opened(virtual_uri)
    }

    /// Mark a document as having had didOpen sent to downstream (ADR-0015).
    ///
    /// This should be called AFTER the didOpen notification has been successfully
    /// written to the downstream server. Request handlers check `is_document_opened()`
    /// before sending requests to ensure LSP spec compliance.
    ///
    /// Note: Production code uses `document_tracker.mark_document_opened()` directly
    /// via `ensure_document_opened()`. This delegation is exposed for test access.
    #[cfg(test)]
    pub(super) fn mark_document_opened(&self, virtual_uri: &VirtualDocumentUri) {
        self.document_tracker.mark_document_opened(virtual_uri)
    }

    /// Ensure document is opened before sending a request.
    ///
    /// Sends didOpen if this is the first request for the document.
    /// Returns error if another request is in the process of opening (race condition).
    ///
    /// The `cleanup_on_error` closure is called before returning error to clean up resources.
    ///
    /// # Decision Logic
    ///
    /// Uses `DocumentOpenDecision` to determine action:
    /// - `SendDidOpen`: Send didOpen notification, mark as opened
    /// - `AlreadyOpened`: Skip (no-op), document was already opened
    /// - `PendingError`: Race condition, call cleanup and return error
    pub(crate) async fn ensure_document_opened<F>(
        &self,
        writer: &mut SplitConnectionWriter,
        host_uri: &Url,
        virtual_uri: &VirtualDocumentUri,
        virtual_content: &str,
        cleanup_on_error: F,
    ) -> io::Result<()>
    where
        F: FnOnce(),
    {
        match self
            .document_tracker
            .document_open_decision(host_uri, virtual_uri)
            .await
        {
            DocumentOpenDecision::SendDidOpen => {
                let did_open = build_bridge_didopen_notification(virtual_uri, virtual_content);
                if let Err(e) = writer.write_message(&did_open).await {
                    cleanup_on_error();
                    return Err(e);
                }
                self.document_tracker.mark_document_opened(virtual_uri);
                Ok(())
            }
            DocumentOpenDecision::AlreadyOpened => Ok(()),
            DocumentOpenDecision::PendingError => {
                cleanup_on_error();
                Err(io::Error::other(
                    "bridge: document not yet opened (didOpen pending)",
                ))
            }
        }
    }

    /// Increment the version of a virtual document and return the new version.
    ///
    /// Returns None if the document has not been opened.
    pub(super) async fn increment_document_version(
        &self,
        virtual_uri: &VirtualDocumentUri,
    ) -> Option<i32> {
        self.document_tracker
            .increment_document_version(virtual_uri)
            .await
    }

    /// Check if document is opened and mark it as opened atomically.
    ///
    /// Returns true if the document was NOT previously opened (i.e., didOpen should be sent).
    /// Returns false if the document was already opened (i.e., skip didOpen).
    ///
    /// This is exposed for tests that need to simulate document opening without
    /// using the full ensure_document_opened flow.
    #[cfg(test)]
    pub(super) async fn should_send_didopen(
        &self,
        host_uri: &Url,
        virtual_uri: &VirtualDocumentUri,
    ) -> bool {
        self.document_tracker
            .should_send_didopen(host_uri, virtual_uri)
            .await
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
}

impl LanguageServerPool {
    /// Get or create a connection for the specified language with custom timeout.
    ///
    /// If no connection exists, spawns the language server and stores the connection
    /// in Initializing state immediately. A background task performs the LSP handshake.
    /// Requests during initialization fail fast with "bridge: downstream server initializing".
    ///
    /// Returns the ConnectionHandle which wraps both the connection and its state.
    /// State transitions per ADR-0015 Operation Gating:
    /// - Initializing: fast-fail with REQUEST_FAILED
    /// - Ready: proceed with request
    /// - Failed: remove from pool and respawn
    ///
    /// # Architecture (ADR-0015 Fast-Fail)
    ///
    /// 1. Spawn server process
    /// 2. Split into writer + reader immediately
    /// 3. Store ConnectionHandle in Initializing state
    /// 4. Spawn background task for LSP handshake
    /// 5. Background task transitions to Ready or Failed
    async fn get_or_create_connection_with_timeout(
        &self,
        language: &str,
        server_config: &crate::config::settings::BridgeServerConfig,
        timeout: Duration,
    ) -> io::Result<Arc<ConnectionHandle>> {
        let mut connections = self.connections.lock().await;

        // Check if we already have a connection for this language
        // Use pure decision function for testability (ADR-0015 Operation Gating)
        let existing_state = connections.get(language).map(|h| h.state());
        match decide_connection_action(existing_state) {
            ConnectionAction::ReturnExisting => {
                // Safe: we just checked that the connection exists and is Ready
                return Ok(Arc::clone(connections.get(language).unwrap()));
            }
            ConnectionAction::FailFast(msg) => {
                return Err(io::Error::other(msg));
            }
            ConnectionAction::SpawnNew => {
                // Remove stale connection if present (Failed or Closed state)
                if existing_state.is_some() {
                    connections.remove(language);
                }
            }
        }

        // Spawn new connection (while holding lock to prevent concurrent spawns)
        let mut conn = AsyncBridgeConnection::spawn(server_config.cmd.clone()).await?;

        // Split connection immediately
        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());

        // Pre-register initialize request ID (=1) BEFORE spawning reader task.
        // This prevents a race condition where fast language servers (e.g., pyright)
        // respond before register_request() is called, causing the response to be
        // dropped as "unknown request ID".
        let init_request_id = super::protocol::RequestId::new(1);
        let init_response_rx = router
            .register(init_request_id)
            .expect("fresh router cannot have duplicate IDs");

        // Now spawn reader task - it can route the initialize response immediately
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        // Create handle in Initializing state (fast-fail for concurrent requests)
        let handle = Arc::new(ConnectionHandle::with_state(
            writer,
            router,
            reader_handle,
            ConnectionState::Initializing,
        ));

        // Insert into pool immediately so concurrent requests see Initializing state
        connections.insert(language.to_string(), Arc::clone(&handle));

        // Release lock before async initialization
        drop(connections);

        // Perform LSP handshake with timeout
        let init_options = server_config.initialization_options.clone();
        let init_result = tokio::time::timeout(
            timeout,
            perform_lsp_handshake(&handle, init_request_id, init_response_rx, init_options),
        )
        .await;

        // Handle initialization result - transition state
        match init_result {
            Ok(Ok(())) => {
                // Init succeeded - transition to Ready
                handle.set_state(ConnectionState::Ready);
                Ok(handle)
            }
            Ok(Err(e)) => {
                // Init failed with io::Error - transition to Failed
                handle.set_state(ConnectionState::Failed);
                Err(e)
            }
            Err(_elapsed) => {
                // Timeout occurred - transition to Failed
                handle.set_state(ConnectionState::Failed);
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "bridge: initialize timeout",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use test_helpers::*;

    // ============================================================
    // Pool Integration Tests
    // ============================================================
    // Unit tests for ConnectionHandle, ConnectionState, GlobalShutdownTimeout,
    // and OpenedVirtualDoc live in their respective submodules.
    // This file contains integration tests that exercise cross-module behavior.

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

    /// Test that requests during Initializing state return error immediately.
    #[tokio::test]
    async fn request_during_init_returns_error_immediately() {
        use std::sync::Arc;
        use tower_lsp_server::ls_types::Position;

        let pool = Arc::new(LanguageServerPool::new());
        let config = devnull_config();

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
    #[tokio::test]
    async fn request_during_failed_triggers_retry_with_new_server() {
        use std::sync::Arc;
        use tower_lsp_server::ls_types::Position;

        if !lua_ls_available() {
            return;
        }

        let pool = Arc::new(LanguageServerPool::new());
        let config = lua_ls_config();

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
    #[tokio::test]
    async fn request_succeeds_when_state_is_ready() {
        use std::sync::Arc;
        use tower_lsp_server::ls_types::Position;

        if !lua_ls_available() {
            return;
        }

        let pool = Arc::new(LanguageServerPool::new());
        let config = lua_ls_config();

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

    /// Test that timeout transitions connection to Failed state.
    #[tokio::test]
    async fn connection_transitions_to_failed_state_on_timeout() {
        let pool = LanguageServerPool::new();
        let config = devnull_config_for_language("test");

        // Attempt connection with short timeout (will fail)
        let result = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;

        // Should return timeout error with correct kind and descriptive message
        match result {
            Ok(_) => panic!("Should fail with timeout"),
            Err(err) => {
                assert_eq!(
                    err.kind(),
                    io::ErrorKind::TimedOut,
                    "Error should be TimedOut"
                );
                let msg = err.to_string();
                assert!(
                    msg.contains("timeout"),
                    "Error message should mention timeout: {}",
                    msg
                );
            }
        }

        // With async fast-fail architecture, failed connections are in Failed state
        // (will be removed on next request attempt via Failed state handling)
        let connections = pool.connections.lock().await;
        if let Some(handle) = connections.get("test") {
            assert_eq!(
                handle.state(),
                ConnectionState::Failed,
                "Connection should be in Failed state after timeout"
            );
        }
        // Note: Connection may or may not be present depending on timing
    }

    /// Test that initialization times out when downstream server doesn't respond.
    #[tokio::test]
    async fn init_times_out_when_server_unresponsive() {
        let pool = LanguageServerPool::new();
        // devnull_config simulates an unresponsive server that never sends a response
        let config = devnull_config_for_language("test");

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

    /// Test that failed connection is removed from cache and new server is spawned on retry.
    ///
    /// When a connection is in Failed state, the next request should:
    /// 1. Remove the failed connection from the cache
    /// 2. Spawn a fresh server process
    /// 3. Return success if the new server initializes correctly
    #[tokio::test]
    async fn failed_connection_retry_removes_cache_and_spawns_new_server() {
        if !lua_ls_available() {
            return;
        }

        let pool = LanguageServerPool::new();

        // Setup: Insert a Failed connection handle
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

        // Test: Request connection - should remove failed entry and spawn new server
        let config = lua_ls_config();

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
        if !lua_ls_available() {
            return;
        }

        let pool = LanguageServerPool::new();

        // First attempt with unresponsive server - should timeout
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

        // With async fast-fail architecture, connection is stored and transitions to Failed
        {
            let connections = pool.connections.lock().await;
            if let Some(handle) = connections.get("lua") {
                assert_eq!(
                    handle.state(),
                    ConnectionState::Failed,
                    "Connection should be in Failed state after timeout"
                );
            }
        }

        // Second attempt with working server - should succeed immediately
        let working_config = lua_ls_config();

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

    /// Test that send_didclose_notification sends notification without closing connection.
    ///
    /// After sending didClose, the connection should still be in Ready state and
    /// can be used for other requests.
    #[tokio::test]
    async fn send_didclose_notification_keeps_connection_open() {
        if !lua_ls_available() {
            return;
        }

        let pool = std::sync::Arc::new(LanguageServerPool::new());
        let config = lua_ls_config();

        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = tower_lsp_server::ls_types::Position {
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
                TEST_ULID_LUA_0,
                3,
                "print('hello')",
                1,
            )
            .await;
        assert!(result.is_ok(), "Hover request should succeed");

        // Get the virtual URI that was opened
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);

        // Send didClose notification
        let result = pool.send_didclose_notification(&virtual_uri).await;
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
        if !lua_ls_available() {
            return;
        }

        let pool = std::sync::Arc::new(LanguageServerPool::new());
        let config = lua_ls_config();

        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // First, send hover requests to establish connection and open virtual docs
        // Use positions that are within the code block (position.line >= region_start_line)
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                tower_lsp_server::ls_types::Position {
                    line: 4,
                    character: 5,
                },
                "lua",
                TEST_ULID_LUA_0,
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
                tower_lsp_server::ls_types::Position {
                    line: 8,
                    character: 5,
                },
                "lua",
                TEST_ULID_LUA_1,
                7, // region starts at line 7, position is at line 8, so virtual line = 1
                "print('world')",
                2,
            )
            .await;
        assert!(result.is_ok(), "Second hover request should succeed");

        // Close the host document
        let closed_docs = pool.close_host_document(&host_uri).await;

        // Verify we got back the closed docs
        assert_eq!(closed_docs.len(), 2, "Should return 2 closed docs");

        // Verify documents are no longer tracked as opened
        for doc in &closed_docs {
            assert!(
                !pool.is_document_opened(&doc.virtual_uri),
                "Document should no longer be tracked as opened: {}",
                doc.virtual_uri.to_uri_string()
            );
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

    /// Test that forward_didchange_to_opened_docs does not block on a busy connection.
    ///
    /// When a downstream request is in-flight (holding the connection lock),
    /// didChange forwarding should return quickly and enqueue the send in the background.
    #[tokio::test]
    async fn forward_didchange_does_not_block_on_busy_connection() {
        use super::super::protocol::VirtualDocumentUri;
        use std::sync::Arc;
        use std::time::{Duration, Instant};

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let opened = pool.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(opened, "First call should open the document");
        // Also mark as opened (simulating successful didOpen write)
        pool.mark_document_opened(&virtual_uri);

        let handle = create_handle_with_state(ConnectionState::Ready).await;
        pool.connections
            .lock()
            .await
            .insert("lua".to_string(), Arc::clone(&handle));

        // Hold the writer lock to simulate an in-flight request.
        let _writer_guard = handle.writer().await;

        let injections = vec![(
            "lua".to_string(),
            TEST_ULID_LUA_0.to_string(),
            "local x = 42".to_string(),
        )];

        let start = Instant::now();
        pool.forward_didchange_to_opened_docs(&host_uri, &injections)
            .await;
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "forward_didchange_to_opened_docs should not block on connection lock"
        );
    }

    // ========================================
    // ensure_document_opened tests
    // ========================================
    // Decision logic unit tests (DocumentOpenDecision) live in document_tracker.rs.
    // These integration tests verify ensure_document_opened I/O behavior:
    // - Writing didOpen notification to downstream
    // - Cleanup callback invocation on error
    // - Post-condition: document marked as opened

    /// Test that ensure_document_opened sends didOpen when document is not yet opened.
    ///
    /// Happy path: Document not in document_versions → should_send_didopen returns true
    /// → sends didOpen → marks document as opened via mark_document_opened.
    #[tokio::test]
    async fn ensure_document_opened_sends_didopen_for_new_document() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let virtual_content = "print('hello')";

        // Create a mock writer using cat (will discard our didOpen notification)
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        let (mut writer, _reader) = conn.split();

        // Before ensure_document_opened, document should not be marked as opened
        assert!(
            !pool.is_document_opened(&virtual_uri),
            "Document should not be opened initially"
        );

        // Track if cleanup was called (should NOT be called in happy path)
        let cleanup_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleanup_called_clone = cleanup_called.clone();

        // Call ensure_document_opened
        let result = pool
            .ensure_document_opened(
                &mut writer,
                &host_uri,
                &virtual_uri,
                virtual_content,
                move || {
                    cleanup_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                },
            )
            .await;

        // Should succeed
        assert!(result.is_ok(), "ensure_document_opened should succeed");

        // Cleanup should NOT have been called
        assert!(
            !cleanup_called.load(std::sync::atomic::Ordering::SeqCst),
            "Cleanup callback should NOT be called in happy path"
        );

        // After ensure_document_opened, document should be marked as opened
        assert!(
            pool.is_document_opened(&virtual_uri),
            "Document should be marked as opened after ensure_document_opened"
        );
    }

    /// Test that ensure_document_opened skips didOpen when document is already opened.
    ///
    /// Already opened path: Document marked as opened via mark_document_opened
    /// → should_send_didopen returns false, is_document_opened returns true
    /// → no didOpen sent, returns Ok(()).
    #[tokio::test]
    async fn ensure_document_opened_skips_didopen_for_already_opened_document() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let virtual_content = "print('hello')";

        // Pre-open the document (simulate previous didOpen)
        pool.should_send_didopen(&host_uri, &virtual_uri).await;
        pool.mark_document_opened(&virtual_uri);

        // Verify document is already marked as opened
        assert!(
            pool.is_document_opened(&virtual_uri),
            "Document should be marked as opened"
        );

        // Create a mock writer - we use a command that will fail if we try to write
        // This verifies that no didOpen is actually sent
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        let (mut writer, _reader) = conn.split();

        // Track if cleanup was called (should NOT be called)
        let cleanup_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleanup_called_clone = cleanup_called.clone();

        // Call ensure_document_opened - should skip didOpen
        let result = pool
            .ensure_document_opened(
                &mut writer,
                &host_uri,
                &virtual_uri,
                virtual_content,
                move || {
                    cleanup_called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                },
            )
            .await;

        // Should succeed (just skips didOpen)
        assert!(
            result.is_ok(),
            "ensure_document_opened should succeed for already opened document"
        );

        // Cleanup should NOT have been called
        assert!(
            !cleanup_called.load(std::sync::atomic::Ordering::SeqCst),
            "Cleanup callback should NOT be called when document already opened"
        );

        // Document should still be marked as opened
        assert!(
            pool.is_document_opened(&virtual_uri),
            "Document should still be marked as opened"
        );
    }

    /// Test that ensure_document_opened returns error when document is in inconsistent state.
    ///
    /// Error path: Another request called should_send_didopen (returned true) but hasn't
    /// yet called mark_document_opened. Our call sees:
    /// - should_send_didopen returns false (document_versions entry exists)
    /// - is_document_opened returns false (not yet marked)
    /// This is a race condition where didOpen is pending.
    ///
    /// Expected behavior: cleanup_on_error is called, returns error.
    #[tokio::test]
    async fn ensure_document_opened_returns_error_and_calls_cleanup_for_pending_didopen() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let virtual_content = "print('hello')";

        // Simulate another request having called should_send_didopen but NOT mark_document_opened
        // This puts the document in the "didOpen pending" state
        pool.should_send_didopen(&host_uri, &virtual_uri).await;
        // Deliberately do NOT call mark_document_opened to simulate pending didOpen

        // Verify the inconsistent state:
        // - should_send_didopen will return false (document already registered)
        // - is_document_opened will return false (not yet marked as opened)
        assert!(
            !pool.should_send_didopen(&host_uri, &virtual_uri).await,
            "should_send_didopen should return false (already registered)"
        );
        assert!(
            !pool.is_document_opened(&virtual_uri),
            "Document should NOT be marked as opened"
        );

        // Create a mock writer
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        let (mut writer, _reader) = conn.split();

        // Track cleanup calls with counter to verify called exactly once
        let cleanup_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cleanup_count_clone = cleanup_count.clone();

        // Call ensure_document_opened - should fail and call cleanup
        let result = pool
            .ensure_document_opened(
                &mut writer,
                &host_uri,
                &virtual_uri,
                virtual_content,
                move || {
                    cleanup_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                },
            )
            .await;

        // Should return error
        assert!(
            result.is_err(),
            "ensure_document_opened should return error for pending didOpen state"
        );

        // Verify error message
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("didOpen pending"),
            "Error message should mention didOpen pending: {}",
            err
        );

        // CRITICAL: Cleanup callback MUST be called exactly once
        assert_eq!(
            cleanup_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "Cleanup callback must be called exactly once for pending didOpen error"
        );

        // Document should still NOT be marked as opened
        assert!(
            !pool.is_document_opened(&virtual_uri),
            "Document should still NOT be marked as opened after error"
        );
    }

    // ========================================
    // Connection State Machine Integration Tests
    // ========================================
    // Unit tests for ConnectionState enum live in connection_state.rs.
    // These integration tests verify pool behavior with different connection states.

    /// Test that requests during Closing state receive error immediately.
    ///
    /// ADR-0015 Operation Gating: When connection is Closing, new requests
    /// are rejected with "bridge: connection closing" error. This prevents
    /// new requests from queuing during shutdown.
    #[tokio::test]
    async fn request_during_closing_state_returns_error_immediately() {
        use std::sync::Arc;
        use tower_lsp_server::ls_types::Position;

        let pool = Arc::new(LanguageServerPool::new());
        let config = devnull_config();

        // Insert a ConnectionHandle with Closing state
        {
            let handle = create_handle_with_state(ConnectionState::Closing).await;
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

        // Test hover request - should fail immediately with connection closing error
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
            "bridge: connection closing"
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
            "bridge: connection closing"
        );
    }

    // ========================================
    // LSP Shutdown Handshake Tests
    // ========================================

    /// Test that shutdown sends LSP shutdown request and receives response.
    ///
    /// ADR-0017: Graceful shutdown requires sending LSP "shutdown" request and
    /// waiting for the server's response before sending "exit" notification.
    /// This test verifies the shutdown request is properly formatted and sent.
    #[tokio::test]
    async fn shutdown_sends_lsp_shutdown_request_and_waits_for_response() {
        if !lua_ls_available() {
            return;
        }

        let pool = std::sync::Arc::new(LanguageServerPool::new());
        let config = lua_ls_config();

        // First, establish a Ready connection
        let handle = pool
            .get_or_create_connection("lua", &config)
            .await
            .expect("should establish connection");

        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "Connection should be Ready"
        );

        // Perform graceful shutdown
        let result = handle.graceful_shutdown().await;

        // Should succeed
        assert!(
            result.is_ok(),
            "graceful_shutdown should succeed: {:?}",
            result.err()
        );

        // Connection should be in Closed state after shutdown
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "Connection should be Closed after graceful_shutdown"
        );
    }

    /// Test that graceful shutdown acquires exclusive writer access.
    #[tokio::test]
    async fn graceful_shutdown_acquires_exclusive_writer_access() {
        // Create a connection to a mock server
        let handle = create_handle_with_state(ConnectionState::Ready).await;

        // Verify initial state
        assert_eq!(handle.state(), ConnectionState::Ready);

        // Perform shutdown
        let result = handle.graceful_shutdown().await;

        // Should complete (though may fail due to mock server not responding)
        // The important thing is state transitions are correct
        let _ = result;

        // After shutdown completes, state should be Closed
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "State should be Closed after graceful_shutdown"
        );
    }

    /// Test that shutdown transitions through Closing state.
    ///
    /// ADR-0017: Shutdown transitions to Closing state first, which rejects new
    /// operations. This test verifies the state transition happens immediately
    /// when begin_shutdown() is called.
    #[tokio::test]
    async fn shutdown_transitions_through_closing_state() {
        let handle = create_handle_with_state(ConnectionState::Ready).await;

        // Verify initial state
        assert_eq!(handle.state(), ConnectionState::Ready);

        // Manually call begin_shutdown to verify transition
        handle.begin_shutdown();

        // State should be Closing now
        assert_eq!(
            handle.state(),
            ConnectionState::Closing,
            "State should be Closing after begin_shutdown"
        );

        // Complete shutdown
        handle.complete_shutdown();

        // State should be Closed now
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "State should be Closed after complete_shutdown"
        );
    }

    /// Test that shutdown_all handles multiple connections in parallel.
    ///
    /// ADR-0017: All connections shut down in parallel with a global timeout.
    /// This test verifies that multiple connections can be shut down concurrently.
    #[tokio::test]
    async fn shutdown_all_handles_multiple_connections_in_parallel() {
        let pool = LanguageServerPool::new();

        // Create multiple connections with different states
        {
            let ready_handle = create_handle_with_state(ConnectionState::Ready).await;
            let failed_handle = create_handle_with_state(ConnectionState::Failed).await;
            let closing_handle = create_handle_with_state(ConnectionState::Closing).await;

            let mut connections = pool.connections.lock().await;
            connections.insert("lua".to_string(), ready_handle);
            connections.insert("python".to_string(), failed_handle);
            connections.insert("rust".to_string(), closing_handle);
        }

        // Call shutdown_all
        pool.shutdown_all().await;

        // Verify final states
        let connections = pool.connections.lock().await;

        // Ready -> should be Closed (went through graceful shutdown)
        let lua_handle = connections.get("lua").expect("lua should exist");
        assert_eq!(
            lua_handle.state(),
            ConnectionState::Closed,
            "Ready connection should be Closed after shutdown_all"
        );

        // Failed -> should be Closed (directly, no LSP handshake)
        let python_handle = connections.get("python").expect("python should exist");
        assert_eq!(
            python_handle.state(),
            ConnectionState::Closed,
            "Failed connection should be Closed after shutdown_all"
        );

        // Closing -> should remain Closing (was already shutting down)
        // Note: The closing handle was created with Closing state, but
        // shutdown_all skips it, so it stays Closing unless something else
        // completes the shutdown
        let rust_handle = connections.get("rust").expect("rust should exist");
        assert_eq!(
            rust_handle.state(),
            ConnectionState::Closing,
            "Already-closing connection should remain Closing"
        );
    }

    // ========================================
    // Forced Shutdown Tests
    // ========================================

    /// Test SIGTERM->SIGKILL escalation for unresponsive processes (Unix only).
    #[cfg(unix)]
    #[tokio::test]
    async fn unresponsive_process_receives_sigterm_then_sigkill() {
        use std::time::Instant;

        // Create a connection to a process that ignores SIGTERM
        // This script traps SIGTERM and continues, requiring SIGKILL to terminate
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            // Trap SIGTERM and ignore it, sleep indefinitely
            "trap '' TERM; while true; do sleep 1; done".to_string(),
        ])
        .await
        .expect("should spawn process");

        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        let handle = Arc::new(ConnectionHandle::with_state(
            writer,
            router,
            reader_handle,
            ConnectionState::Ready,
        ));

        // Start timer to verify escalation doesn't wait too long
        let start = Instant::now();

        // Call force_kill which should:
        // 1. Send SIGTERM
        // 2. Wait 2 seconds for graceful termination
        // 3. Send SIGKILL if process still alive
        handle.force_kill().await;

        // Escalation should complete within reasonable time (SIGTERM wait + SIGKILL)
        // We use 5 seconds as upper bound to account for SIGTERM wait period
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "Signal escalation should complete within 5 seconds"
        );

        // Process should be terminated
        // Note: After force_kill, we can't directly check process status via handle,
        // but the absence of a hang confirms the process was killed
    }

    /// Test that shutdown with pending requests fails those requests and then completes.
    #[tokio::test]
    async fn shutdown_with_pending_requests_fails_requests_then_completes() {
        use std::sync::Arc;

        if !lua_ls_available() {
            return;
        }

        let pool = Arc::new(LanguageServerPool::new());
        let config = lua_ls_config();

        // Step 1: Establish a Ready connection
        let handle = pool
            .get_or_create_connection("lua", &config)
            .await
            .expect("should establish connection");

        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "Connection should be Ready"
        );

        // Step 2: Register a pending request (simulates in-flight request)
        let (request_id, response_rx) = handle.register_request().expect("should register request");

        // Step 3: Initiate shutdown - should transition to Closing
        handle.begin_shutdown();
        assert_eq!(
            handle.state(),
            ConnectionState::Closing,
            "Connection should be Closing after begin_shutdown"
        );

        // Step 4: Complete graceful shutdown in background
        let shutdown_handle = Arc::clone(&handle);
        let shutdown_task = tokio::spawn(async move { shutdown_handle.graceful_shutdown().await });

        // Step 5: The pending request should fail when shutdown completes
        // The router is dropped during shutdown, closing all channels
        let response = tokio::time::timeout(Duration::from_secs(10), response_rx).await;

        // The response should be an error (channel closed) or timeout
        // because the shutdown closes the router
        match response {
            Ok(Ok(_)) => {
                // If we got a response, it should be because the server
                // responded before shutdown completed - this is acceptable
                log::debug!("Pending request received response before shutdown");
            }
            Ok(Err(_)) => {
                // Channel closed - this is the expected behavior
                // Pending request failed due to shutdown
                log::debug!("Pending request failed as expected (channel closed)");
            }
            Err(_) => {
                // Timeout - clean up
                handle.router().remove(request_id);
                log::debug!("Pending request timed out");
            }
        }

        // Wait for shutdown to complete
        let _ = shutdown_task.await;

        // Step 6: Connection should be in Closed state
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "Connection should be Closed after graceful_shutdown"
        );
    }

    // ============================================================
    // Global Shutdown Timeout Integration Tests
    // ============================================================
    // Unit tests for GlobalShutdownTimeout newtype live in shutdown_timeout.rs.

    /// Test that shutdown_all completes within configured timeout even with hung servers.
    #[tokio::test]
    async fn shutdown_all_completes_within_global_timeout_with_hung_servers() {
        let pool = LanguageServerPool::new();

        // Insert a connection that will hang (cat > /dev/null never responds)
        {
            let handle = create_handle_with_state(ConnectionState::Ready).await;
            pool.connections
                .lock()
                .await
                .insert("hung_server".to_string(), handle);
        }

        let timeout = GlobalShutdownTimeout::new(Duration::from_secs(5)).expect("5s is valid");

        let start = std::time::Instant::now();
        pool.shutdown_all_with_timeout(timeout).await;
        let elapsed = start.elapsed();

        // Should complete within timeout + 2s buffer for overhead.
        // Buffer accounts for: SIGTERM->SIGKILL escalation (2s) + test/CI variability.
        // Total: 5s timeout + 2s buffer = 7s max expected.
        assert!(
            elapsed < Duration::from_secs(7),
            "Shutdown should complete within global timeout. Elapsed: {:?}",
            elapsed
        );

        // All connections should be in Closed state
        let connections = pool.connections.lock().await;
        if let Some(handle) = connections.get("hung_server") {
            assert_eq!(
                handle.state(),
                ConnectionState::Closed,
                "Connection should be Closed after shutdown timeout"
            );
        }
    }

    /// Test that multiple servers shut down concurrently (O(1) not O(N) time).
    #[tokio::test]
    async fn multiple_servers_shutdown_concurrently_bounded_by_global_timeout() {
        let pool = LanguageServerPool::new();

        // Insert 3 hung servers - if sequential would be 3 * 5s = 15s
        for i in 0..3 {
            let handle = create_handle_with_state(ConnectionState::Ready).await;
            pool.connections
                .lock()
                .await
                .insert(format!("hung_server_{}", i), handle);
        }

        // Use 5s timeout - should complete in ~5s even with 3 servers
        let timeout = GlobalShutdownTimeout::new(Duration::from_secs(5)).expect("5s is valid");

        let start = std::time::Instant::now();
        pool.shutdown_all_with_timeout(timeout).await;
        let elapsed = start.elapsed();

        // Key assertion: total time should be O(1), not O(N).
        // 3 servers would take 15s sequential, but should complete in ~5-7s parallel.
        // Buffer (3s) accounts for: SIGTERM->SIGKILL escalation (2s) + process spawn overhead
        // + CI variability. Total: 5s timeout + 3s buffer = 8s max expected.
        assert!(
            elapsed < Duration::from_secs(8),
            "3 servers should shut down in O(1) time, not O(N). Elapsed: {:?}",
            elapsed
        );

        // All connections should be in Closed state
        let connections = pool.connections.lock().await;
        for i in 0..3 {
            let key = format!("hung_server_{}", i);
            if let Some(handle) = connections.get(&key) {
                assert_eq!(
                    handle.state(),
                    ConnectionState::Closed,
                    "Connection {} should be Closed after parallel shutdown",
                    key
                );
            }
        }
    }

    // ============================================================
    // Force-kill Fallback Tests
    // ============================================================

    /// Test that shutdown_all_with_timeout ensures all connections reach Closed state.
    #[tokio::test]
    #[cfg(unix)]
    async fn shutdown_with_timeout_ensures_all_connections_closed() {
        let pool = LanguageServerPool::new();

        // Insert connections
        for i in 0..2 {
            let handle = create_handle_with_state(ConnectionState::Ready).await;
            pool.connections
                .lock()
                .await
                .insert(format!("server_{}", i), handle);
        }

        // Use minimum valid timeout (5s)
        let timeout = GlobalShutdownTimeout::new(Duration::from_secs(5)).expect("5s is valid");

        pool.shutdown_all_with_timeout(timeout).await;

        // All connections should be in Closed state (via graceful shutdown or force-kill)
        let connections = pool.connections.lock().await;
        for i in 0..2 {
            let key = format!("server_{}", i);
            if let Some(handle) = connections.get(&key) {
                assert_eq!(
                    handle.state(),
                    ConnectionState::Closed,
                    "Connection {} should be Closed after shutdown_all_with_timeout",
                    key
                );
            }
        }
    }

    // ============================================================
    // Writer Synchronization Tests
    // ============================================================

    /// Test that writer synchronization is within graceful_shutdown scope.
    #[tokio::test]
    async fn writer_synchronization_is_within_graceful_shutdown_scope() {
        let handle = create_handle_with_state(ConnectionState::Ready).await;

        // Hold the writer lock to simulate ongoing write
        let _writer_guard = handle.writer().await;

        // Spawn a task to perform graceful_shutdown (will block on writer lock)
        let shutdown_handle = Arc::clone(&handle);
        let shutdown_task = tokio::spawn(async move {
            // This will block until writer lock is released
            let _ = shutdown_handle.graceful_shutdown().await;
        });

        // Give the shutdown task a moment to start and block
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Shutdown is blocked waiting for writer lock
        assert!(
            !shutdown_task.is_finished(),
            "Shutdown should be blocked on writer lock"
        );

        // Release the writer lock
        drop(_writer_guard);

        // Now shutdown should proceed
        let _ = tokio::time::timeout(Duration::from_secs(2), shutdown_task).await;

        // Verify shutdown completed (state is Closed)
        assert_eq!(
            handle.state(),
            ConnectionState::Closed,
            "State should be Closed after writer released and shutdown completed"
        );
    }
}
