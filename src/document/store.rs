use crate::document::{Document, SemanticSnapshot};
use dashmap::DashMap;
use dashmap::mapref::one::Ref;
use std::ops::Deref;
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

    pub fn update_document(&self, uri: Url, text: String) {
        // Preserve language and tree from existing document if available
        let existing = self.documents.get(&uri);
        let language_id = existing.as_ref().and_then(|doc| doc.language_id().map(String::from));
        let tree = existing.and_then(|doc| doc.tree().cloned());

        if let (Some(lang), Some(t)) = (language_id, tree) {
            let new_doc = Document::with_tree(text, lang, t);
            self.documents.insert(uri, new_doc);
        } else {
            self.documents.insert(uri, Document::new(text));
        }
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

    /// Update document with a new tree after incremental parsing
    pub fn update_document_with_tree(&self, uri: Url, text: String, tree: Tree) {
        // Get the language_id from existing document
        let language_id = self
            .documents
            .get(&uri)
            .and_then(|doc| doc.language_id().map(|s| s.to_string()));

        if let Some(language_id) = language_id {
            let new_doc = Document::with_tree(text, language_id, tree);
            self.documents.insert(uri, new_doc);
        } else {
            // If no language_id, just update the text
            self.update_document(uri, text);
        }
    }

    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticSnapshot) {
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
        store.update_document(uri.clone(), text2.clone());

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

        store.insert(uri.clone(), text.clone(), Some("rust".to_string()), Some(tree.clone()));

        // Update with new tree
        let new_text = "let x = 2;".to_string();
        let new_tree = parser.parse(&new_text, Some(&tree)).unwrap();
        store.update_document_with_tree(uri.clone(), new_text.clone(), new_tree);

        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.text(), new_text);
        assert!(doc.tree().is_some());
    }
}
