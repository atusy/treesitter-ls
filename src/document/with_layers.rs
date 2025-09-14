use super::text::TextDocument;
use crate::language::LayerManager;

/// A parsed document with language layers
pub struct ParsedDocument {
    document: TextDocument,
    layers: LayerManager,
}

impl ParsedDocument {
    /// Create a new parsed document
    pub fn new(text: String) -> Self {
        Self {
            document: TextDocument::new(text),
            layers: LayerManager::new(),
        }
    }

    /// Create from existing text document
    pub fn from_text_document(document: TextDocument) -> Self {
        Self {
            document,
            layers: LayerManager::new(),
        }
    }

    /// Create with initial layer
    pub fn with_root_layer(text: String, language_id: String, tree: tree_sitter::Tree) -> Self {
        Self {
            document: TextDocument::new(text),
            layers: LayerManager::with_root(language_id, tree),
        }
    }

    /// Get the text content
    pub fn text(&self) -> &str {
        self.document.text()
    }

    /// Get mutable access to the text document
    pub fn text_document_mut(&mut self) -> &mut TextDocument {
        &mut self.document
    }

    /// Get the text document
    pub fn text_document(&self) -> &TextDocument {
        &self.document
    }

    /// Get the layer manager
    pub fn layers(&self) -> &LayerManager {
        &self.layers
    }

    /// Get mutable access to the layer manager
    pub fn layers_mut(&mut self) -> &mut LayerManager {
        &mut self.layers
    }

    /// Update text and clear layers (for document changes)
    pub fn update_text(&mut self, text: String) {
        self.document.set_text(text);
        // Note: Layers need to be rebuilt after text change
        self.layers = LayerManager::new();
    }

    /// Get document version
    pub fn version(&self) -> Option<i32> {
        self.document.version()
    }

    /// Set document version
    pub fn set_version(&mut self, version: Option<i32>) {
        self.document.set_version(version);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsed_document_creation() {
        let doc = ParsedDocument::new("test content".to_string());
        assert_eq!(doc.text(), "test content");
        assert!(doc.layers().root_layer().is_none());
    }

    #[test]
    fn test_parsed_document_with_layer() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("fn main() {}", None).unwrap();

        let doc =
            ParsedDocument::with_root_layer("fn main() {}".to_string(), "rust".to_string(), tree);

        assert_eq!(doc.text(), "fn main() {}");
        assert!(doc.layers().root_layer().is_some());
    }

    #[test]
    fn test_update_text() {
        let mut doc = ParsedDocument::new("initial".to_string());
        doc.update_text("updated".to_string());

        assert_eq!(doc.text(), "updated");
        // Layers should be cleared after text update
        assert!(doc.layers().root_layer().is_none());
    }
}
