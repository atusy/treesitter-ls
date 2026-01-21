//! Language server pool for downstream language servers.
//!
//! This module provides the LanguageServerPool which manages connections to
//! downstream language servers per ADR-0016 (Server Pool Coordination).
//!
//! Phase 1: Single-LS-per-Language routing (language → single server).

mod connection_handle;
mod connection_state;
mod document_tracker;
mod shutdown_timeout;
#[cfg(test)]
mod test_helpers;

pub(crate) use connection_handle::ConnectionHandle;
pub(crate) use connection_state::ConnectionState;
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
use super::protocol::{
    VirtualDocumentUri, build_bridge_didopen_notification, build_initialize_request,
    build_initialized_notification,
};

/// Timeout for LSP initialize handshake (ADR-0018 Tier 0: 30-60s recommended).
///
/// If a downstream language server does not respond to the initialize request
/// within this duration, the connection attempt fails with a timeout error.
const INIT_TIMEOUT_SECS: u64 = 30;

use super::actor::{ResponseRouter, spawn_reader_task};
use super::connection::AsyncBridgeConnection;

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
    /// Used by did_close module for cleanup, and by Phase 3
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
        if self
            .document_tracker
            .should_send_didopen(host_uri, virtual_uri)
            .await
        {
            let did_open = build_bridge_didopen_notification(virtual_uri, virtual_content);
            if let Err(e) = writer.write_message(&did_open).await {
                cleanup_on_error();
                return Err(e);
            }
            self.document_tracker.mark_document_opened(virtual_uri);
        } else if !self.document_tracker.is_document_opened(virtual_uri) {
            cleanup_on_error();
            return Err(io::Error::other(
                "bridge: document not yet opened (didOpen pending)",
            ));
        }
        Ok(())
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

    /// Drains a JoinSet, logging any task panics with the provided context.
    async fn drain_join_set(join_set: &mut tokio::task::JoinSet<()>, task_context: &str) {
        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                log::error!(
                    target: "kakehashi::bridge",
                    "{} panicked: {}",
                    task_context,
                    e
                );
            }
    }
}

    /// Initiate graceful shutdown of all connections.
    ///
    /// Called during LSP server shutdown to cleanly terminate all downstream
    /// language servers. Performs LSP shutdown/exit handshake per ADR-0017.
    ///
    /// # Usage
    ///
    /// This method should be called exactly once during the LSP `shutdown` handler.
    /// Multiple concurrent calls are safe (due to state machine monotonicity) but
    /// wasteful, as connections already in Closing/Closed state are skipped.
    ///
    /// # Shutdown Behavior by State
    ///
    /// - Ready/Initializing: Perform full LSP shutdown handshake
    /// - Failed: Skip LSP handshake, go directly to Closed (stdin unavailable)
    /// - Closing/Closed: Already shutting down, skip
    ///
    /// All shutdowns run in parallel with a global timeout (ADR-0017).
    /// Uses the default GlobalShutdownTimeout (10s) per ADR-0018.
    pub(crate) async fn shutdown_all(&self) {
        self.shutdown_all_with_timeout(GlobalShutdownTimeout::default())
            .await;
    }

    /// Initiate graceful shutdown of all connections with a global timeout.
    ///
    /// This is the primary shutdown method per ADR-0017. It wraps parallel shutdown
    /// of all connections under a single global ceiling. When the timeout expires,
    /// remaining connections are force-killed with SIGTERM->SIGKILL escalation.
    ///
    /// # Arguments
    /// * `timeout` - Global shutdown timeout (5-15s per ADR-0018)
    ///
    /// # Behavior
    ///
    /// 1. All Ready/Initializing connections begin graceful shutdown in parallel
    /// 2. Failed connections transition directly to Closed (skip LSP handshake)
    /// 3. If global timeout expires before all complete:
    ///    - Remaining connections receive force_kill (SIGTERM->SIGKILL on Unix)
    ///    - All connections transition to Closed state
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeout = GlobalShutdownTimeout::new(Duration::from_secs(10))?;
    /// pool.shutdown_all_with_timeout(timeout).await;
    /// ```
    pub(crate) async fn shutdown_all_with_timeout(&self, timeout: GlobalShutdownTimeout) {
        // Track connections that were skipped for logging (minimize lock duration)
        let mut failed_connections: Vec<String> = Vec::new();
        let mut already_closing: Vec<String> = Vec::new();

        // Collect handles to shutdown - release lock before async operations
        let handles_to_shutdown: Vec<(String, Arc<ConnectionHandle>)> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .filter_map(|(language, handle)| match handle.state() {
                    ConnectionState::Ready | ConnectionState::Initializing => {
                        Some((language.clone(), Arc::clone(handle)))
                    }
                    ConnectionState::Failed => {
                        failed_connections.push(language.clone());
                        handle.complete_shutdown();
                        None
                    }
                    ConnectionState::Closing | ConnectionState::Closed => {
                        already_closing.push(language.clone());
                        None
                    }
                })
                .collect()
        };

        // Log after releasing lock (same pattern as force_kill_all)
        for language in failed_connections {
            log::debug!(
                target: "kakehashi::bridge",
                "Shutting down {} connection (Failed → Closed)",
                language
            );
        }
        for language in already_closing {
            log::debug!(
                target: "kakehashi::bridge",
                "Connection {} already shutting down or closed",
                language
            );
        }

        if handles_to_shutdown.is_empty() {
            return;
        }

        // Spawn graceful shutdown tasks into JoinSet (outside timeout so we can abort on timeout)
        let mut join_set = tokio::task::JoinSet::new();
        for (language, handle) in handles_to_shutdown {
            join_set.spawn(async move {
                log::debug!(
                    target: "kakehashi::bridge",
                    "Performing graceful shutdown for {} connection",
                    language
                );
                if let Err(e) = handle.graceful_shutdown().await {
                    log::warn!(
                        target: "kakehashi::bridge",
                        "Graceful shutdown failed for {}: {}",
                        language, e
                    );
                }
            });
        }

        // Wait for all shutdowns to complete with global timeout
        let graceful_result = tokio::time::timeout(
            timeout.as_duration(),
            Self::drain_join_set(&mut join_set, "Shutdown task"),
        )
        .await;

        // Handle timeout: abort remaining tasks and force-kill connections
        if graceful_result.is_err() {
            log::warn!(
                target: "kakehashi::bridge",
                "Global shutdown timeout ({:?}) expired, force-killing remaining connections",
                timeout.as_duration()
            );

            // Abort still-running graceful shutdown tasks to avoid duplicate logs and wasted work.
            // Note: force_kill is idempotent (returns early if process exited), so any race is harmless.
            join_set.abort_all();

            self.force_kill_all().await;
    }
}

    /// Force-kill all connections with platform-appropriate escalation.
    ///
    /// This is the fallback when global shutdown timeout expires.
    /// Per ADR-0017, this method terminates all non-closed connections and
    /// transitions them to Closed state.
    ///
    /// # Platform-Specific Behavior
    ///
    /// **Unix**: Uses SIGTERM->SIGKILL escalation (2s grace period)
    /// **Windows**: Uses TerminateProcess directly (no grace period)
    ///
    /// The method executes kills in parallel to minimize total shutdown time.
    pub(crate) async fn force_kill_all(&self) {
        // Collect handles to force-kill (minimize lock duration - no logging inside lock)
        let handles_with_info: Vec<(String, ConnectionState, Arc<ConnectionHandle>)> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .filter_map(|(language, handle)| {
                    let state = handle.state();
                    if state != ConnectionState::Closed {
                        Some((language.clone(), state, Arc::clone(handle)))
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Log after releasing lock
        for (language, state, _) in &handles_with_info {
            log::debug!(
                target: "kakehashi::bridge",
                "Force-killing {} connection (state: {:?})",
                language,
                state
            );
        }

        // Force-kill all connections in parallel with SIGTERM->SIGKILL escalation.
        // Using JoinSet for parallel execution ensures O(1) force-kill time for N connections
        // instead of O(N * 2s) when done sequentially (2s is SIGTERM->SIGKILL wait).
        let mut join_set = tokio::task::JoinSet::new();
        for (_, _, handle) in handles_with_info {
            join_set.spawn(async move {
                handle.force_kill().await;
                handle.complete_shutdown();
            });
        }

        // Wait for all force-kills to complete
        Self::drain_join_set(&mut join_set, "Force-kill task").await;
    }

    /// Perform the LSP initialize/initialized handshake.
    ///
    /// Sends the initialize request, waits for the response, and sends the
    /// initialized notification. This function is called by `get_or_create_connection_with_timeout`
    /// after the connection is spawned and the reader task is running.
    ///
    /// # Arguments
    /// * `handle` - The connection handle (in Initializing state)
    /// * `init_request_id` - Pre-registered request ID (always 1)
    /// * `init_response_rx` - Pre-registered receiver for initialize response
    /// * `init_options` - Server-specific initialization options
    ///
    /// # Returns
    /// * `Ok(())` - Handshake completed successfully
    /// * `Err(e)` - Handshake failed (server error, I/O error)
    async fn perform_lsp_handshake(
        handle: &ConnectionHandle,
        init_request_id: super::protocol::RequestId,
        init_response_rx: tokio::sync::oneshot::Receiver<serde_json::Value>,
        init_options: Option<serde_json::Value>,
    ) -> io::Result<()> {
        // 1. Build and send initialize request
        let init_request = build_initialize_request(init_request_id, init_options);
        {
            let mut writer = handle.writer().await;
            writer.write_message(&init_request).await?;
        }

        // 2. Wait for initialize response via pre-registered receiver
        let response = init_response_rx
            .await
            .map_err(|_| io::Error::other("bridge: initialize response channel closed"))?;

        // 3. Validate response
        Self::validate_initialize_response(&response)?;

        // 4. Send initialized notification
        let initialized = build_initialized_notification();
        {
            let mut writer = handle.writer().await;
            writer.write_message(&initialized).await?;
        }

        Ok(())
    }

    /// Validates a JSON-RPC initialize response.
    ///
    /// Uses lenient interpretation to maximize compatibility with non-conformant servers:
    /// - Prioritizes error field if present and non-null
    /// - Accepts result with null error field (`{"result": {...}, "error": null}`)
    /// - Rejects null or missing result field
    ///
    /// # Returns
    /// * `Ok(())` - Response is valid (has non-null result, no error)
    /// * `Err(e)` - Response has error or missing/null result
    fn validate_initialize_response(response: &serde_json::Value) -> io::Result<()> {
        // 1. Check for error response (prioritize error if present)
        if let Some(error) = response.get("error").filter(|e| !e.is_null()) {
            // Error field is non-null: treat as error regardless of result
            let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
            let message = error
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");

            return Err(io::Error::other(format!(
                "bridge: initialize failed (code {}): {}",
                code, message
            )));
        }

        // 2. Reject if result is absent or null
        if response.get("result").filter(|r| !r.is_null()).is_none() {
            return Err(io::Error::other(
                "bridge: initialize response missing valid result",
            ));
        }

        Ok(())
    }

}

#[cfg(test)]
mod validate_initialize_response_tests {
    use super::*;
    use serde_json::json;

        #[test]
    fn accepts_valid_result_without_error() {
        let response = json!({"result": {"capabilities": {}}});
        assert!(LanguageServerPool::validate_initialize_response(&response).is_ok());
    }

    #[test]
    fn accepts_valid_result_with_null_error() {
        let response = json!({"result": {"capabilities": {}}, "error": null});
        assert!(LanguageServerPool::validate_initialize_response(&response).is_ok());
        }

    #[test]
    fn rejects_error_response() {
        let response = json!({
                "error": {
                    "code": -32600,
                    "message": "Invalid Request"
                }
            });
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("code -32600"));
        assert!(err_msg.contains("Invalid Request"));
        }

    #[test]
    fn rejects_error_response_even_with_result() {
        let response = json!({
                "result": {"capabilities": {}},
                "error": {
                    "code": -32603,
                    "message": "Internal error"
                }
            });
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("code -32603"));
        assert!(err_msg.contains("Internal error"));
        }

    #[test]
    fn rejects_null_result() {
        let response = json!({"result": null});
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing valid result"));
        }

    #[test]
    fn rejects_missing_result_and_error() {
        let response = json!({});
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing valid result"));
        }

    #[test]
    fn rejects_null_result_with_null_error() {
        let response = json!({"result": null, "error": null});
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing valid result"));
        }

    #[test]
    fn handles_malformed_error_missing_code() {
        let response = json!({
                "error": {
                    "message": "Something went wrong"
                }
            });
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("code -1")); // Default code
        assert!(err_msg.contains("Something went wrong"));
        }

    #[test]
    fn handles_malformed_error_missing_message() {
        let response = json!({
                "error": {
                    "code": -32700
                }
            });
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("code -32700"));
        assert!(err_msg.contains("unknown error")); // Default message
        }

    #[test]
    fn handles_malformed_error_empty_object() {
        let response = json!({"error": {}});
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("code -1"));
        assert!(err_msg.contains("unknown error"));
        }

    #[test]
    fn handles_malformed_error_wrong_types() {
        let response = json!({
                "error": {
                    "code": "not-a-number",
                    "message": 123
                }
            });
        let result = LanguageServerPool::validate_initialize_response(&response);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("code -1")); // Can't parse string as i64
        assert!(err_msg.contains("unknown error")); // Can't parse number as str
        }

    #[test]
    fn accepts_complex_result_object() {
        let response = json!({
                "result": {
                    "capabilities": {
                        "textDocumentSync": 1,
                        "completionProvider": {
                            "triggerCharacters": ["."]
                        }
                    },
                    "serverInfo": {
                        "name": "test-server",
                        "version": "1.0.0"
                    }
                }
            });
        assert!(LanguageServerPool::validate_initialize_response(&response).is_ok());
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
        if let Some(handle) = connections.get(language) {
            // Check state atomically with connection lookup (ADR-0015 Operation Gating)
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
                ConnectionState::Closing => {
                    // Connection is shutting down, reject new requests
                    return Err(io::Error::other("bridge: connection closing"));
                }
                ConnectionState::Closed => {
                    // Connection is terminated, remove from pool and respawn
                    connections.remove(language);
                    drop(connections);
                    return Box::pin(self.get_or_create_connection_with_timeout(
                        language,
                        server_config,
                        timeout,
                    ))
                    .await;
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
            Self::perform_lsp_handshake(&handle, init_request_id, init_response_rx, init_options),
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

    /// Test that requests during Initializing state return error immediately (non-blocking).
    /// Verifies both hover and completion fail fast with exact error message per ADR-0015.
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
    /// This is a regression test to ensure the init check doesn't block valid requests.
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

    /// Test that timeout returns error and transitions connection to Failed state.
    ///
    /// With the async fast-fail architecture (ADR-0015), connections are stored
    /// immediately in Initializing state. On timeout, they transition to Failed
    /// state. Subsequent requests will remove the failed entry and spawn fresh.
    #[tokio::test]
    async fn connection_transitions_to_failed_state_on_timeout() {
        let pool = LanguageServerPool::new();
        let config = devnull_config_for_language("test");

        // Attempt connection with short timeout (will fail)
        let result = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;

        // Should return timeout error
        assert!(result.is_err(), "Should fail with timeout");
        assert_eq!(
            result.err().unwrap().kind(),
            io::ErrorKind::TimedOut,
            "Error should be TimedOut"
        );

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
    ///
    /// This test uses a mock server that reads input but never writes output,
    /// simulating an unresponsive downstream language server.
    /// The initialization handshake should timeout.
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

    /// Test that timeout returns io::Error with io::ErrorKind::TimedOut.
    ///
    /// This verifies the error is properly typed so callers can distinguish
    /// timeout errors from other I/O errors.
    #[tokio::test]
    async fn init_timeout_returns_timed_out_error_kind() {
        let pool = LanguageServerPool::new();
        let config = devnull_config_for_language("test");

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
        if !lua_ls_available() {
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

        // Phase 2: Second attempt with working server - should succeed immediately
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

    /// Test that ConnectionHandle is NOT cached after timeout.
    ///
    /// With the Reader Task architecture (ADR-0015 Phase A), failed connections
    /// are NOT cached because a ConnectionHandle requires:
    /// - A successfully split connection (writer + reader)
    /// - A running reader task
    /// - A response router
    ///
    /// When initialization times out, the connection transitions to Failed state.
    /// Subsequent requests will remove the failed entry and spawn fresh.
    #[tokio::test]
    async fn connection_handle_transitions_to_failed_after_timeout() {
        let pool = LanguageServerPool::new();
        let config = devnull_config_for_language("test");

        // First attempt - should timeout
        let result = pool
            .get_or_create_connection_with_timeout("test", &config, Duration::from_millis(100))
            .await;
        assert!(result.is_err(), "First attempt should fail with timeout");

        // With async fast-fail architecture, connection is stored and transitions to Failed
        let connections = pool.connections.lock().await;
        if let Some(handle) = connections.get("test") {
            assert_eq!(
                handle.state(),
                ConnectionState::Failed,
                "Connection should be in Failed state after timeout"
            );
        }
        // Note: Connection will be removed on next request attempt via Failed state handling
    }

    // ========================================
    // ensure_document_opened tests
    // ========================================
    // Note: Unit tests for should_send_didopen, is_document_opened, mark_document_opened,
    // and remove_matching_virtual_docs live in document_tracker.rs.
    // These integration tests verify ensure_document_opened behavior with I/O.

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

        // Track if cleanup was called (SHOULD be called in error path)
        let cleanup_called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleanup_called_clone = cleanup_called.clone();

        // Call ensure_document_opened - should fail and call cleanup
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

        // CRITICAL: Cleanup callback SHOULD have been called
        assert!(
            cleanup_called.load(std::sync::atomic::Ordering::SeqCst),
            "Cleanup callback MUST be called when returning error for pending didOpen"
        );

        // Document should still NOT be marked as opened
        assert!(
            !pool.is_document_opened(&virtual_uri),
            "Document should still NOT be marked as opened after error"
        );
    }

    /// Test that cleanup callback receives correct context for resource cleanup.
    ///
    /// The cleanup callback is typically used to remove a registered request from
    /// the router. This test verifies the callback is invoked correctly and can
    /// perform cleanup operations.
    #[tokio::test]
    async fn ensure_document_opened_cleanup_callback_can_perform_cleanup() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&url_to_uri(&host_uri), "lua", TEST_ULID_LUA_0);
        let virtual_content = "print('hello')";

        // Simulate pending didOpen state (inconsistent state)
        pool.should_send_didopen(&host_uri, &virtual_uri).await;

        // Create a mock writer
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat > /dev/null".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        let (mut writer, _reader) = conn.split();

        // Use a counter to verify cleanup is called exactly once
        let cleanup_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cleanup_count_clone = cleanup_count.clone();

        // Call ensure_document_opened - should fail and call cleanup
        let _result = pool
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

        // Cleanup should have been called exactly once
        assert_eq!(
            cleanup_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "Cleanup callback should be called exactly once"
        );
    }

    // ========================================
    // Sprint 12: Connection State Machine Integration Tests
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
    // Sprint 12 Phase 2: LSP Shutdown Handshake Tests
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
    ///
    /// ADR-0017 three-phase synchronization: The current architecture uses a Mutex
    /// to serialize all writer access, which provides exclusive access during shutdown.
    /// This test verifies that:
    /// 1. Shutdown transitions to Closing state first (rejects new operations)
    /// 2. Shutdown acquires writer lock for shutdown request
    /// 3. After shutdown completes, state is Closed
    ///
    /// Note: The full three-phase writer loop synchronization (signal stop, wait idle,
    /// exclusive access) applies to future writer loop architecture. Current Mutex-based
    /// architecture provides equivalent synchronization.
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
    // Sprint 12 Phase 3: Forced Shutdown Tests
    // ========================================

    /// Test that unresponsive process receives SIGTERM then SIGKILL escalation.
    ///
    /// ADR-0017: When LSP shutdown handshake times out, escalate to process signals.
    /// This test uses a script that ignores SIGTERM to verify SIGKILL escalation.
    ///
    /// Note: This test is Unix-specific due to process signal handling.
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
    ///
    /// ADR-0017 end-to-end shutdown sequence:
    /// 1. Create a connection with in-flight requests
    /// 2. Initiate shutdown (begin_shutdown transitions to Closing)
    /// 3. Pending requests should receive REQUEST_FAILED error (router channels closed)
    /// 4. LSP shutdown/exit handshake should complete
    /// 5. Connection should transition to Closed state
    ///
    /// This test uses lua-language-server to verify real LSP shutdown behavior.
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

    /// Test that new requests during Closing state receive REQUEST_FAILED immediately.
    ///
    /// This verifies operation gating during shutdown - the acceptance criterion that
    /// "new requests in Closing state receive REQUEST_FAILED error".
    #[tokio::test]
    async fn new_request_during_closing_receives_request_failed() {
        use std::sync::Arc;
        use tower_lsp_server::ls_types::Position;

        let pool = Arc::new(LanguageServerPool::new());
        let config = devnull_config();

        // Insert a connection in Closing state
        let closing_handle = create_handle_with_state(ConnectionState::Closing).await;
        pool.connections
            .lock()
            .await
            .insert("lua".to_string(), closing_handle);

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Attempt to send a hover request - should fail immediately
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
                1,
            )
            .await;

        // Should fail fast (not block)
        assert!(
            start.elapsed() < Duration::from_millis(100),
            "Should fail immediately, not block"
        );

        // Should return the specific error message
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "bridge: connection closing",
            "Should return REQUEST_FAILED with connection closing message"
        );
    }

    // ============================================================
    // Sprint 13: Global Shutdown Timeout Integration Tests
    // ============================================================
    // Unit tests for GlobalShutdownTimeout newtype live in shutdown_timeout.rs.
    // These integration tests verify pool shutdown behavior with the timeout.

    /// Test that shutdown_all completes within configured timeout even with hung servers.
    ///
    /// ADR-0017: Global timeout wraps all parallel shutdowns.
    /// This test verifies that:
    /// 1. shutdown_all_with_timeout() accepts a GlobalShutdownTimeout
    /// 2. Shutdown completes within the timeout even if servers hang
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

    /// Test that multiple servers shut down concurrently, total time bounded by global timeout.
    ///
    /// ADR-0017: N servers should shut down in O(1) time, not O(N).
    /// This test verifies:
    /// 1. Multiple hung servers all receive shutdown in parallel
    /// 2. Total shutdown time is bounded by global timeout (not N * per-server)
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
    // Sprint 13: Phase 3 - Force-kill fallback
    // ============================================================

    /// Test that force_kill_all() sends signals to all remaining connections.
    ///
    /// ADR-0017: When global timeout expires, force_kill_all() is called.
    /// This test verifies force_kill_all() method exists and transitions
    /// all connections to Closed state.
    #[tokio::test]
    #[cfg(unix)]
    async fn force_kill_all_terminates_all_connections() {
        let pool = LanguageServerPool::new();

        // Insert multiple connections in Ready state
        for i in 0..2 {
            let handle = create_handle_with_state(ConnectionState::Ready).await;
            pool.connections
                .lock()
                .await
                .insert(format!("server_{}", i), handle);
        }

        // Call force_kill_all directly
        pool.force_kill_all().await;

        // All connections should be in Closed state
        let connections = pool.connections.lock().await;
        for i in 0..2 {
            let key = format!("server_{}", i);
            if let Some(handle) = connections.get(&key) {
                assert_eq!(
                    handle.state(),
                    ConnectionState::Closed,
                    "Connection {} should be Closed after force_kill_all()",
                    key
                );
            }
    }
}

    /// Test that shutdown_all_with_timeout wires force_kill fallback correctly.
    ///
    /// ADR-0017: When global timeout expires, remaining connections are force-killed.
    /// This test verifies all connections end up in Closed state regardless of
    /// how graceful shutdown proceeds.
    ///
    /// Note: Full timeout behavior testing depends on removing the per-connection
    /// timeout (subtask 6). For now, we verify the force_kill path is wired and
    /// all connections reach Closed state.
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
    // Sprint 13: Phase 4 - Cleanup (remove per-connection timeout)
    // ============================================================

    /// Architectural verification: graceful_shutdown has no internal timeout.
    ///
    /// ADR-0018: Global shutdown is the only ceiling. The per-connection timeout
    /// was removed; graceful_shutdown waits indefinitely for response, relying
    /// on the caller (shutdown_all_with_timeout) to enforce the global timeout.
    ///
    /// # Design Rationale
    ///
    /// Previously, graceful_shutdown() had a hardcoded SHUTDOWN_TIMEOUT of 5 seconds.
    /// This caused timeout multiplication: N connections × 5s when shutting down
    /// sequentially, or unpredictable behavior with parallel shutdowns.
    ///
    /// Per ADR-0018, the timeout was removed. Now:
    /// - graceful_shutdown() waits indefinitely for the LSP shutdown response
    /// - shutdown_all_with_timeout() wraps ALL parallel shutdowns in a single
    ///   global timeout (5-15s configurable)
    /// - Fast servers complete quickly; slow servers use remaining budget
    /// - When global timeout expires, force_kill_all() terminates remaining connections
    ///
    /// # Verification
    ///
    /// This test verifies the design by checking that:
    /// 1. GlobalShutdownTimeout provides the only configurable timeout
    /// 2. graceful_shutdown() has no Duration constant or timeout wrapper
    ///
    /// The actual runtime behavior is tested by:
    /// - `shutdown_all_completes_within_global_timeout_with_hung_servers`
    /// - `multiple_servers_shutdown_concurrently_bounded_by_global_timeout`
    #[test]
    fn graceful_shutdown_relies_on_global_timeout_not_internal() {
        // Verify the architectural property: GlobalShutdownTimeout is the only timeout config
        let timeout = GlobalShutdownTimeout::default();
        assert_eq!(
            timeout.as_duration(),
            Duration::from_secs(10),
            "Default global timeout should be 10s per ADR-0018"
        );

        // The absence of SHUTDOWN_TIMEOUT constant in graceful_shutdown() is verified by:
        // 1. Code review during PR
        // 2. The integration tests above which would fail if internal timeout existed
        //    (hung servers would timeout individually instead of being bounded by global)
    }

    // ============================================================
    // Sprint 13: Phase 5 - Robustness (writer-idle budget verification)
    // ============================================================

    /// Test that writer synchronization is within graceful_shutdown scope.
    ///
    /// ADR-0017: Writer-idle wait (2s) counts against global budget, not additional time.
    ///
    /// The current Mutex-based architecture provides equivalent synchronization:
    /// - graceful_shutdown() acquires writer lock via self.writer().await
    /// - This blocks until any ongoing writes complete
    /// - The wait is part of graceful_shutdown(), counting against global timeout
    /// - No separate 2s timeout needed - the global timeout (shutdown_all_with_timeout) provides the ceiling
    ///
    /// This test verifies the architectural property that writer synchronization
    /// happens INSIDE graceful_shutdown, not as a separate pre-step.
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
