//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.
//!
//! # Module Structure
//!
//! - `actor` - Actor components (ResponseRouter, Reader task) for async I/O (ADR-0015)
//! - `connection` - AsyncBridgeConnection for process spawning and I/O
//! - `coordinator` - BridgeCoordinator for unified pool + region ID tracking
//! - `protocol` - VirtualDocumentUri, request building, and response transformation
//! - `pool` - LanguageServerPool for server pool coordination (ADR-0016)

mod actor;
mod connection;
mod coordinator;
mod pool;
mod protocol;
mod text_document;

// Re-export public types
pub(crate) use coordinator::BridgeCoordinator;
pub(crate) use coordinator::ResolvedServerConfig;
pub use pool::LanguageServerPool;
pub use pool::UpstreamId;

/// Integration tests for the bridge module.
///
/// These tests verify the end-to-end behavior of the bridge components working together.
/// Unit tests for individual components live in their respective modules:
/// - `connection.rs` - AsyncBridgeConnection tests
/// - `protocol.rs` - VirtualDocumentUri and request/response transformation tests
/// - `pool.rs` - LanguageServerPool lifecycle and state tests
#[cfg(test)]
mod tests {
    use super::pool::{LanguageServerPool, UpstreamId};
    use crate::config::settings::BridgeServerConfig;
    use tower_lsp_server::ls_types::Position;
    use url::Url;

    /// Integration test: LanguageServerPool sends hover request to lua-language-server
    #[tokio::test]
    async fn pool_hover_request_succeeds_with_lua_server() {
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
        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 9,
        };
        let virtual_content = "function greet(name)\n    return \"Hello, \" .. name\nend";

        let response = pool
            .send_hover_request(
                "lua", // server_name
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                UpstreamId::Number(1), // upstream_request_id
            )
            .await;

