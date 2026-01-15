//! Language server pool for downstream language servers.
//!
//! This module provides the LanguageServerPool which manages connections to
//! downstream language servers per ADR-0016 (Server Pool Coordination).
//!
//! Phase 1: Single-LS-per-Language routing (language → single server).

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use log::warn;
use tokio::sync::Mutex;
use tower_lsp::lsp_types::Url;

use super::protocol::{VirtualDocumentUri, build_bridge_didopen_notification};

/// Timeout for LSP initialize handshake (ADR-0018 Tier 0: 30-60s recommended).
///
/// If a downstream language server does not respond to the initialize request
/// within this duration, the connection attempt fails with a timeout error.
const INIT_TIMEOUT_SECS: u64 = 30;

/// State of a downstream language server connection.
///
/// Tracks the lifecycle of the LSP handshake per ADR-0015:
/// - Ready: initialize/initialized handshake complete, can accept requests
/// - Initializing (test-only): spawn started, awaiting initialize response
/// - Failed (test-only): initialization failed (timeout, error, etc.)
///
/// Note: Initializing and Failed are currently only constructed in tests
/// to verify concurrent access handling. Production code always starts in Ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    /// Initialize/initialized handshake complete, ready for requests
    Ready,
    /// Server spawned, initialize request sent, awaiting response (test-only)
    #[cfg(test)]
    Initializing,
    /// Initialization failed (timeout, error, server crash) (test-only)
    #[cfg(test)]
    Failed,
}

use super::actor::{ReaderTaskHandle, ResponseRouter, spawn_reader_task};
use super::connection::{AsyncBridgeConnection, SplitConnectionWriter};

/// Represents an opened virtual document for tracking.
///
/// Used for didClose propagation when host document closes.
/// Each OpenedVirtualDoc represents a virtual document that was opened
/// via didOpen on a downstream language server.
#[derive(Debug, Clone)]
pub(crate) struct OpenedVirtualDoc {
    /// The virtual document URI (contains language and region_id)
    pub(crate) virtual_uri: VirtualDocumentUri,
}

/// Handle wrapping a connection with its state (ADR-0015 per-connection state).
///
/// Each connection has its own lifecycle state that transitions:
/// - Initializing: spawn started, awaiting initialize response
/// - Ready: initialize/initialized handshake complete
/// - Failed: initialization failed (timeout, error, etc.)
///
/// # Architecture (ADR-0015 Phase A)
///
/// Uses Reader Task separation for non-blocking response waiting:
/// - `writer`: Mutex-protected for serialized request sending
/// - `router`: Routes responses to oneshot waiters
/// - `reader_handle`: Background task reading from stdout
///
/// Request flow:
/// 1. Register request ID with router to get oneshot receiver
/// 2. Lock writer, send request, release lock
/// 3. Await oneshot receiver (no Mutex held)
pub(crate) struct ConnectionHandle {
    /// Connection state - uses std::sync::RwLock for fast, synchronous state checks
    state: std::sync::RwLock<ConnectionState>,
    /// Writer for sending messages (Mutex serializes writes)
    writer: tokio::sync::Mutex<SplitConnectionWriter>,
    /// Router for pending request tracking
    router: Arc<ResponseRouter>,
    /// Handle to the reader task (for graceful shutdown on drop)
    _reader_handle: ReaderTaskHandle,
    /// Atomic counter for generating unique downstream request IDs.
    ///
    /// Each upstream request may have the same ID (from different contexts),
    /// so we generate unique IDs for downstream requests to avoid
    /// "duplicate request ID" errors in the ResponseRouter.
    next_request_id: AtomicI64,
}

impl ConnectionHandle {
    /// Create a new ConnectionHandle from split connection components.
    ///
    /// This spawns the Reader Task and sets up response routing.
    /// Initial state is set to Ready (caller has completed initialization).
    pub(crate) fn new(
        writer: SplitConnectionWriter,
        router: Arc<ResponseRouter>,
        reader_handle: ReaderTaskHandle,
    ) -> Self {
        Self {
            state: std::sync::RwLock::new(ConnectionState::Ready),
            writer: tokio::sync::Mutex::new(writer),
            router,
            _reader_handle: reader_handle,
            next_request_id: AtomicI64::new(1),
        }
    }

