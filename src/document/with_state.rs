use super::with_layers::ParsedDocument;
use tower_lsp::lsp_types::SemanticTokens;

/// A document with parsing and LSP state information
pub struct StatefulDocument {
    parsed_document: ParsedDocument,
    last_semantic_tokens: Option<SemanticTokens>,
}

impl StatefulDocument {
    /// Create a new stateful document
    pub fn new(text: String) -> Self {
        Self {
            parsed_document: ParsedDocument::new(text),
            last_semantic_tokens: None,
        }
    }

    /// Create from parsed document
    pub fn from_parsed(parsed_document: ParsedDocument) -> Self {
        Self {
            parsed_document,
            last_semantic_tokens: None,
        }
    }

    /// Create with root layer
    pub fn with_root_layer(text: String, language_id: String, tree: tree_sitter::Tree) -> Self {
        Self {
            parsed_document: ParsedDocument::with_root_layer(text, language_id, tree),
            last_semantic_tokens: None,
        }
    }

    /// Get the text content
    pub fn text(&self) -> &str {
        self.parsed_document.text()
    }

    /// Get the parsed document
    pub fn parsed_document(&self) -> &ParsedDocument {
        &self.parsed_document
    }

    /// Get mutable access to the parsed document
    pub fn parsed_document_mut(&mut self) -> &mut ParsedDocument {
        &mut self.parsed_document
    }

    /// Get the layer manager
    pub fn layers(&self) -> &crate::language::LayerManager {
        self.parsed_document.layers()
    }

    /// Get mutable access to the layer manager
    pub fn layers_mut(&mut self) -> &mut crate::language::LayerManager {
        self.parsed_document.layers_mut()
    }

    /// Get the last semantic tokens
    pub fn last_semantic_tokens(&self) -> Option<&SemanticTokens> {
        self.last_semantic_tokens.as_ref()
    }

    /// Set the last semantic tokens
    pub fn set_last_semantic_tokens(&mut self, tokens: Option<SemanticTokens>) {
        self.last_semantic_tokens = tokens;
    }

    /// Update text and clear state
    pub fn update_text(&mut self, text: String) {
        self.parsed_document.update_text(text);
        self.last_semantic_tokens = None;
    }

    /// Get document version
    pub fn version(&self) -> Option<i32> {
        self.parsed_document.version()
    }

    /// Set document version
    pub fn set_version(&mut self, version: Option<i32>) {
        self.parsed_document.set_version(version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stateful_document_creation() {
        let doc = StatefulDocument::new("test content".to_string());
        assert_eq!(doc.text(), "test content");
        assert!(doc.last_semantic_tokens().is_none());
    }

    #[test]
    fn test_semantic_tokens_management() {
        let mut doc = StatefulDocument::new("test".to_string());

        let tokens = SemanticTokens {
            result_id: Some("test-id".to_string()),
            data: vec![],
        };

        doc.set_last_semantic_tokens(Some(tokens.clone()));
        assert!(doc.last_semantic_tokens().is_some());

        doc.update_text("updated".to_string());
        assert!(doc.last_semantic_tokens().is_none());
    }
}
