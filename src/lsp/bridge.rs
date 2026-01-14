//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.
//!
//! # Module Structure
//!
//! - `connection` - AsyncBridgeConnection for process spawning and I/O
//! - `protocol` - VirtualDocumentUri, request building, and response transformation
//! - `pool` - LanguageServerPool for server pool coordination (ADR-0016)

mod connection;
mod pool;
mod protocol;
mod text_document;

// Re-export public types
pub(crate) use pool::LanguageServerPool;

/// Integration tests for the bridge module.
///
/// These tests verify the end-to-end behavior of the bridge components working together.
/// Unit tests for individual components live in their respective modules:
/// - `connection.rs` - AsyncBridgeConnection tests
/// - `protocol.rs` - VirtualDocumentUri and request/response transformation tests
/// - `pool.rs` - LanguageServerPool lifecycle and state tests
#[cfg(test)]
mod tests {
    use super::pool::LanguageServerPool;
    use crate::config::settings::BridgeServerConfig;
    use tower_lsp::lsp_types::{Position, Url};

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
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                1, // upstream_request_id
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
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                1, // upstream_request_id
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

    /// Integration test: Verify upstream request ID flows to downstream unchanged (ADR-0016).
    ///
    /// This test verifies the full request ID passthrough flow:
    /// 1. Send hover request with explicit upstream_request_id = 42
    /// 2. Verify the downstream server receives the same ID
    /// 3. Verify the response contains the same ID
    #[tokio::test]
    async fn upstream_request_id_flows_to_downstream_unchanged() {
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

        // Use a specific request ID (42) to verify it flows through unchanged
        let upstream_request_id = 42;
        let response = pool
            .send_hover_request(
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                upstream_request_id,
            )
            .await;

        assert!(
            response.is_ok(),
            "Hover request should succeed: {:?}",
            response.err()
        );

        // Verify the response contains the same request ID we sent
        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert_eq!(
            json_response["id"].as_i64(),
            Some(upstream_request_id),
            "Response should contain the upstream request ID"
        );
    }

    /// Integration test: Verify completion request ID flows through unchanged (ADR-0016).
    #[tokio::test]
    async fn completion_request_id_flows_to_downstream_unchanged() {
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

        // Use a specific request ID (123) to verify it flows through unchanged
        let upstream_request_id = 123;
        let response = pool
            .send_completion_request(
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                upstream_request_id,
            )
            .await;

        assert!(
            response.is_ok(),
            "Completion request should succeed: {:?}",
            response.err()
        );

        // Verify the response contains the same request ID we sent
        let json_response = response.unwrap();
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert_eq!(
            json_response["id"].as_i64(),
            Some(upstream_request_id),
            "Response should contain the upstream request ID"
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
                &server_config,
                &host_uri,
                "lua",
                "region-0",
                3, // region_start_line
                virtual_content,
                1, // upstream_request_id
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
}
