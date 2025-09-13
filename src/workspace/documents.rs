use crate::document::StatefulDocument;
use crate::injection::LanguageLayer;
use crate::syntax::parser_pool::DocumentParserPool;
use dashmap::DashMap;
use tower_lsp::lsp_types::{SemanticTokens, Url};
use tree_sitter::{InputEdit, Parser, Tree};

// Type alias for backward compatibility during migration
pub type Document = StatefulDocument;

// Extension methods for Document compatibility
impl Document {
    /// Get the language ID from root layer
    pub fn get_language_id(&self) -> Option<&str> {
        self.layers().get_language_id().map(|s| s.as_str())
    }

    /// Get the root layer
    pub fn root_layer(&self) -> Option<&crate::injection::LanguageLayer> {
        self.layers().root_layer()
    }

    /// Get injection layers
    pub fn injection_layers(&self) -> &[crate::injection::LanguageLayer] {
        self.layers().injection_layers()
    }

    /// Add an injection layer
    pub fn add_injection_layer(&mut self, layer: crate::injection::LanguageLayer) {
        self.layers_mut().add_injection_layer(layer);
    }

    /// Update the root tree
    pub fn update_root_tree(&mut self, tree: Tree) {
        self.layers_mut().update_root_tree(tree);
    }

    /// Acquire a parser for the specified language
    pub fn acquire_parser(&mut self, language_id: &str) -> Option<Parser> {
        self.layers_mut().acquire_parser(language_id)
    }

    /// Release a parser back to the pool
    pub fn release_parser(&mut self, language_id: String, parser: Parser) {
        self.layers_mut().release_parser(language_id, parser);
    }

    /// Set parser pool
    pub fn init_parser_pool(&mut self, pool: DocumentParserPool) {
        self.layers_mut().set_parser_pool(pool);
    }

    /// Get all layers
    pub fn get_all_layers(&self) -> impl Iterator<Item = &crate::injection::LanguageLayer> {
        self.layers().all_layers()
    }

    /// Get layer at offset
    pub fn get_layer_at_position(
        &self,
        byte_offset: usize,
    ) -> Option<&crate::injection::LanguageLayer> {
        self.layers().get_layer_at_offset(byte_offset)
    }

    /// Create position mapper (renamed from position_mapper for clarity)
    pub fn position_mapper<'a>(&'a self) -> Box<dyn crate::injection::PositionMapper + 'a> {
        if self.layers().injection_layers().is_empty() {
            Box::new(crate::text::position::SimplePositionMapper::new(
                self.text(),
            ))
        } else {
            Box::new(crate::injection::InjectionPositionMapper::new(
                self.text(),
                self.layers().injection_layers(),
            ))
        }
    }
}

// The central store for all document-related information.
pub struct DocumentStore {
    documents: DashMap<Url, Document>,
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }
}

impl DocumentStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, uri: Url, text: String, root_layer: Option<LanguageLayer>) {
        let document = if let Some(layer) = root_layer {
            Document::with_root_layer(text, layer.language_id.clone(), layer.tree.clone())
        } else {
            Document::new(text)
        };

        self.documents.insert(uri, document);
    }

    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, Document>> {
        self.documents.get(uri)
    }

    pub fn update_document(&self, uri: Url, text: String) {
        // Preserve root layer info from existing document if available
        let root_layer = self
            .documents
            .get(&uri)
            .and_then(|doc| doc.layers().root_layer().cloned());

        if let Some(root) = root_layer {
            let new_doc =
                Document::with_root_layer(text, root.language_id.clone(), root.tree.clone());
            self.documents.insert(uri, new_doc);
        } else {
            self.documents.insert(uri, Document::new(text));
        }
    }

    /// Get the existing tree and apply edits for incremental parsing
    /// Returns the edited tree without updating the document store
    pub fn get_edited_tree(&self, uri: &Url, edits: &[InputEdit]) -> Option<Tree> {
        self.documents
            .get(uri)
            .and_then(|doc| doc.layers().apply_edits_to_root(edits))
    }

    /// Update document with a new tree after incremental parsing
    pub fn update_document_with_tree(&self, uri: Url, text: String, tree: Tree) {
        // Get the language_id from existing document
        let language_id = self
            .documents
            .get(&uri)
            .and_then(|doc| doc.layers().get_language_id().cloned());

        if let Some(language_id) = language_id {
            let new_doc = Document::with_root_layer(text, language_id, tree);
            self.documents.insert(uri, new_doc);
        } else {
            // If no language_id, just update the text
            self.update_document(uri, text);
        }
    }

    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.set_last_semantic_tokens(Some(tokens));
        }
    }

    pub fn get_document_text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|doc| doc.text().to_string())
    }

    pub fn init_parser_pool(&self, uri: &Url, pool: DocumentParserPool) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.layers_mut().set_parser_pool(pool);
        }
    }

    pub fn remove(&self, uri: &Url) -> Option<Document> {
        self.documents.remove(uri).map(|(_, doc)| doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_get_document() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.txt").unwrap();
        let text = "hello world".to_string();

        store.insert(uri.clone(), text.clone(), None);
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), &text);
    }

    #[test]
    fn test_update_document_preserves_language() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        let text1 = "fn main() {}".to_string();
        let text2 = "fn main() { println!(\"hello\"); }".to_string();

        // Create a fake tree for testing
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(&text1, None).unwrap();
        let layer = LanguageLayer::root("rust".to_string(), tree);

        store.insert(uri.clone(), text1, Some(layer));

        // Update text
        store.update_document(uri.clone(), text2.clone());

        // Language info should be preserved
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), text2);
        assert_eq!(doc.get_language_id(), Some("rust"));
    }

    #[test]
    fn test_document_layer_preservation() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        let text = "let x = 1;".to_string();

        // Create document with tree
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(&text, None).unwrap();
        let layer = LanguageLayer::root("rust".to_string(), tree.clone());

        store.insert(uri.clone(), text.clone(), Some(layer));

        // Update with new tree
        let new_text = "let x = 2;".to_string();
        let new_tree = parser.parse(&new_text, Some(&tree)).unwrap();
        store.update_document_with_tree(uri.clone(), new_text.clone(), new_tree);

        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), new_text);
        assert!(doc.root_layer().is_some());
    }
}
