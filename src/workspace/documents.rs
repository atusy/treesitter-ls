use crate::document::{Document, DocumentStore, LanguageLayer};
use dashmap::mapref::one::Ref;
use tower_lsp::lsp_types::{SemanticTokens, Url};
use tree_sitter::{InputEdit, Tree};

pub type DocumentRef<'a> = Ref<'a, Url, Document>;

pub struct WorkspaceDocuments {
    store: DocumentStore,
}

impl WorkspaceDocuments {
    pub fn new() -> Self {
        Self {
            store: DocumentStore::new(),
        }
    }

    pub fn insert(&self, uri: Url, text: String, root_layer: Option<LanguageLayer>) {
        self.store.insert(uri, text, root_layer);
    }

    pub fn update_text(&self, uri: Url, text: String) {
        self.store.update_document(uri, text);
    }

    pub fn update_with_tree(&self, uri: Url, text: String, tree: Tree) {
        self.store.update_document_with_tree(uri, text, tree);
    }

    pub fn get(&self, uri: &Url) -> Option<DocumentRef<'_>> {
        self.store.get(uri)
    }

    pub fn get_edited_tree(&self, uri: &Url, edits: &[InputEdit]) -> Option<Tree> {
        self.store.get_edited_tree(uri, edits)
    }

    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        self.store.update_semantic_tokens(uri, tokens);
    }

    pub fn text(&self, uri: &Url) -> Option<String> {
        self.store.get_document_text(uri)
    }

    pub fn remove(&self, uri: &Url) -> Option<Document> {
        self.store.remove(uri)
    }
}

impl Default for WorkspaceDocuments {
    fn default() -> Self {
        Self::new()
    }
}
