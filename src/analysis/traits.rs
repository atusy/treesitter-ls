use crate::document::{LayerManager, LanguageLayer};
use tree_sitter::Tree;

/// Trait for providing analysis context to LSP features
/// This abstraction allows analysis functions to work without direct Document dependency
pub trait AnalysisContext {
    /// Get the text content of the document
    fn text(&self) -> &str;

    /// Get the primary syntax tree
    fn tree(&self) -> Option<&Tree>;

    /// Get the language ID
    fn language_id(&self) -> Option<&str>;

    /// Get the layer manager for injection handling
    fn layers(&self) -> &LayerManager;

    /// Get layer at specific offset
    fn get_layer_at_offset(&self, offset: usize) -> Option<&LanguageLayer> {
        self.layers().get_layer_at_offset(offset)
    }
}