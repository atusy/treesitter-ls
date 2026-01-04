//! LanguageServerPool for managing multiple language server connections

use dashmap::DashMap;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::connection::BridgeConnection;

/// Pool for managing language server connections
///
/// Manages one BridgeConnection per language, spawning on first access.
pub(crate) struct LanguageServerPool {
    /// Map from language name (e.g., "lua") to BridgeConnection
    /// Arc enables sharing connections across async tasks
    /// DashMap provides concurrent access without explicit locking
    connections: DashMap<String, Arc<BridgeConnection>>,
}

impl LanguageServerPool {
    /// Creates a new LanguageServerPool
    ///
    /// Starts with no connections; connections are spawned on first access.
    pub(crate) fn new() -> Self {
        Self {
            connections: DashMap::new(),
        }
    }

    /// Gets or spawns a BridgeConnection for the specified language
    ///
    /// # Arguments
    /// * `language` - Language name (e.g., "lua")
    ///
    /// # Returns
    /// Arc<BridgeConnection> for the language
    ///
    /// # Errors
    /// Returns error if:
    /// - Language server command not found (e.g., "lua-language-server" not in PATH)
    /// - Failed to spawn language server process
    /// - Initialize handshake failed
    async fn get_or_spawn_connection(
        &self,
        language: &str,
    ) -> std::result::Result<Arc<BridgeConnection>, String> {
        // Check if connection already exists
        if let Some(conn) = self.connections.get(language) {
            return Ok(conn.clone());
        }

        // Spawn new connection
        // Map language name to language server command
        // For MVP, hardcode lua -> lua-language-server
        // TODO: Make this configurable via settings
        let command = match language {
            "lua" => "lua-language-server",
            _ => return Err(format!("No language server configured for language: {}", language)),
        };

        // Spawn and initialize the language server
        let connection = BridgeConnection::new(command).await?;
        connection.initialize().await?;

        let arc_conn = Arc::new(connection);

        // Insert into map (use entry API to handle race condition)
        self.connections
            .entry(language.to_string())
            .or_insert(arc_conn.clone());

        Ok(arc_conn)
    }

    /// Handles textDocument/completion request
    ///
    /// # Arguments
    /// * `params` - Completion parameters including virtual document URI and translated position
    ///
    /// # Returns
    /// Completion response from language server, or None if no connection
    pub(crate) async fn completion(
        &self,
        _params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        // TODO(PBI-180a Phase 2): Implement real LSP completion request
        // For now, return Ok(None) - real implementation will:
        // 1. Get/create BridgeConnection for language
        // 2. Call connection.send_request("textDocument/completion", params)
        // 3. Deserialize response into CompletionResponse
        Ok(None)
    }

    /// Handles textDocument/hover request
    ///
    /// Fakeit implementation: returns Ok(None) immediately.
    pub(crate) async fn hover(&self, _params: HoverParams) -> Result<Option<Hover>> {
        Ok(None)
    }

    /// Handles textDocument/definition request
    ///
    /// Fakeit implementation: returns Ok(None) immediately.
    pub(crate) async fn definition(
        &self,
        _params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        Ok(None)
    }

    /// Handles textDocument/signatureHelp request
    ///
    /// Fakeit implementation: returns Ok(None) immediately.
    pub(crate) async fn signature_help(
        &self,
        _params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pool_new_creates_empty_connections_map() {
        let pool = LanguageServerPool::new();
        assert_eq!(
            pool.connections.len(),
            0,
            "New pool should have no connections"
        );
    }

    #[tokio::test]
    async fn test_get_or_spawn_connection_spawns_new_connection_on_first_access() {
        // This test requires lua-language-server in PATH
        // Skip if not available
        let check = tokio::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .await;

        if check.is_err() {
            eprintln!("SKIP: lua-language-server not found in PATH");
            return;
        }

        let pool = LanguageServerPool::new();
        assert_eq!(pool.connections.len(), 0, "Pool should start empty");

        // First access should spawn connection
        let result = pool.get_or_spawn_connection("lua").await;
        assert!(
            result.is_ok(),
            "Should spawn lua-language-server: {:?}",
            result.err()
        );

        assert_eq!(
            pool.connections.len(),
            1,
            "Pool should have one connection after first access"
        );
    }

    #[tokio::test]
    async fn test_get_or_spawn_connection_reuses_existing_connection() {
        // This test requires lua-language-server in PATH
        // Skip if not available
        let check = tokio::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .await;

        if check.is_err() {
            eprintln!("SKIP: lua-language-server not found in PATH");
            return;
        }

        let pool = LanguageServerPool::new();

        // First access - spawns connection
        let conn1 = pool.get_or_spawn_connection("lua").await.unwrap();

        // Second access - should reuse connection
        let conn2 = pool.get_or_spawn_connection("lua").await.unwrap();

        // Both should point to same Arc (same memory address)
        assert!(
            Arc::ptr_eq(&conn1, &conn2),
            "Second access should reuse existing connection"
        );

        assert_eq!(
            pool.connections.len(),
            1,
            "Pool should still have one connection"
        );
    }

    #[tokio::test]
    async fn test_get_or_spawn_connection_returns_error_for_unsupported_language() {
        let pool = LanguageServerPool::new();

        let result = pool.get_or_spawn_connection("python").await;
        assert!(
            result.is_err(),
            "Should return error for unsupported language"
        );

        let error = result.unwrap_err();
        assert!(
            error.contains("No language server configured"),
            "Error should mention language not configured: {}",
            error
        );
    }

    #[tokio::test]
    async fn test_pool_completion_returns_ok_none() {
        let pool = LanguageServerPool::new();
        let params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: "file:///test.lua".parse().unwrap(),
                },
                position: Position::new(0, 0),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        };

        let result = pool.completion(params).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_pool_hover_returns_ok_none() {
        let pool = LanguageServerPool::new();
        let params = HoverParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: "file:///test.lua".parse().unwrap(),
                },
                position: Position::new(0, 0),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
        };

        let result = pool.hover(params).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_pool_definition_returns_ok_none() {
        let pool = LanguageServerPool::new();
        let params = GotoDefinitionParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: "file:///test.lua".parse().unwrap(),
                },
                position: Position::new(0, 0),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = pool.definition(params).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_pool_signature_help_returns_ok_none() {
        let pool = LanguageServerPool::new();
        let params = SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier {
                    uri: "file:///test.lua".parse().unwrap(),
                },
                position: Position::new(0, 0),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            context: None,
        };

        let result = pool.signature_help(params).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
