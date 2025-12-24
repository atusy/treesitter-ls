use crate::document::Document;
use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use std::ops::Deref;
use tower_lsp::lsp_types::SemanticTokens;
use tree_sitter::{InputEdit, Tree};
use url::Url;

// The central store for all document-related information.
pub struct DocumentStore {
    documents: DashMap<Url, Document>,
}

pub struct DocumentHandle<'a> {
    inner: Ref<'a, Url, Document>,
}

impl<'a> DocumentHandle<'a> {
    fn new(inner: Ref<'a, Url, Document>) -> Self {
        Self { inner }
    }
}

impl<'a> Deref for DocumentHandle<'a> {
    type Target = Document;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
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

    pub fn insert(&self, uri: Url, text: String, language_id: Option<String>, tree: Option<Tree>) {
        let document = if let (Some(lang), Some(t)) = (language_id, tree) {
            Document::with_tree(text, lang, t)
        } else {
            Document::new(text)
        };

        self.documents.insert(uri, document);
    }

    pub fn get(&self, uri: &Url) -> Option<DocumentHandle<'_>> {
        self.documents.get(uri).map(DocumentHandle::new)
    }

    pub fn update_document(&self, uri: Url, text: String, new_tree: Option<Tree>) {
        // Preserve language_id and semantic tokens from existing document if available
        let (language_id, existing_tokens) = self
            .documents
            .get(&uri)
            .map(|doc| {
                (
                    doc.language_id().map(String::from),
                    doc.last_semantic_tokens().cloned(),
                )
            })
            .unwrap_or((None, None));

        let mut new_doc = match (language_id, new_tree) {
            (Some(lang), Some(tree)) => Document::with_tree(text, lang, tree),
            (Some(lang), None) => {
                // Preserve existing tree if no new tree provided
                let existing_tree = self.documents.get(&uri).and_then(|doc| doc.tree().cloned());
                if let Some(tree) = existing_tree {
                    Document::with_tree(text, lang, tree)
                } else {
                    Document::new(text)
                }
            }
            _ => Document::new(text),
        };

        // Preserve semantic tokens for delta calculation
        if existing_tokens.is_some() {
            new_doc.set_last_semantic_tokens(existing_tokens);
        }

        self.documents.insert(uri, new_doc);
    }

    /// Get the existing tree and apply edits for incremental parsing
    /// Returns the edited tree without updating the document store
    pub fn get_edited_tree(&self, uri: &Url, edits: &[InputEdit]) -> Option<Tree> {
        self.documents.get(uri).and_then(|doc| {
            doc.tree().map(|tree| {
                let mut tree = tree.clone();
                for edit in edits {
                    tree.edit(edit);
                }
                tree
            })
        })
    }

    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        if let Some(mut doc) = self.documents.get_mut(uri) {
            doc.set_last_semantic_tokens(Some(tokens));
        }
    }

    pub fn get_document_text(&self, uri: &Url) -> Option<String> {
        self.documents.get(uri).map(|doc| doc.text().to_string())
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

        store.insert(uri.clone(), text.clone(), None, None);
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

        store.insert(uri.clone(), text1, Some("rust".to_string()), Some(tree));

        // Update text
        store.update_document(uri.clone(), text2.clone(), None);

        // Language info should be preserved
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), text2);
        assert_eq!(doc.language_id(), Some("rust"));
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

        store.insert(
            uri.clone(),
            text.clone(),
            Some("rust".to_string()),
            Some(tree.clone()),
        );

        // Update with new tree
        let new_text = "let x = 2;".to_string();
        let new_tree = parser.parse(&new_text, Some(&tree)).unwrap();
        store.update_document(uri.clone(), new_text.clone(), Some(new_tree));

        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), new_text);
        assert!(doc.tree().is_some());
    }

    #[test]
    fn test_update_document_preserves_semantic_tokens() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.rs").unwrap();
        let text = "let x = 1;".to_string();

        // Create document with tree
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(&text, None).unwrap();

        store.insert(
            uri.clone(),
            text.clone(),
            Some("rust".to_string()),
            Some(tree.clone()),
        );

        // Set semantic tokens
        let tokens = SemanticTokens {
            result_id: Some("v0".to_string()),
            data: vec![],
        };
        store.update_semantic_tokens(&uri, tokens.clone());

        // Verify tokens are stored
        let doc = store.get(&uri).unwrap();
        assert!(doc.last_semantic_tokens().is_some());
        assert_eq!(
            doc.last_semantic_tokens().unwrap().result_id,
            Some("v0".to_string())
        );
        drop(doc);

        // Update document with new text and tree
        let new_text = "let x = 2;".to_string();
        let new_tree = parser.parse(&new_text, Some(&tree)).unwrap();
        store.update_document(uri.clone(), new_text.clone(), Some(new_tree));

        // Semantic tokens should be preserved after document update
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), new_text);
        assert!(doc.tree().is_some());
        assert!(
            doc.last_semantic_tokens().is_some(),
            "Semantic tokens should be preserved after document update"
        );
        assert_eq!(
            doc.last_semantic_tokens().unwrap().result_id,
            Some("v0".to_string()),
            "Semantic tokens result_id should be preserved"
        );
    }
}
