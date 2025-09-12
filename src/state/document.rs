use dashmap::DashMap;
use tower_lsp::lsp_types::{SemanticTokens, Url};
use tree_sitter::Tree;

// A document entry in our store.
pub struct Document {
    pub text: String,
    pub tree: Option<Tree>,
    pub old_tree: Option<Tree>, // Previous tree for incremental parsing
    pub last_semantic_tokens: Option<SemanticTokens>,
    pub language_id: Option<String>,
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

        self.documents.insert(
            uri,
            Document {
                text,
                tree,
                old_tree,
                last_semantic_tokens: None,
                language_id,
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

        self.documents.insert(
            uri,
            Document {
                text,
                tree: None,
                old_tree,
                last_semantic_tokens: None,
                language_id,
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