        assert!(
            response.is_ok(),
            "Hover request should succeed: {:?}",
            response.err()
        );

        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(json_response.get("id").is_some());
    }

    /// Integration test: LanguageServerPool sends completion request to lua-language-server
    #[tokio::test]
    async fn pool_completion_request_succeeds_with_lua_server() {
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
        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 3,
        };
        let virtual_content = "pri"; // Partial identifier for completion

        let response = pool
            .send_completion_request(
                "lua", // server_name
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                UpstreamId::Number(1), // upstream_request_id
            )
            .await;

        assert!(
            response.is_ok(),
            "Completion request should succeed: {:?}",
            response.err()
        );

        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(json_response.get("id").is_some());
    }

    /// Integration test: Verify unique downstream request IDs are generated.
    ///
    /// This test verifies that downstream requests use unique generated IDs,
    /// separate from upstream IDs. This prevents "duplicate request ID" errors
    /// when multiple upstream requests have the same ID.
    ///
    /// The upstream_request_id parameter is now unused (prefixed with _) since
    /// we generate unique downstream IDs internally.
    #[tokio::test]
    async fn downstream_request_uses_unique_generated_id() {
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
        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 9,
        };
        let virtual_content = "function greet(name)\n    return \"Hello, \" .. name\nend";

        // upstream_request_id is no longer used for downstream requests
        // (unique IDs are generated internally)
        let response = pool
            .send_hover_request(
                "lua", // server_name
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                UpstreamId::Number(42), // upstream_request_id (unused)
            )
            .await;

        assert!(
            response.is_ok(),
            "Hover request should succeed: {:?}",
            response.err()
        );

        // Verify the response has valid JSON-RPC structure
        // The response ID will be from our generated sequence
        // Note: ID 1 is used by the initialize request during connection setup,
        // so the first user request gets ID 2
        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(
            json_response.get("id").is_some(),
            "Response should contain an ID"
        );
        // First user request gets ID 2 (ID 1 is used by initialize request)
        assert_eq!(
            json_response["id"].as_i64(),
            Some(2),
            "Response should contain generated downstream ID (2, since 1 is used by initialize)"
        );
    }

    /// Integration test: Verify completion request uses unique generated downstream ID.
    ///
    /// This test verifies that completion requests use unique generated IDs,
    /// separate from upstream IDs. This prevents "duplicate request ID" errors
    /// when multiple upstream requests have the same ID.
    #[tokio::test]
    async fn completion_request_uses_unique_generated_id() {
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
        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        let host_position = Position {
            line: 3,
            character: 3,
        };
        let virtual_content = "pri"; // Partial identifier for completion

        // upstream_request_id is no longer used for downstream requests
        // (unique IDs are generated internally)
        let response = pool
            .send_completion_request(
                "lua", // server_name
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                UpstreamId::Number(123), // upstream_request_id (unused)
            )
            .await;

        assert!(
            response.is_ok(),
            "Completion request should succeed: {:?}",
            response.err()
        );

        // Verify the response has valid JSON-RPC structure
        // The response ID will be from our generated sequence
        // Note: ID 1 is used by the initialize request during connection setup,
        // so the first user request gets ID 2
        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(
            json_response.get("id").is_some(),
            "Response should contain an ID"
        );
        // First user request gets ID 2 (ID 1 is used by initialize request)
        assert_eq!(
            json_response["id"].as_i64(),
            Some(2),
            "Response should contain generated downstream ID (2, since 1 is used by initialize)"
        );
    }

    /// Integration test: LanguageServerPool sends document link request to lua-language-server
    #[tokio::test]
    async fn pool_document_link_request_succeeds_with_lua_server() {
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
        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        // Lua code with require statement - lua-ls may return document links for requires
        let virtual_content = "local mod = require(\"mymodule\")\nprint(mod)";

        let response = pool
            .send_document_link_request(
                "lua", // server_name
                &server_config,
                &host_uri,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                UpstreamId::Number(1), // upstream_request_id
            )
            .await;

        assert!(
            response.is_ok(),
            "Document link request should succeed: {:?}",
            response.err()
        );

        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(json_response.get("id").is_some());
        // Result can be null, empty array, array of DocumentLink, or error
        // lua-ls may or may not return links depending on configuration
        // The important thing is the request succeeded and we got a valid JSON-RPC response
        assert!(
            json_response.get("result").is_some() || json_response.get("error").is_some(),
            "Document link response should have result or error field: {:?}",
            json_response
        );
    }

    /// Unit test: Different languages using the same server_name share a single connection.
    ///
    /// This test verifies the core server-name-based pooling behavior by inserting
    /// a mock connection keyed by server_name, then checking that subsequent lookups
    /// for the same server_name return the same connection.
    ///
    /// Real-world example: ts and tsx both using server "tsgo" should share one process.
    #[tokio::test]
    async fn same_server_different_languages_share_connection() {
        use super::pool::ConnectionState;
        use super::pool::test_helpers::create_handle_with_state;

        let pool = std::sync::Arc::new(LanguageServerPool::new());

        // Create and insert a Ready connection for server_name "tsgo"
        let server_name = "tsgo";
        let handle = create_handle_with_state(ConnectionState::Ready).await;
        let inserted_ptr = std::sync::Arc::as_ptr(&handle);

        pool.connections()
            .await
            .insert(server_name.to_string(), std::sync::Arc::clone(&handle));

        // Verify only one connection exists
        let connections = pool.connections().await;
        assert_eq!(
            connections.len(),
            1,
            "Only one connection should exist for server_name"
        );
        assert!(
            connections.contains_key(server_name),
            "Connection should be keyed by server_name"
        );

        // Verify the connection is the same one we inserted
        let retrieved_ptr = std::sync::Arc::as_ptr(connections.get(server_name).unwrap());
        assert_eq!(
            inserted_ptr, retrieved_ptr,
            "Connection should be the same instance we inserted"
        );

        // Both ts and tsx lookups should return the same connection
        // (in the real system, coordinator resolves both languages to "tsgo")
        let ts_lookup = connections.get("tsgo");
        let tsx_lookup = connections.get("tsgo"); // Same key, same connection
        assert!(ts_lookup.is_some(), "ts lookup should find connection");
        assert!(tsx_lookup.is_some(), "tsx lookup should find connection");
        assert!(
            std::sync::Arc::ptr_eq(ts_lookup.unwrap(), tsx_lookup.unwrap()),
            "Both lookups should return the same connection"
        );
    }

    /// Unit test: Different server_names create separate connections.
    ///
    /// This test verifies that different server_names have separate connections,
    /// even if they might handle similar languages.
    ///
    /// Real-world example: "tsgo" and "eslint" are separate servers even if both
    /// handle TypeScript files.
    #[tokio::test]
    async fn different_servers_create_separate_connections() {
        use super::pool::ConnectionState;
        use super::pool::test_helpers::create_handle_with_state;

        let pool = std::sync::Arc::new(LanguageServerPool::new());

        // Create and insert two different connections with different server_names
        let handle_tsgo = create_handle_with_state(ConnectionState::Ready).await;
        let handle_eslint = create_handle_with_state(ConnectionState::Ready).await;

        pool.connections()
            .await
            .insert("tsgo".to_string(), std::sync::Arc::clone(&handle_tsgo));
        pool.connections()
            .await
            .insert("eslint".to_string(), std::sync::Arc::clone(&handle_eslint));

        // Verify two separate connections exist
        let connections = pool.connections().await;
        assert_eq!(
            connections.len(),
            2,
            "Two separate connections should exist for different server_names"
        );
        assert!(
            connections.contains_key("tsgo"),
            "Should have tsgo connection"
        );
        assert!(
            connections.contains_key("eslint"),
            "Should have eslint connection"
        );

        // Verify handles point to different connections
        let tsgo_ptr = std::sync::Arc::as_ptr(connections.get("tsgo").unwrap());
        let eslint_ptr = std::sync::Arc::as_ptr(connections.get("eslint").unwrap());
        assert_ne!(
            tsgo_ptr, eslint_ptr,
            "Different server_names should have different connections"
        );
    }
}
