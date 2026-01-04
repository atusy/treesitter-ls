//! LanguageServerPool for managing multiple language server connections

use dashmap::DashMap;
use std::sync::Arc;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::connection::{BridgeConnection, IncrementalType};

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
    /// Returns immediately without blocking on initialization.
    /// Initialization runs in a background task (tokio::spawn).
    ///
    /// # Arguments
    /// * `language` - Language name (e.g., "lua")
    ///
    /// # Returns
    /// Arc<BridgeConnection> for the language (may not be initialized yet)
    ///
    /// # Errors
    /// Returns error if:
    /// - Language server command not found (e.g., "lua-language-server" not in PATH)
    /// - Failed to spawn language server process
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
            _ => {
                return Err(format!(
                    "No language server configured for language: {}",
                    language
                ));
            }
        };

        // Spawn the language server process (but don't initialize yet)
        let connection = BridgeConnection::new(command).await?;
        let arc_conn = Arc::new(connection);

        // Insert into map (use entry API to handle race condition)
        self.connections
            .entry(language.to_string())
            .or_insert(arc_conn.clone());

        // Spawn background task to run initialization
        // This prevents blocking the caller
        let conn_for_init = arc_conn.clone();
        let language_owned = language.to_string();
        tokio::spawn(async move {
            // Run initialization in background
            // Errors are logged but don't fail the spawn
            if let Err(e) = conn_for_init.initialize().await {
                eprintln!(
                    "Background initialization failed for {}: {}",
                    language_owned, e
                );
            }
        });

        Ok(arc_conn)
    }

    /// Extracts language from virtual document URI
    ///
    /// Expected format: file:///virtual/{language}/{hash}.{ext}
    /// Example: file:///virtual/lua/abc123.lua -> "lua"
    fn extract_language_from_uri(uri: &Url) -> Option<String> {
        let path = uri.path();
        let parts: Vec<&str> = path.split('/').collect();

        // Path format: /virtual/{language}/{hash}.{ext}
        // parts: ["", "virtual", "{language}", "{hash}.{ext}"]
        if parts.len() >= 4 && parts[1] == "virtual" {
            Some(parts[2].to_string())
        } else {
            None
        }
    }

    /// Handles textDocument/completion request
    ///
    /// # Arguments
    /// * `params` - Completion parameters including virtual document URI and translated position
    /// * `content` - Virtual document content to send via didOpen on first access
    ///
    /// # Returns
    /// Completion response from language server, or None if no connection
    pub(crate) async fn completion(
        &self,
        params: CompletionParams,
        content: String,
    ) -> Result<Option<CompletionResponse>> {
        // Extract language from virtual URI
        let uri = &params.text_document_position.text_document.uri;
        let Some(language) = Self::extract_language_from_uri(uri) else {
            // Not a virtual URI - return None
            return Ok(None);
        };

        // Get or spawn connection for this language
        let connection = self.get_or_spawn_connection(&language).await.map_err(|e| {
            tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("Failed to get language server for {}: {}", language, e).into(),
                data: None,
            }
        })?;

        // Wait for connection to be initialized (with 5s timeout)
        connection
            .wait_for_initialized(std::time::Duration::from_secs(5))
            .await
            .map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: e.into(),
                data: None,
            })?;

        // Send didOpen with virtual document content on first access
        let virtual_uri_str = uri.to_string();
        connection
            .check_and_send_did_open(&virtual_uri_str, &language, &content)
            .await
            .map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("Failed to send didOpen for virtual document: {}", e).into(),
                data: None,
            })?;

        // Build JSON params for LSP request
        let request_params = serde_json::json!({
            "textDocument": {
                "uri": virtual_uri_str
            },
            "position": {
                "line": params.text_document_position.position.line,
                "character": params.text_document_position.position.character
            },
            "context": params.context
        });

        // Send completion request with superseding during init window
        let response = connection
            .send_incremental_request("textDocument/completion", request_params, IncrementalType::Completion)
            .await
            .map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("Completion request failed: {}", e).into(),
                data: None,
            })?;

        // Deserialize response into CompletionResponse
        // LSP spec allows null, CompletionList, or CompletionItem[]
        if response.is_null() {
            return Ok(None);
        }

        let completion_response: CompletionResponse =
            serde_json::from_value(response).map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::ParseError,
                message: format!("Failed to parse completion response: {}", e).into(),
                data: None,
            })?;

        Ok(Some(completion_response))
    }

    /// Handles textDocument/hover request
    ///
    /// # Arguments
    /// * `params` - Hover parameters including virtual document URI and translated position
    /// * `content` - Virtual document content to send via didOpen on first access
    ///
    /// # Returns
    /// Hover response from language server, or None if no connection
    pub(crate) async fn hover(
        &self,
        params: HoverParams,
        content: String,
    ) -> Result<Option<Hover>> {
        // Extract language from virtual URI
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(language) = Self::extract_language_from_uri(uri) else {
            // Not a virtual URI - return None
            return Ok(None);
        };

        // Get or spawn connection for this language
        let connection = self.get_or_spawn_connection(&language).await.map_err(|e| {
            tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("Failed to get language server for {}: {}", language, e).into(),
                data: None,
            }
        })?;

        // Wait for connection to be initialized (with 5s timeout)
        connection
            .wait_for_initialized(std::time::Duration::from_secs(5))
            .await
            .map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: e.into(),
                data: None,
            })?;

        // Send didOpen with virtual document content on first access
        let virtual_uri_str = uri.to_string();
        connection
            .check_and_send_did_open(&virtual_uri_str, &language, &content)
            .await
            .map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("Failed to send didOpen for virtual document: {}", e).into(),
                data: None,
            })?;

        // Build JSON params for LSP request
        let request_params = serde_json::json!({
            "textDocument": {
                "uri": virtual_uri_str
            },
            "position": {
                "line": params.text_document_position_params.position.line,
                "character": params.text_document_position_params.position.character
            }
        });

        // Send hover request with superseding during init window
        let response = connection
            .send_incremental_request("textDocument/hover", request_params, IncrementalType::Hover)
            .await
            .map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                message: format!("Hover request failed: {}", e).into(),
                data: None,
            })?;

        // Deserialize response into Hover
        // LSP spec allows null or Hover object
        if response.is_null() {
            return Ok(None);
        }

        let hover: Hover =
            serde_json::from_value(response).map_err(|e| tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::ParseError,
                message: format!("Failed to parse hover response: {}", e).into(),
                data: None,
            })?;

        Ok(Some(hover))
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

    #[test]
    fn test_extract_language_from_virtual_uri() {
        let uri: Url = "file:///virtual/lua/abc123.lua".parse().unwrap();
        let language = LanguageServerPool::extract_language_from_uri(&uri);
        assert_eq!(language, Some("lua".to_string()));
    }

    #[test]
    fn test_extract_language_from_virtual_uri_with_different_language() {
        let uri: Url = "file:///virtual/python/xyz789.py".parse().unwrap();
        let language = LanguageServerPool::extract_language_from_uri(&uri);
        assert_eq!(language, Some("python".to_string()));
    }

    #[test]
    fn test_extract_language_from_non_virtual_uri_returns_none() {
        let uri: Url = "file:///real/document.lua".parse().unwrap();
        let language = LanguageServerPool::extract_language_from_uri(&uri);
        assert_eq!(language, None);
    }

    #[test]
    fn test_extract_language_from_malformed_virtual_uri_returns_none() {
        let uri: Url = "file:///virtual/".parse().unwrap();
        let language = LanguageServerPool::extract_language_from_uri(&uri);
        assert_eq!(language, None);
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
    async fn test_get_or_spawn_connection_returns_immediately_without_blocking_on_init() {
        // RED: Test that get_or_spawn_connection returns immediately
        // without blocking on initialize()
        // This test requires lua-language-server in PATH
        let check = tokio::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .await;

        if check.is_err() {
            eprintln!("SKIP: lua-language-server not found in PATH");
            return;
        }

        let pool = LanguageServerPool::new();

        // Measure time to spawn connection
        let start = std::time::Instant::now();
        let result = pool.get_or_spawn_connection("lua").await;
        let elapsed = start.elapsed();

        assert!(
            result.is_ok(),
            "Should spawn connection successfully: {:?}",
            result.err()
        );

        // Should return quickly (< 100ms) since init happens in background
        // Initialize itself typically takes 100-500ms, so this proves we're not blocking
        assert!(
            elapsed < std::time::Duration::from_millis(100),
            "get_or_spawn_connection should return quickly (< 100ms), took {:?}",
            elapsed
        );

        // Connection should not be initialized immediately
        let connection = result.unwrap();
        assert!(
            !connection.is_initialized(),
            "Connection should not be initialized immediately after spawn"
        );
    }

    #[tokio::test]
    async fn test_pool_completion_returns_ok_none_for_non_virtual_uri() {
        // Test that completion returns None for non-virtual URIs (not in injection regions)
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

        let result = pool.completion(params, String::new()).await;
        assert!(
            result.is_ok(),
            "Completion should succeed: {:?}",
            result.err()
        );
        assert!(
            result.unwrap().is_none(),
            "Non-virtual URI should return None"
        );
    }

    #[tokio::test]
    async fn test_pool_hover_returns_ok_none_for_non_virtual_uri() {
        // Test that hover returns None for non-virtual URIs (not in injection regions)
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

        let result = pool.hover(params, String::new()).await;
        assert!(result.is_ok(), "Hover should succeed: {:?}", result.err());
        assert!(
            result.unwrap().is_none(),
            "Non-virtual URI should return None"
        );
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
