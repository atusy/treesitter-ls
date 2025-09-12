use crate::state::language_layer::LanguageLayer;
use crate::state::parser_pool::DocumentParserPool;
use dashmap::DashMap;
use tower_lsp::lsp_types::{SemanticTokens, Url};
use tree_sitter::Tree;

// A document entry in our store.
pub struct Document {
    pub text: String,
    pub tree: Option<Tree>,  // Deprecated: use root_layer instead
    pub old_tree: Option<Tree>, // Previous tree for incremental parsing
    pub last_semantic_tokens: Option<SemanticTokens>,
    pub language_id: Option<String>,  // Deprecated: use root_layer instead
    pub root_layer: Option<LanguageLayer>,  // Main language layer
    pub injection_layers: Vec<LanguageLayer>,  // Injection language layers
    pub parser_pool: Option<DocumentParserPool>,  // Document-specific parser pool
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

    pub fn insert(&self, uri: Url, text: String, tree: Option<Tree>, language_id: Option<String>) {
        // Preserve the old tree for incremental parsing
        let old_tree = self.documents.get(&uri).and_then(|doc| doc.tree.clone());
        
        // Create root layer if tree and language_id are provided
        let root_layer = match (&tree, &language_id) {
            (Some(t), Some(lang)) => Some(LanguageLayer::root(lang.clone(), t.clone())),
            _ => None,
        };

        self.documents.insert(
            uri,
            Document {
                text,
                tree,
                old_tree,
                last_semantic_tokens: None,
                language_id,
                root_layer,
                injection_layers: Vec::new(),
                parser_pool: None,  // Will be initialized when needed
            },
        );
    }

    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, Document>> {
        self.documents.get(uri)
    }

    pub fn update_document(&self, uri: Url, text: String) {
        // Preserve language_id and tree from existing document if available
        let (language_id, old_tree) = self
            .documents
            .get(&uri)
            .map(|doc| (doc.language_id.clone(), doc.tree.clone()))
            .unwrap_or((None, None));

        // Create root layer if language_id is available
        let root_layer = language_id.as_ref().and_then(|lang| {
            // For update_document, we don't have a tree yet, it will be set later
            // This is a temporary state
            old_tree.as_ref().map(|t| LanguageLayer::root(lang.clone(), t.clone()))
        });
        
        self.documents.insert(
            uri,
            Document {
                text,
                tree: None,
                old_tree,
                last_semantic_tokens: None,
                language_id,
                root_layer,
                injection_layers: Vec::new(),
                parser_pool: None,  // Will be initialized when needed
            },
        );
    }

    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.last_semantic_tokens = Some(tokens);
        }
    }

    pub fn get_document_text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|doc| doc.text.clone())
    }
}

// Backward compatibility methods for Document
impl Document {
    /// Initialize parser pool for this document
    pub fn init_parser_pool(&mut self, pool: DocumentParserPool) {
        self.parser_pool = Some(pool);
    }
    
    /// Get a position mapper for this document
    /// Returns SimplePositionMapper for simple documents,
    /// InjectionPositionMapper when injection layers are present
    pub fn position_mapper(&self) -> Box<dyn crate::treesitter::PositionMapper + '_> {
        if self.injection_layers.is_empty() {
            Box::new(crate::treesitter::SimplePositionMapper::new(&self.text))
        } else {
            // For now, use SimplePositionMapper even with injections
            // TODO: Fix lifetime issues with InjectionPositionMapper
            Box::new(crate::treesitter::SimplePositionMapper::new(&self.text))
        }
    }
    
    /// Get the primary language layer at a specific byte offset
    pub fn get_layer_at_position(&self, byte_offset: usize) -> Option<&LanguageLayer> {
        // Check injection layers first (they have higher priority)
        for layer in &self.injection_layers {
            if layer.contains_offset(byte_offset) {
                return Some(layer);
            }
        }
        
        // Fall back to root layer
        self.root_layer.as_ref()
    }
    
    /// Get all language layers (root + injections)
    pub fn get_all_layers(&self) -> impl Iterator<Item = &LanguageLayer> {
        self.root_layer.iter().chain(self.injection_layers.iter())
    }
    
    /// Add an injection layer
    pub fn add_injection_layer(&mut self, layer: LanguageLayer) {
        self.injection_layers.push(layer);
    }
    
    /// Update the root layer's tree (used after re-parsing)
    pub fn update_root_tree(&mut self, tree: Tree) {
        if let Some(root) = &mut self.root_layer {
            root.tree = tree.clone();
        }
        // Also update the deprecated tree field for backward compatibility
        self.tree = Some(tree);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Url;
    use tree_sitter::Parser;

    #[test]
    fn test_add_and_get_document() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.txt").unwrap();
        let text = "hello world";

        store.update_document(uri.clone(), text.to_string());

        let retrieved_text = store.get_document_text(&uri);

        assert_eq!(retrieved_text, Some(text.to_string()));
    }

    #[test]
    fn test_incremental_parsing_preserves_old_tree() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();

        // Create a simple tree for testing (using Rust language)
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        // First insert with a tree
        let text1 = "fn main() {}";
        let tree1 = parser.parse(text1, None).unwrap();
        store.insert(
            uri.clone(),
            text1.to_string(),
            Some(tree1.clone()),
            Some("rust".to_string()),
        );

        // Verify the document has no old_tree initially
        {
            let doc = store.get(&uri).unwrap();
            assert!(doc.tree.is_some());
            assert!(doc.old_tree.is_none());
        }

        // Second insert should preserve the previous tree as old_tree
        let text2 = "fn main() { println!(\"hello\"); }";
        let tree2 = parser.parse(text2, Some(&tree1)).unwrap();
        store.insert(
            uri.clone(),
            text2.to_string(),
            Some(tree2),
            Some("rust".to_string()),
        );

        // Verify the old tree is preserved
        {
            let doc = store.get(&uri).unwrap();
            assert!(doc.tree.is_some());
            assert!(doc.old_tree.is_some());
            // The old_tree should be from the first parse
            assert_eq!(
                doc.old_tree.as_ref().unwrap().root_node().kind(),
                tree1.root_node().kind()
            );
        }
    }

    #[test]
    fn test_update_document_preserves_tree_as_old_tree() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();

        // Create a simple tree for testing
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        // First insert with a tree
        let text1 = "fn main() {}";
        let tree1 = parser.parse(text1, None).unwrap();
        store.insert(
            uri.clone(),
            text1.to_string(),
            Some(tree1.clone()),
            Some("rust".to_string()),
        );

        // Update document should preserve the tree as old_tree
        let text2 = "fn main() { println!(\"hello\"); }";
        store.update_document(uri.clone(), text2.to_string());

        // Verify the tree is preserved as old_tree
        {
            let doc = store.get(&uri).unwrap();
            assert!(doc.tree.is_none()); // update_document sets tree to None
            assert!(doc.old_tree.is_some()); // but preserves the previous tree as old_tree
            assert_eq!(
                doc.old_tree.as_ref().unwrap().root_node().kind(),
                tree1.root_node().kind()
            );
            assert_eq!(doc.language_id, Some("rust".to_string()));
        }
    }
}
