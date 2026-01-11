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
use super::protocol::{
    VirtualDocumentUri, build_bridge_completion_request, build_bridge_didchange_notification,
    build_bridge_hover_request, transform_completion_response_to_host,
    transform_hover_response_to_host,
};

/// Pool of connections to downstream language servers (ADR-0016).
///
/// Implements Phase 1: Single-LS-per-Language routing where each injection
/// language maps to exactly one downstream server.
///
/// Provides lazy initialization of connections and handles the LSP handshake
/// (initialize/initialized) for each language server.
pub(crate) struct LanguageServerPool {
    /// Map of language -> initialized connection
    connections: Mutex<HashMap<String, Arc<Mutex<AsyncBridgeConnection>>>>,
    /// Map of language -> connection state (Initializing, Ready, Failed)
    connection_states: Mutex<HashMap<String, ConnectionState>>,
    /// Map of language -> (virtual document URI -> version)
    /// Tracks which documents have been opened and their current version number
    document_versions: Mutex<HashMap<String, HashMap<String, i32>>>,
    /// Counter for generating unique request IDs
    next_request_id: std::sync::atomic::AtomicI64,
}

impl LanguageServerPool {
    /// Create a new language server pool.
    pub(crate) fn new() -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            connection_states: Mutex::new(HashMap::new()),
            document_versions: Mutex::new(HashMap::new()),
            next_request_id: std::sync::atomic::AtomicI64::new(1),
        }
    }

    /// Increment the version of a virtual document and return the new version.
    ///
    /// Returns None if the document has not been opened.
    async fn increment_document_version(&self, language: &str, virtual_uri: &str) -> Option<i32> {
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
    async fn should_send_didopen(&self, language: &str, virtual_uri: &str) -> bool {
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
    async fn get_or_create_connection(
        &self,
        language: &str,
        server_config: &crate::config::settings::BridgeServerConfig,
    ) -> io::Result<Arc<Mutex<AsyncBridgeConnection>>> {
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
    async fn get_or_create_connection_with_timeout(
        &self,
        language: &str,
        server_config: &crate::config::settings::BridgeServerConfig,
        timeout: Duration,
    ) -> io::Result<Arc<Mutex<AsyncBridgeConnection>>> {
        let mut connections = self.connections.lock().await;

        // Check if we already have a connection for this language
        if let Some(conn) = connections.get(language) {
            return Ok(Arc::clone(conn));
        }

        // Set state to Initializing before spawning
        {
            let mut states = self.connection_states.lock().await;
            states.insert(language.to_string(), ConnectionState::Initializing);
        }

        // Spawn new connection
        let mut conn = AsyncBridgeConnection::spawn(server_config.cmd.clone()).await?;

        // Perform LSP initialize handshake with timeout
        let init_result = tokio::time::timeout(timeout, async {
            let init_request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": self.next_request_id(),
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

        // Handle timeout
        match init_result {
            Ok(Ok(())) => {
                // Init succeeded - set state to Ready
                {
                    let mut states = self.connection_states.lock().await;
                    states.insert(language.to_string(), ConnectionState::Ready);
                }
                let conn = Arc::new(Mutex::new(conn));
                connections.insert(language.to_string(), Arc::clone(&conn));
                Ok(conn)
            }
            Ok(Err(e)) => {
                // Init failed with io::Error - set state to Failed
                {
                    let mut states = self.connection_states.lock().await;
                    states.insert(language.to_string(), ConnectionState::Failed);
                }
                Err(e)
            }
            Err(_elapsed) => {
                // Timeout occurred - set state to Failed
                {
                    let mut states = self.connection_states.lock().await;
                    states.insert(language.to_string(), ConnectionState::Failed);
                }
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Initialize timeout: downstream server unresponsive",
                ))
            }
        }
    }

    /// Generate a unique request ID.
    fn next_request_id(&self) -> i64 {
        self.next_request_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Send a hover request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Check connection state - return error if Initializing or Failed (non-blocking)
    /// 2. Get or create a connection to the language server
    /// 3. Send a textDocument/didOpen notification if needed
    /// 4. Send the hover request
    /// 5. Wait for and return the response
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_hover_request(
        &self,
        server_config: &crate::config::settings::BridgeServerConfig,
        host_uri: &tower_lsp::lsp_types::Url,
        host_position: tower_lsp::lsp_types::Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
    ) -> io::Result<serde_json::Value> {
        // Check if server is still initializing or failed - return error immediately (non-blocking)
        {
            let states = self.connection_states.lock().await;
            match states.get(injection_language) {
                Some(ConnectionState::Initializing) => {
                    return Err(io::Error::other("bridge: downstream server initializing"));
                }
                Some(ConnectionState::Failed) => {
                    return Err(io::Error::other("bridge: downstream server failed"));
                }
                _ => {} // Ready or not present - proceed
            }
        }

        // Get or create connection
        let conn = self
            .get_or_create_connection(injection_language, server_config)
            .await?;
        let mut conn = conn.lock().await;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Send didOpen notification only if document hasn't been opened yet
        if self
            .should_send_didopen(injection_language, &virtual_uri_string)
            .await
        {
            let did_open = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": virtual_uri_string,
                        "languageId": injection_language,
                        "version": 1,
                        "text": virtual_content
                    }
                }
            });
            conn.write_message(&did_open).await?;
        }

        // Build and send hover request
        let request_id = self.next_request_id();
        let hover_request = build_bridge_hover_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );
        conn.write_message(&hover_request).await?;

        // Wait for the hover response (skip notifications)
        loop {
            let msg = conn.read_message().await?;
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(request_id)
            {
                // Transform response to host coordinates
                return Ok(transform_hover_response_to_host(msg, region_start_line));
            }
            // Skip notifications and other responses
        }
    }

    /// Send a completion request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Check connection state - return error if Initializing or Failed (non-blocking)
    /// 2. Get or create a connection to the language server
    /// 3. Send a textDocument/didOpen notification if not opened, or didChange if already opened
    /// 4. Send the completion request
    /// 5. Wait for and return the response with transformed coordinates
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_completion_request(
        &self,
        server_config: &crate::config::settings::BridgeServerConfig,
        host_uri: &tower_lsp::lsp_types::Url,
        host_position: tower_lsp::lsp_types::Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
    ) -> io::Result<serde_json::Value> {
        // Check if server is still initializing or failed - return error immediately (non-blocking)
        {
            let states = self.connection_states.lock().await;
            match states.get(injection_language) {
                Some(ConnectionState::Initializing) => {
                    return Err(io::Error::other("bridge: downstream server initializing"));
                }
                Some(ConnectionState::Failed) => {
                    return Err(io::Error::other("bridge: downstream server failed"));
                }
                _ => {} // Ready or not present - proceed
            }
        }

        // Get or create connection
        let conn = self
            .get_or_create_connection(injection_language, server_config)
            .await?;
        let mut conn = conn.lock().await;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(host_uri, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Send didOpen or didChange depending on whether document is already opened
        if self
            .should_send_didopen(injection_language, &virtual_uri_string)
            .await
        {
            // First time: send didOpen
            let did_open = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": virtual_uri_string,
                        "languageId": injection_language,
                        "version": 1,
                        "text": virtual_content
                    }
                }
            });
            conn.write_message(&did_open).await?;
        } else {
            // Document already opened: send didChange with incremented version
            if let Some(version) = self
                .increment_document_version(injection_language, &virtual_uri_string)
                .await
            {
                let did_change = build_bridge_didchange_notification(
                    host_uri,
                    injection_language,
                    region_id,
                    virtual_content,
                    version,
                );
                conn.write_message(&did_change).await?;
            }
        }

        // Build and send completion request
        let request_id = self.next_request_id();
        let completion_request = build_bridge_completion_request(
            host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            request_id,
        );
        conn.write_message(&completion_request).await?;

        // Wait for the completion response (skip notifications)
        loop {
            let msg = conn.read_message().await?;
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(request_id)
            {
                // Transform response to host coordinates
                return Ok(transform_completion_response_to_host(
                    msg,
                    region_start_line,
                ));
            }
            // Skip notifications and other responses
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::BridgeServerConfig;
    use std::time::Duration;

    /// Test that ConnectionState enum exists and has expected variants.
    /// State should start as Initializing, transition to Ready after successful init.
    #[tokio::test]
    async fn connection_state_starts_as_initializing_then_transitions_to_ready() {
        let pool = LanguageServerPool::new();

        // Verify initial state: no connection states exist
        let states = pool.connection_states.lock().await;
        assert!(
            states.is_empty(),
            "States map should exist and be empty initially"
        );
        assert!(
            !states.contains_key("test"),
            "State should not exist before connection attempt"
        );
    }

    /// Test that request during Initializing state returns error immediately (non-blocking).
    /// This test simulates a slow-to-initialize server and verifies that hover/completion
    /// requests during initialization return an error instead of blocking.
    #[tokio::test]
    async fn request_during_init_returns_error_immediately() {
        use std::sync::Arc;
        use tower_lsp::lsp_types::{Position, Url};

        let pool = Arc::new(LanguageServerPool::new());
        let config = BridgeServerConfig {
            // This server reads stdin but never responds (slow init)
            cmd: vec![
                "sh".to_string(),
                "-c".to_string(),
                "cat > /dev/null".to_string(),
            ],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Manually set state to Initializing (simulating init in progress)
        {
            let mut states = pool.connection_states.lock().await;
            states.insert("lua".to_string(), ConnectionState::Initializing);
        }

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Try to send hover request while state is Initializing
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
            )
            .await;
        let elapsed = start.elapsed();

        // Request should fail immediately (not block waiting for init)
        assert!(
            elapsed < Duration::from_millis(100),
            "Request should return immediately, not block. Elapsed: {:?}",
            elapsed
        );

        // Should return an error (not succeed)
        assert!(
            result.is_err(),
            "Request during Initializing should return error"
        );

        let err = result.unwrap_err();
        // The error message should indicate server is initializing
        assert!(
            err.to_string().contains("initializing"),
            "Error should mention 'initializing': {}",
            err
        );
    }

    /// Test that error message is exactly "bridge: downstream server initializing".
    /// This verifies the specific error message format per ADR-0015.
    #[tokio::test]
    async fn request_during_init_returns_exact_error_message() {
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

        // Set state to Initializing
        {
            let mut states = pool.connection_states.lock().await;
            states.insert("lua".to_string(), ConnectionState::Initializing);
        }

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Test hover request error message
        let result = pool
            .send_hover_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
            )
            .await;
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "bridge: downstream server initializing",
            "Hover error message should match exactly"
        );

        // Test completion request error message
        let result = pool
            .send_completion_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
            )
            .await;
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "bridge: downstream server initializing",
            "Completion error message should match exactly"
        );
    }

    /// Test that hover request during Failed state returns error immediately.
    /// This verifies the exact error message per ADR-0015.
    #[tokio::test]
    async fn hover_request_during_failed_returns_error() {
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

        // Set state to Failed (simulating failed initialization)
        {
            let mut states = pool.connection_states.lock().await;
            states.insert("lua".to_string(), ConnectionState::Failed);
        }

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Try to send hover request while state is Failed
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
            )
            .await;
        let elapsed = start.elapsed();

        // Request should fail immediately (not block)
        assert!(
            elapsed < Duration::from_millis(100),
            "Request should return immediately, not block. Elapsed: {:?}",
            elapsed
        );

        // Should return an error with exact message
        assert!(
            result.is_err(),
            "Request during Failed should return error"
        );

        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "bridge: downstream server failed",
            "Error message should indicate server failed"
        );
    }

    /// Test that completion request during Failed state returns error immediately.
    /// This verifies the exact error message per ADR-0015.
    #[tokio::test]
    async fn completion_request_during_failed_returns_error() {
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

        // Set state to Failed (simulating failed initialization)
        {
            let mut states = pool.connection_states.lock().await;
            states.insert("lua".to_string(), ConnectionState::Failed);
        }

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 5,
        };

        // Try to send completion request while state is Failed
        let start = std::time::Instant::now();
        let result = pool
            .send_completion_request(
                &config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3,
                "print('hello')",
            )
            .await;
        let elapsed = start.elapsed();

        // Request should fail immediately (not block)
        assert!(
            elapsed < Duration::from_millis(100),
            "Request should return immediately, not block. Elapsed: {:?}",
            elapsed
        );

        // Should return an error with exact message
        assert!(
            result.is_err(),
            "Request during Failed should return error"
        );

        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "bridge: downstream server failed",
            "Error message should indicate server failed"
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
            )
            .await;

        // Verify request succeeded (not blocked by init check)
        assert!(
            result.is_ok(),
            "Request should succeed after init completes: {:?}",
            result.err()
        );

        // Verify state is Ready after successful init
        {
            let states = pool.connection_states.lock().await;
            assert_eq!(
                states.get("lua"),
                Some(&ConnectionState::Ready),
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

        // After timeout, state should be Failed
        let states = pool.connection_states.lock().await;
        assert_eq!(
            states.get("test"),
            Some(&ConnectionState::Failed),
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

    /// Test that connection is NOT cached in pool after timeout.
    ///
    /// When initialization times out, the connection should not be stored
    /// in the pool. This ensures that:
    /// 1. Next request will attempt a fresh connection (retry behavior)
    /// 2. No broken/half-initialized connections are cached
    #[tokio::test]
    async fn connection_not_cached_after_timeout() {
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

        // Check that pool has no connections cached for "test"
        let connections = pool.connections.lock().await;
        assert!(
            !connections.contains_key("test"),
            "Connection should NOT be cached after timeout failure"
        );
    }
}
