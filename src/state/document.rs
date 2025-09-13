use crate::state::language_layer::LanguageLayer;
use crate::state::parser_pool::DocumentParserPool;
use dashmap::DashMap;
use tower_lsp::lsp_types::{SemanticTokens, Url};
use tree_sitter::{InputEdit, Tree};

// A document entry in our store.
pub struct Document {
    pub text: String,
    pub last_semantic_tokens: Option<SemanticTokens>,
    pub root_layer: Option<LanguageLayer>, // Main language layer
    pub injection_layers: Vec<LanguageLayer>, // Injection language layers
    pub parser_pool: Option<DocumentParserPool>, // Document-specific parser pool
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
        self.documents.insert(
            uri,
            Document {
                text,
                last_semantic_tokens: None,
                root_layer,
                injection_layers: Vec::new(),
                parser_pool: None, // Will be initialized when needed
            },
        );
    }

    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, Document>> {
        self.documents.get(uri)
    }

    pub fn update_document(&self, uri: Url, text: String) {
        // Preserve root layer info from existing document if available
        let root_layer = self.documents.get(&uri).and_then(|doc| {
            doc.root_layer.as_ref().map(|layer| {
                // Preserve the language but we'll update the tree later
                LanguageLayer::root(layer.language_id.clone(), layer.tree.clone())
            })
        });

        self.documents.insert(
            uri,
            Document {
                text,
                last_semantic_tokens: None,
                root_layer,
                injection_layers: Vec::new(),
                parser_pool: None, // Will be initialized when needed
            },
        );
    }

    /// Get the existing tree and apply edits for incremental parsing
    /// Returns the edited tree without updating the document store
    pub fn get_edited_tree(&self, uri: &Url, edits: &[InputEdit]) -> Option<Tree> {
        self.documents.get(uri).and_then(|doc| {
            doc.root_layer.as_ref().map(|layer| {
                let mut tree = layer.tree.clone();
                // Apply all edits to the tree
                for edit in edits {
                    tree.edit(edit);
                }
                tree
            })
        })
    }

    /// Update document with a new tree after incremental parsing
    pub fn update_document_with_tree(&self, uri: Url, text: String, tree: Tree) {
        // Get the language_id from existing document
        let language_id = self.documents.get(&uri).and_then(|doc| {
            doc.root_layer
                .as_ref()
                .map(|layer| layer.language_id.clone())
        });

        if let Some(language_id) = language_id {
            let root_layer = Some(LanguageLayer::root(language_id, tree));
            self.documents.insert(
                uri,
                Document {
                    text,
                    last_semantic_tokens: None,
                    root_layer,
                    injection_layers: Vec::new(),
                    parser_pool: None, // Will be initialized when needed
                },
            );
        } else {
            // If no language_id, just update the text
            self.update_document(uri, text);
        }
    }

    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.last_semantic_tokens = Some(tokens);
        }
    }

    pub fn get_document_text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|doc| doc.text.clone())
    }

    pub fn init_parser_pool(&self, uri: &Url, pool: DocumentParserPool) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.parser_pool = Some(pool);
        }
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
            Box::new(crate::treesitter::InjectionPositionMapper::new(
                &self.text,
                &self.injection_layers,
            ))
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
            root.tree = tree;
        }
    }

    /// Get the language_id from root layer
    pub fn get_language_id(&self) -> Option<&String> {
        self.root_layer.as_ref().map(|layer| &layer.language_id)
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
    fn test_document_layer_preservation() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();

        // Create a simple tree for testing (using Rust language)
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        // First insert with a tree
        let text1 = "fn main() {}";
        let tree1 = parser.parse(text1, None).unwrap();
        let root_layer1 = Some(LanguageLayer::root("rust".to_string(), tree1.clone()));
        store.insert(uri.clone(), text1.to_string(), root_layer1);

        // Verify the document has root layer
        {
            let doc = store.get(&uri).unwrap();
            assert!(doc.root_layer.is_some());
        }

        // Second insert should preserve the previous tree as old_tree
        let text2 = "fn main() { println!(\"hello\"); }";
        let tree2 = parser.parse(text2, Some(&tree1)).unwrap();
        let root_layer2 = Some(LanguageLayer::root("rust".to_string(), tree2));
        store.insert(uri.clone(), text2.to_string(), root_layer2);

        // Verify the root layer is updated
        {
            let doc = store.get(&uri).unwrap();
            assert!(doc.root_layer.is_some());
            assert_eq!(doc.text, text2);
        }
    }

    #[test]
    fn test_update_document_preserves_language() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();

        // Create a simple tree for testing
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        // First insert with a tree
        let text1 = "fn main() {}";
        let tree1 = parser.parse(text1, None).unwrap();
        let root_layer1 = Some(LanguageLayer::root("rust".to_string(), tree1.clone()));
        store.insert(uri.clone(), text1.to_string(), root_layer1);

        // Update document should preserve the language
        let text2 = "fn main() { println!(\"hello\"); }";
        store.update_document(uri.clone(), text2.to_string());

        // Verify the language is preserved
        {
            let doc = store.get(&uri).unwrap();
            assert!(doc.root_layer.is_some());
            assert_eq!(doc.text, text2);
            assert_eq!(doc.get_language_id(), Some(&"rust".to_string()));
        }
    }
}
