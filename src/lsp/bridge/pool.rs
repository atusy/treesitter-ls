//! LanguageServerPool for managing multiple language server connections

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use super::connection::BridgeConnection;

/// Pool for managing language server connections
///
/// This is a fakeit implementation that returns Ok(None) for all LSP requests
/// to validate the API structure before adding async complexity.
pub(crate) struct LanguageServerPool {
    /// Optional connection to a language server
    /// None for fakeit implementation, Some for real implementation
    _connection: Option<BridgeConnection>,
}

impl LanguageServerPool {
    /// Creates a new LanguageServerPool
    ///
    /// This is a fakeit implementation with no real connection.
    pub(crate) fn new() -> Self {
        Self { _connection: None }
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
