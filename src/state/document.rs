use dashmap::DashMap;
use tower_lsp::lsp_types::{Url, SemanticTokens};
use tree_sitter::Tree;

// A document entry in our store.
pub struct Document {
    pub text: String,
    pub tree: Option<Tree>,
    pub last_semantic_tokens: Option<SemanticTokens>,
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

    pub fn insert(&self, uri: Url, text: String, tree: Option<Tree>) {
        self.documents.insert(uri, Document { 
            text, 
            tree,
            last_semantic_tokens: None,
        });
    }

    pub fn get(&self, uri: &Url) -> Option<dashmap::mapref::one::Ref<'_, Url, Document>> {
        self.documents.get(uri)
    }

    pub fn update_document(&self, uri: Url, text: String) {
        self.documents.insert(uri, Document { 
            text, 
            tree: None,
            last_semantic_tokens: None,
        });
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

    #[test]
    fn test_add_and_get_document() {
        let store = DocumentStore::new();
        let uri = Url::parse("file:///test.txt").unwrap();
        let text = "hello world";

        store.update_document(uri.clone(), text.to_string());

        let retrieved_text = store.get_document_text(&uri);

        assert_eq!(retrieved_text, Some(text.to_string()));
    }
}