    /// Generate a unique downstream request ID.
    ///
    /// Each call returns the next ID in the sequence (1, 2, 3, ...).
    /// This ensures unique IDs for the ResponseRouter even when multiple
    /// upstream requests have the same ID.
    pub(crate) fn next_request_id(&self) -> i64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Get the current connection state.
    ///
    /// Uses std::sync::RwLock for fast, non-blocking read access.
    /// Recovers from poisoned locks with logging per project convention.
    pub(crate) fn state(&self) -> ConnectionState {
        match self.state.read() {
            Ok(guard) => *guard,
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned state lock in ConnectionHandle::state()"
                );
                *poisoned.into_inner()
            }
        }
    }

    /// Set the connection state (test-only).
    ///
    /// Used in tests to simulate various connection states (Initializing, Failed).
    /// Recovers from poisoned locks with logging per project convention.
    #[cfg(test)]
    fn set_state(&self, new_state: ConnectionState) {
        match self.state.write() {
            Ok(mut guard) => *guard = new_state,
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned state lock in ConnectionHandle::set_state()"
                );
                *poisoned.into_inner() = new_state;
            }
        }
    }

    /// Get access to the writer for sending messages.
    ///
    /// Returns the tokio::sync::MutexGuard for exclusive write access.
    pub(crate) async fn writer(&self) -> tokio::sync::MutexGuard<'_, SplitConnectionWriter> {
        self.writer.lock().await
    }

    /// Get the response router for registering pending requests.
    pub(crate) fn router(&self) -> &Arc<ResponseRouter> {
        &self.router
    }

    /// Register a new request and return (request_id, response_receiver).
    ///
    /// Generates a unique request ID and registers it with the router.
    /// Returns error if registration fails (should never happen with unique IDs).
    pub(crate) fn register_request(
        &self,
    ) -> io::Result<(
        super::protocol::RequestId,
        tokio::sync::oneshot::Receiver<serde_json::Value>,
    )> {
        let request_id = super::protocol::RequestId::new(self.next_request_id());
        let response_rx = self
            .router()
            .register(request_id)
            .ok_or_else(|| io::Error::other("duplicate request ID"))?;
        Ok((request_id, response_rx))
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
    /// Tracks documents that have had didOpen ACTUALLY sent to downstream (ADR-0015).
    ///
    /// This is separate from document_versions which marks intent to open.
    /// A document is only added here AFTER the didOpen notification has been
    /// written to the downstream server. Request handlers check this before
    /// sending requests to ensure LSP spec compliance.
    ///
    /// Uses std::sync::RwLock for fast, synchronous read checks.
    opened_documents: std::sync::RwLock<HashSet<String>>,
}

impl LanguageServerPool {
    /// Create a new language server pool.
    pub(crate) fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            document_versions: Mutex::new(HashMap::new()),
            host_to_virtual: Mutex::new(HashMap::new()),
            opened_documents: std::sync::RwLock::new(HashSet::new()),
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

    /// Remove and return all virtual documents for a host URI.
    ///
    /// Used by did_close module for cleanup.
    pub(super) async fn remove_host_virtual_docs(&self, host_uri: &Url) -> Vec<OpenedVirtualDoc> {
        let mut host_map = self.host_to_virtual.lock().await;
        host_map.remove(host_uri).unwrap_or_default()
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
        if invalidated_ulids.is_empty() {
            return Vec::new();
        }

        // Convert ULIDs to strings for matching
        let ulid_strs: std::collections::HashSet<String> =
            invalidated_ulids.iter().map(|u| u.to_string()).collect();

        let mut host_map = self.host_to_virtual.lock().await;
        let Some(docs) = host_map.get_mut(host_uri) else {
            return Vec::new();
        };

        // Partition: matching docs to return, non-matching to keep
        let mut to_close = Vec::new();
        docs.retain(|doc| {
            // Match region_id directly from VirtualDocumentUri
            let should_close = ulid_strs.contains(doc.virtual_uri.region_id());
            if should_close {
                to_close.push(doc.clone());
                false // Remove from host_to_virtual
            } else {
                true // Keep in host_to_virtual
            }
        });

        to_close
    }

