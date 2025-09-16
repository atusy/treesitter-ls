use crate::document::{LanguageLayer, LayerManager};
use tree_sitter::Tree;

/// Read-only view interface for document analysis
/// Provides access to document text, tree, and language information
/// without exposing mutability or implementation details
pub trait DocumentView {
    fn text(&self) -> &str;
    fn tree(&self) -> Option<&Tree>;
    fn language_id(&self) -> Option<&str>;
    fn get_layer_at_offset(&self, offset: usize) -> Option<&LanguageLayer>;
    fn root_layer(&self) -> Option<&LanguageLayer>;
}

/// Concrete implementation of DocumentView for Document
pub struct DocumentViewImpl<'a> {
    text: &'a str,
    layers: &'a LayerManager,
}

impl<'a> DocumentViewImpl<'a> {
    pub fn new(text: &'a str, layers: &'a LayerManager) -> Self {
        Self { text, layers }
    }
}

impl<'a> DocumentView for DocumentViewImpl<'a> {
    fn text(&self) -> &str {
        self.text
    }

    fn tree(&self) -> Option<&Tree> {
        self.layers.root_layer().map(|l| &l.tree)
    }

    fn language_id(&self) -> Option<&str> {
        self.layers.get_language_id()
    }

    fn get_layer_at_offset(&self, offset: usize) -> Option<&LanguageLayer> {
        self.layers.get_layer_at_offset(offset)
    }

    fn root_layer(&self) -> Option<&LanguageLayer> {
        self.layers.root_layer()
    }
}