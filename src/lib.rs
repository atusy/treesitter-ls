use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::Parser;

pub mod config;
pub mod handlers;
mod analysis;
pub mod state;
pub mod utils;

// Re-export config types for backward compatibility
pub use config::{HighlightItem, HighlightSource, LanguageConfig, TreeSitterSettings};

use handlers::{
    DefinitionResolver as PrivateDefinitionResolver, handle_goto_definition,
    handle_semantic_tokens_full,
};

// Re-export for tests
pub use handlers::{
    ContextType, DefinitionCandidate, DefinitionResolver, LEGEND_TYPES, ReferenceContext,
};
use state::{DocumentStore, LanguageService};
use utils::position_to_byte_offset;

pub struct TreeSitterLs {
    client: Client,
    language_service: LanguageService,
    document_store: DocumentStore,
    definition_resolver: std::sync::Mutex<DefinitionResolver>,
}

impl std::fmt::Debug for TreeSitterLs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TreeSitterLs")
            .field("client", &self.client)
            .field("document_store", &"DocumentStore")
            .finish_non_exhaustive()
    }
}

impl TreeSitterLs {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            language_service: LanguageService::new(),
            document_store: DocumentStore::new(),
            definition_resolver: std::sync::Mutex::new(PrivateDefinitionResolver::new()),
        }
    }

    async fn parse_document(&self, uri: Url, text: String) {
        // Detect file extension
        let extension = uri.path().split('.').next_back().unwrap_or("");

        // Find the language for this file extension
        let filetype_map = self.language_service.filetype_map.lock().unwrap();
        let language_name = filetype_map.get(extension);

        if let Some(language_name) = language_name {
            let languages = self.language_service.languages.lock().unwrap();
            if let Some(language) = languages.get(language_name) {
                let mut parser = Parser::new();
                if parser.set_language(language).is_ok() {
                    if let Some(tree) = parser.parse(&text, None) {
                        self.document_store.insert(uri, text, Some(tree));
                        return;
                    }
                }
            }
        }

        self.document_store.insert(uri, text, None);
    }

    fn get_language_for_document(&self, uri: &Url) -> Option<String> {
        self.language_service.get_language_for_document(uri)
    }

    async fn load_settings(&self, settings: TreeSitterSettings) {
        self.language_service.load_settings(settings, &self.client).await;
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for TreeSitterLs {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // Debug: Log initialization
        self.client
            .log_message(MessageType::INFO, "Received initialization request")
            .await;

        // Parse configuration from initialization_options
        if let Some(options) = params.initialization_options {
            if let Ok(settings) = serde_json::from_value::<TreeSitterSettings>(options) {
                self.client
                    .log_message(MessageType::INFO, "Parsed as TreeSitterSettings")
                    .await;
                self.load_settings(settings).await;
            } else {
                self.client
                    .log_message(MessageType::ERROR, "Failed to parse initialization options")
                    .await;
            }
        }

        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "treesitter-ls".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: LEGEND_TYPES.to_vec(),
                                token_modifiers: vec![],
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                definition_provider: Some(OneOf::Left(true)),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server is ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.parse_document(params.text_document.uri, params.text_document.text)
            .await;
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.parse_document(
            params.text_document.uri,
            params.content_changes.remove(0).text,
        )
        .await;
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        if let Ok(settings) = serde_json::from_value::<TreeSitterSettings>(params.settings) {
            self.load_settings(settings).await;
            self.client
                .log_message(MessageType::INFO, "Configuration updated!")
                .await;
        }
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(None);
        };
        let queries = self.language_service.queries.lock().unwrap();
        let Some(query) = queries.get(&language_name) else {
            return Ok(None);
        };

        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };
        let text = &doc.text;
        let Some(tree) = doc.tree.as_ref() else {
            return Ok(None);
        };

        // Delegate to handler
        Ok(handle_semantic_tokens_full(text, tree, query))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Get language for document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(None);
        };

        // Get locals query
        let locals_queries = self.language_service.locals_queries.lock().unwrap();
        let Some(locals_query) = locals_queries.get(&language_name) else {
            return Ok(None);
        };

        // Get document and tree
        let Some(doc) = self.document_store.get(&uri) else {
            return Ok(None);
        };
        let text = &doc.text;
        let Some(tree) = doc.tree.as_ref() else {
            return Ok(None);
        };

        // Convert position to byte offset
        let byte_offset = position_to_byte_offset(text, position);

        // Get resolver
        let resolver = self.definition_resolver.lock().unwrap();

        // Delegate to handler
        Ok(handle_goto_definition(
            &resolver,
            text,
            tree,
            locals_query,
            byte_offset,
            &uri,
        ))
    }
}

#[cfg(test)]
mod simple_tests;