    /// Remove a document from the version tracking.
    ///
    /// Used by did_close module for cleanup, and by Phase 3
    /// close_invalidated_virtual_docs for invalidated region cleanup.
    pub(crate) async fn remove_document_version(&self, virtual_uri: &VirtualDocumentUri) {
        let uri_string = virtual_uri.to_uri_string();
        let language = virtual_uri.language();

        let mut versions = self.document_versions.lock().await;
        if let Some(docs) = versions.get_mut(language) {
            docs.remove(&uri_string);
        }

        // Also remove from opened_documents (ADR-0015)
        match self.opened_documents.write() {
            Ok(mut opened) => {
                opened.remove(&uri_string);
            }
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned opened_documents lock in remove_document_version()"
                );
                poisoned.into_inner().remove(&uri_string);
            }
        }
    }

    /// Check if a document has had didOpen ACTUALLY sent to downstream (ADR-0015).
    ///
    /// This is a fast, synchronous check used by request handlers to ensure
    /// they don't send requests before didOpen has been sent.
    ///
    /// Returns true if `mark_document_opened()` has been called for this document.
    /// Returns false if the document hasn't been opened yet.
    pub(crate) fn is_document_opened(&self, virtual_uri: &VirtualDocumentUri) -> bool {
        let uri_string = virtual_uri.to_uri_string();

        match self.opened_documents.read() {
            Ok(opened) => opened.contains(&uri_string),
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned opened_documents lock in is_document_opened()"
                );
                poisoned.into_inner().contains(&uri_string)
            }
        }
    }

    /// Mark a document as having had didOpen sent to downstream (ADR-0015).
    ///
    /// This should be called AFTER the didOpen notification has been successfully
    /// written to the downstream server. Request handlers check `is_document_opened()`
    /// before sending requests to ensure LSP spec compliance.
    pub(crate) fn mark_document_opened(&self, virtual_uri: &VirtualDocumentUri) {
        let uri_string = virtual_uri.to_uri_string();

        match self.opened_documents.write() {
            Ok(mut opened) => {
                opened.insert(uri_string);
            }
            Err(poisoned) => {
                warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned opened_documents lock in mark_document_opened()"
                );
                poisoned.into_inner().insert(uri_string);
            }
        }
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
        if self.should_send_didopen(host_uri, virtual_uri).await {
            let did_open = build_bridge_didopen_notification(virtual_uri, virtual_content);
            writer.write_message(&did_open).await?;
            self.mark_document_opened(virtual_uri);
        } else if !self.is_document_opened(virtual_uri) {
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
        let uri_string = virtual_uri.to_uri_string();
        let language = virtual_uri.language();

        let mut versions = self.document_versions.lock().await;
        if let Some(docs) = versions.get_mut(language)
            && let Some(version) = docs.get_mut(&uri_string)
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
        virtual_uri: &VirtualDocumentUri,
    ) -> bool {
        use std::collections::hash_map::Entry;

        let uri_string = virtual_uri.to_uri_string();
        let language = virtual_uri.language();

        let mut versions = self.document_versions.lock().await;
        let docs = versions.entry(language.to_string()).or_default();

        if let Entry::Vacant(e) = docs.entry(uri_string) {
            e.insert(1);

            // Record the host -> virtual mapping for didClose propagation
            let mut host_map = self.host_to_virtual.lock().await;
            host_map
                .entry(host_uri.clone())
                .or_default()
                .push(OpenedVirtualDoc {
                    virtual_uri: virtual_uri.clone(),
                });

            true
        } else {
            false
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
    ///
    /// # Architecture (ADR-0015 Phase A)
    ///
    /// After successful initialization:
    /// 1. Connection is split into writer + reader
    /// 2. Reader Task is spawned with ResponseRouter
    /// 3. ConnectionHandle is created with Ready state
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
                #[cfg(test)]
                ConnectionState::Initializing => {
                    return Err(io::Error::other("bridge: downstream server initializing"));
                }
                #[cfg(test)]
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

        // IMPORTANT: Hold connections lock during spawn and initialization to prevent
        // multiple concurrent spawns. This ensures only ONE connection is created per
        // language. Other requests for the same language will wait on the lock.
        //
        // Previous bug: Dropping the lock before spawn allowed multiple requests to
        // each spawn their own connection, causing didOpen to be sent to one connection
        // while requests went to others.

        // Spawn new connection (while holding lock)
        let mut conn = AsyncBridgeConnection::spawn(server_config.cmd.clone()).await?;

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

            conn.write_message(&init_request).await?;

            // Read initialize response (skip any notifications)
            loop {
                let msg = conn.read_message().await?;
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
            conn.write_message(&initialized).await?;

            Ok::<_, io::Error>(())
        })
        .await;

        // Handle initialization result
        match init_result {
            Ok(Ok(())) => {
                // Init succeeded - split connection and spawn reader task
                let (writer, reader) = conn.split();
                let router = Arc::new(ResponseRouter::new());
                let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

                // Create handle with Ready state
                let handle = Arc::new(ConnectionHandle::new(writer, router, reader_handle));

                // Insert into pool (lock still held from start of function)
                connections.insert(language.to_string(), Arc::clone(&handle));

                Ok(handle)
            }
            Ok(Err(e)) => {
                // Init failed with io::Error - connection will be dropped
                Err(e)
            }
            Err(_elapsed) => {
                // Timeout occurred - connection will be dropped
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

    // Test ULID constants - valid 26-char alphanumeric strings matching ULID format.
    // Using realistic ULIDs ensures tests reflect actual runtime behavior.
    const TEST_ULID_LUA_0: &str = "01JPMQ8ZYYQA1W3AVPW4JDRZFR";
    const TEST_ULID_LUA_1: &str = "01JPMQ8ZYYQA1W3AVPW4JDRZFS";
    const TEST_ULID_PYTHON_0: &str = "01JPMQ8ZYYQA1W3AVPW4JDRZFT";

    /// Test that ConnectionHandle provides unique request IDs via atomic counter.
    ///
    /// Each call to next_request_id() should return a unique, incrementing value.
    /// This is critical for avoiding "duplicate request ID" errors when multiple
    /// upstream requests have the same ID (they come from different contexts).
    #[tokio::test]
    async fn connection_handle_provides_unique_request_ids() {
        // Create a mock server process to get a real connection
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        // Split connection and spawn reader task
        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        // Wrap in ConnectionHandle
        let handle = ConnectionHandle::new(writer, router, reader_handle);

        // Get multiple request IDs - they should be unique and incrementing
        let id1 = handle.next_request_id();
        let id2 = handle.next_request_id();
        let id3 = handle.next_request_id();

        assert_eq!(id1, 1, "First request ID should be 1");
        assert_eq!(id2, 2, "Second request ID should be 2");
        assert_eq!(id3, 3, "Third request ID should be 3");
    }

    /// Test that ConnectionHandle wraps connection with state (ADR-0015).
    /// State should start as Ready (since constructor is called after init handshake),
    /// and can transition via set_state().
    #[tokio::test]
    async fn connection_handle_wraps_connection_with_state() {
        // Create a mock server process to get a real connection
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        // Split connection and spawn reader task (new architecture)
        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        // Wrap in ConnectionHandle
        let handle = ConnectionHandle::new(writer, router, reader_handle);

        // Initial state should be Ready (ConnectionHandle is created after init handshake)
        assert_eq!(
            handle.state(),
            ConnectionState::Ready,
            "Initial state should be Ready"
        );

        // Can transition to Failed
        handle.set_state(ConnectionState::Failed);
        assert_eq!(
            handle.state(),
            ConnectionState::Failed,
            "State should transition to Failed"
        );

        // Can access writer
        let _writer_guard = handle.writer().await;
        // Writer is accessible (test passes if no panic)

        // Can access router
        let _router = handle.router();
        // Router is accessible (test passes if no panic)
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
        let mut conn = AsyncBridgeConnection::spawn(vec![
            "sh".to_string(),
            "-c".to_string(),
            "cat".to_string(),
        ])
        .await
        .expect("should spawn cat process");

        // Split connection and spawn reader task (new architecture)
        let (writer, reader) = conn.split();
        let router = Arc::new(ResponseRouter::new());
        let reader_handle = spawn_reader_task(reader, Arc::clone(&router));

        let handle = Arc::new(ConnectionHandle::new(writer, router, reader_handle));
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

    /// Test that timeout returns error and does NOT cache connection
    ///
    /// With the Reader Task architecture (ADR-0015 Phase A), failed connections
    /// are NOT cached. A ConnectionHandle requires a successfully split connection
    /// with a running reader task. On timeout, the connection is dropped and
    /// subsequent retries spawn fresh connections.
    #[tokio::test]
    async fn connection_not_cached_on_timeout() {
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

        // With Reader Task architecture, failed connections are NOT cached
        // (A ConnectionHandle requires a valid writer/router/reader_handle)
        let connections = pool.connections.lock().await;
        assert!(
            !connections.contains_key("test"),
            "Failed connection should NOT be cached (Reader Task architecture)"
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

        // With Reader Task architecture, failed connections are NOT cached
        // (A ConnectionHandle requires a valid writer/router/reader_handle)
        {
            let connections = pool.connections.lock().await;
            assert!(
                !connections.contains_key("lua"),
                "Failed connection should NOT be cached (Reader Task architecture)"
            );
        }

        // Phase 2: Second attempt with working server - should succeed immediately
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

    /// Test that OpenedVirtualDoc struct stores VirtualDocumentUri.
    ///
    /// The struct should have:
    /// - virtual_uri: VirtualDocumentUri (typed URI with language and region_id)
    #[tokio::test]
    async fn opened_virtual_doc_struct_has_required_fields() {
        use super::super::protocol::VirtualDocumentUri;
        use super::OpenedVirtualDoc;

        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let doc = OpenedVirtualDoc {
            virtual_uri: virtual_uri.clone(),
        };

        assert_eq!(doc.virtual_uri.language(), "lua");
        assert_eq!(doc.virtual_uri.region_id(), "region-0");
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
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "lua-0");

        // First call should return true (document not opened yet)
        let result = pool.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(result, "First call should return true");

        // Verify the host_to_virtual mapping was recorded
        let host_map = pool.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(virtual_docs.len(), 1);
        assert_eq!(virtual_docs[0].virtual_uri.language(), "lua");
        assert_eq!(virtual_docs[0].virtual_uri.region_id(), "lua-0");
    }

    /// Test that should_send_didopen records multiple virtual docs for same host.
    ///
    /// A markdown file may have multiple Lua code blocks, each creating a separate
    /// virtual document. All should be tracked under the same host URI.
    #[tokio::test]
    async fn should_send_didopen_records_multiple_virtual_docs_for_same_host() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Open first Lua block
        let virtual_uri_0 = VirtualDocumentUri::new(&host_uri, "lua", "lua-0");
        let result = pool.should_send_didopen(&host_uri, &virtual_uri_0).await;
        assert!(result, "First Lua block should return true");

        // Open second Lua block
        let virtual_uri_1 = VirtualDocumentUri::new(&host_uri, "lua", "lua-1");
        let result = pool.should_send_didopen(&host_uri, &virtual_uri_1).await;
        assert!(result, "Second Lua block should return true");

        // Verify both are tracked under the same host
        let host_map = pool.host_to_virtual.lock().await;
        let virtual_docs = host_map
            .get(&host_uri)
            .expect("host_uri should have entry in host_to_virtual");
        assert_eq!(virtual_docs.len(), 2);
        assert_eq!(virtual_docs[0].virtual_uri.region_id(), "lua-0");
        assert_eq!(virtual_docs[1].virtual_uri.region_id(), "lua-1");
    }

    /// Test that should_send_didopen does not duplicate mapping on second call.
    ///
    /// When should_send_didopen returns false (document already opened),
    /// it should NOT add a duplicate entry to host_to_virtual.
    #[tokio::test]
    async fn should_send_didopen_does_not_duplicate_mapping() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "lua-0");

        // First call - should return true and record mapping
        let result = pool.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(result, "First call should return true");

        // Second call for same virtual doc - should return false
        let result = pool.should_send_didopen(&host_uri, &virtual_uri).await;
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
                TEST_ULID_LUA_0,
                3,
                "print('hello')",
                1,
            )
            .await;
        assert!(result.is_ok(), "Hover request should succeed");

        // Get the virtual URI that was opened
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);

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
                tower_lsp::lsp_types::Position {
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
                    let uri_string = doc.virtual_uri.to_uri_string();
                    assert!(
                        !docs.contains_key(&uri_string),
                        "document_versions should not contain closed doc: {}",
                        uri_string
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

    /// Test that forward_didchange_to_opened_docs sends didChange only for opened virtual documents.
    ///
    /// When a host document changes, we should only send didChange notifications
    /// for virtual documents that have been opened (via didOpen). This test:
    /// 1. Opens a virtual document (by calling should_send_didopen)
    /// 2. Calls forward_didchange_to_opened_docs with injections including the opened doc
    /// 3. Verifies didChange is sent only for opened docs
    #[tokio::test]
    async fn forward_didchange_to_opened_docs_sends_for_opened_docs() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Generate the virtual URI the same way forward_didchange_to_opened_docs does
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);

        // Open a virtual document (simulate first hover/completion request)
        let opened = pool.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(opened, "First call should open the document");
        // Also mark as opened (simulating successful didOpen write)
        pool.mark_document_opened(&virtual_uri);

        // Verify document is tracked
        {
            let host_map = pool.host_to_virtual.lock().await;
            let docs = host_map.get(&host_uri).expect("Should have host entry");
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].virtual_uri.region_id(), TEST_ULID_LUA_0);
        }

        // Now call forward_didchange_to_opened_docs
        // The injection tuple is (language, region_id, content)
        let injections = vec![(
            "lua".to_string(),
            TEST_ULID_LUA_0.to_string(),
            "local x = 42".to_string(),
        )];

        // Call the method - it should find the opened doc and attempt to send didChange
        pool.forward_didchange_to_opened_docs(&host_uri, &injections)
            .await;

        // Verify the document version was incremented (indicating didChange was attempted)
        {
            let versions = pool.document_versions.lock().await;
            if let Some(docs) = versions.get("lua") {
                // Version should be 2 (1 from should_send_didopen, 1 from forward_didchange)
                // Note: Without an actual connection, the didChange send may fail,
                // but increment_document_version should still have been called
                let uri_string = virtual_uri.to_uri_string();
                let version = docs.get(&uri_string);
                assert!(
                    version.is_some(),
                    "Document should still be tracked after forward_didchange"
                );
                // Version should be incremented to 2
                assert_eq!(
                    *version.unwrap(),
                    2,
                    "Version should be incremented after forward_didchange"
                );
            } else {
                panic!("Should have lua documents tracked");
            }
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

        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);
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

    /// Test that forward_didchange_to_opened_docs skips unopened virtual documents.
    ///
    /// When a host document changes, we should NOT send didChange notifications
    /// for virtual documents that have NOT been opened (not in host_to_virtual).
    /// This test:
    /// 1. Calls forward_didchange_to_opened_docs with an injection that has no opened doc
    /// 2. Verifies no version increment happens (document not in document_versions)
    #[tokio::test]
    async fn forward_didchange_to_opened_docs_skips_unopened_docs() {
        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Do NOT call should_send_didopen - document is unopened
        // Now call forward_didchange_to_opened_docs with an injection
        let injections = vec![(
            "python".to_string(),
            "python-0".to_string(),
            "x = 42".to_string(),
        )];

        // Call the method - it should skip because no virtual doc is opened
        pool.forward_didchange_to_opened_docs(&host_uri, &injections)
            .await;

        // Verify no document version was created (document was never opened)
        {
            let versions = pool.document_versions.lock().await;
            // Should NOT have any python entries because document was never opened
            assert!(
                !versions.contains_key("python"),
                "Should NOT have python documents because none were opened"
            );
        }

        // Also verify host_to_virtual is empty
        {
            let host_map = pool.host_to_virtual.lock().await;
            assert!(
                !host_map.contains_key(&host_uri),
                "Should NOT have host entry because no document was opened"
            );
        }
    }

    /// Test that forward_didchange_to_opened_docs only sends didChange for opened docs in mixed scenario.
    ///
    /// When a host document changes with multiple injections:
    /// - Opened injections should get didChange (version incremented)
    /// - Unopened injections should be skipped (no version entry)
    #[tokio::test]
    async fn forward_didchange_mixed_opened_and_unopened_docs() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Open only the first Lua block
        let lua_virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);
        let opened = pool.should_send_didopen(&host_uri, &lua_virtual_uri).await;
        assert!(opened, "First call should open the document");
        // Also mark as opened (simulating successful didOpen write)
        pool.mark_document_opened(&lua_virtual_uri);

        // Do NOT open python

        // Now call forward_didchange_to_opened_docs with both injections
        let injections = vec![
            (
                "lua".to_string(),
                TEST_ULID_LUA_0.to_string(),
                "local x = 42".to_string(),
            ),
            (
                "python".to_string(),
                TEST_ULID_PYTHON_0.to_string(),
                "x = 42".to_string(),
            ),
        ];

        pool.forward_didchange_to_opened_docs(&host_uri, &injections)
            .await;

        // Verify:
        // 1. Lua document version was incremented (was opened)
        // 2. Python document version does NOT exist (was not opened)
        {
            let versions = pool.document_versions.lock().await;

            // Lua should have version 2
            let lua_docs = versions.get("lua").expect("Should have lua documents");
            let lua_uri_string = lua_virtual_uri.to_uri_string();
            let lua_version = lua_docs.get(&lua_uri_string);
            assert_eq!(
                lua_version,
                Some(&2),
                "Lua version should be 2 (1 from open, 1 from didChange)"
            );

            // Python should NOT exist
            assert!(
                !versions.contains_key("python"),
                "Should NOT have python documents because none were opened"
            );
        }
    }

    /// Test that ConnectionHandle is NOT cached after timeout.
    ///
    /// With the Reader Task architecture (ADR-0015 Phase A), failed connections
    /// are NOT cached because a ConnectionHandle requires:
    /// - A successfully split connection (writer + reader)
    /// - A running reader task
    /// - A response router
    ///
    /// When initialization times out, the connection is dropped and subsequent
    /// requests will spawn a fresh connection.
    #[tokio::test]
    async fn connection_handle_not_cached_after_timeout() {
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

        // With Reader Task architecture, ConnectionHandle is NOT cached after timeout
        let connections = pool.connections.lock().await;
        assert!(
            !connections.contains_key("test"),
            "ConnectionHandle should NOT be cached after timeout (Reader Task architecture)"
        );
    }

    // ========================================
    // Phase 3 Tests: remove_matching_virtual_docs
    // ========================================

    fn test_host_uri(name: &str) -> Url {
        Url::parse(&format!("file:///test/{}.md", name)).unwrap()
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_removes_matching_docs() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = test_host_uri("phase3_take");

        // Register some virtual docs using should_send_didopen
        // Use VirtualDocumentUri for proper type safety
        let virtual_uri_1 = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);
        let virtual_uri_2 = VirtualDocumentUri::new(&host_uri, "python", TEST_ULID_PYTHON_0);

        pool.should_send_didopen(&host_uri, &virtual_uri_1).await;
        pool.should_send_didopen(&host_uri, &virtual_uri_2).await;

        // Parse the ULIDs for matching
        let ulid_lua: ulid::Ulid = TEST_ULID_LUA_0.parse().unwrap();

        // Take only the Lua ULID
        let taken = pool
            .remove_matching_virtual_docs(&host_uri, &[ulid_lua])
            .await;

        // Should return the Lua doc
        assert_eq!(taken.len(), 1, "Should take exactly one doc");
        assert_eq!(
            taken[0].virtual_uri.language(),
            "lua",
            "Should be the Lua doc"
        );
        assert_eq!(
            taken[0].virtual_uri.region_id(),
            TEST_ULID_LUA_0,
            "Should have the Lua ULID"
        );

        // Verify remaining docs in host_to_virtual
        let host_map = pool.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Should have one remaining doc");
        assert_eq!(
            remaining[0].virtual_uri.language(),
            "python",
            "Python doc should remain"
        );
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_returns_empty_for_no_match() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = test_host_uri("phase3_no_match");

        // Register a virtual doc
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);
        pool.should_send_didopen(&host_uri, &virtual_uri).await;

        // Try to take a different ULID
        let other_ulid: ulid::Ulid = TEST_ULID_LUA_1.parse().unwrap();
        let taken = pool
            .remove_matching_virtual_docs(&host_uri, &[other_ulid])
            .await;

        assert!(taken.is_empty(), "Should return empty when no ULIDs match");

        // Original doc should still be there
        let host_map = pool.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Original doc should remain");
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_returns_empty_for_unknown_host() {
        let pool = LanguageServerPool::new();
        let host_uri = test_host_uri("phase3_unknown_host");

        let ulid: ulid::Ulid = TEST_ULID_LUA_0.parse().unwrap();
        let taken = pool.remove_matching_virtual_docs(&host_uri, &[ulid]).await;

        assert!(taken.is_empty(), "Should return empty for unknown host URI");
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_returns_empty_for_empty_ulids() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = test_host_uri("phase3_empty_ulids");

        // Register a virtual doc
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);
        pool.should_send_didopen(&host_uri, &virtual_uri).await;

        // Take with empty ULID list (fast path)
        let taken = pool.remove_matching_virtual_docs(&host_uri, &[]).await;

        assert!(taken.is_empty(), "Should return empty for empty ULID list");

        // Original doc should still be there
        let host_map = pool.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Original doc should remain");
    }

    #[tokio::test]
    async fn remove_matching_virtual_docs_takes_multiple_docs() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = test_host_uri("phase3_multiple");

        // Register multiple virtual docs using VirtualDocumentUri for proper type safety
        let virtual_uri_1 = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);
        let virtual_uri_2 = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_1);
        let virtual_uri_3 = VirtualDocumentUri::new(&host_uri, "python", TEST_ULID_PYTHON_0);

        pool.should_send_didopen(&host_uri, &virtual_uri_1).await;
        pool.should_send_didopen(&host_uri, &virtual_uri_2).await;
        pool.should_send_didopen(&host_uri, &virtual_uri_3).await;

        // Take both Lua ULIDs
        let ulid_1: ulid::Ulid = TEST_ULID_LUA_0.parse().unwrap();
        let ulid_2: ulid::Ulid = TEST_ULID_LUA_1.parse().unwrap();

        let taken = pool
            .remove_matching_virtual_docs(&host_uri, &[ulid_1, ulid_2])
            .await;

        assert_eq!(taken.len(), 2, "Should take both Lua docs");

        // Verify Python doc remains
        let host_map = pool.host_to_virtual.lock().await;
        let remaining = host_map.get(&host_uri).unwrap();
        assert_eq!(remaining.len(), 1, "Python doc should remain");
        assert_eq!(
            remaining[0].virtual_uri.language(),
            "python",
            "Remaining doc should be Python"
        );
    }

    // ========================================
    // ADR-0015: is_document_opened tests
    // ========================================

    /// Test that is_document_opened returns false before mark_document_opened is called.
    ///
    /// This is part of the fix for LSP spec violation where requests were sent
    /// before didOpen. The is_document_opened() method checks whether didOpen
    /// has ACTUALLY been sent to the downstream server (not just marked for sending).
    #[tokio::test]
    async fn is_document_opened_returns_false_before_marked() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);

        // Before marking, should return false
        assert!(
            !pool.is_document_opened(&virtual_uri),
            "is_document_opened should return false before mark_document_opened"
        );
    }

    /// Test that is_document_opened returns true after mark_document_opened is called.
    #[tokio::test]
    async fn is_document_opened_returns_true_after_marked() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);

        // Mark the document as opened
        pool.mark_document_opened(&virtual_uri);

        // After marking, should return true
        assert!(
            pool.is_document_opened(&virtual_uri),
            "is_document_opened should return true after mark_document_opened"
        );
    }

    /// Test that should_send_didopen does NOT mark document as opened.
    ///
    /// should_send_didopen only reserves the document version for tracking.
    /// The actual "opened" state should only be set by mark_document_opened
    /// which is called AFTER didOpen is sent to downstream.
    #[tokio::test]
    async fn should_send_didopen_does_not_mark_as_opened() {
        use super::super::protocol::VirtualDocumentUri;

        let pool = LanguageServerPool::new();
        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", TEST_ULID_LUA_0);

        // Call should_send_didopen - this reserves the version but doesn't mark as opened
        let should_open = pool.should_send_didopen(&host_uri, &virtual_uri).await;
        assert!(should_open, "First call should return true");

        // is_document_opened should still return false
        assert!(
            !pool.is_document_opened(&virtual_uri),
            "is_document_opened should return false even after should_send_didopen"
        );
    }
}
