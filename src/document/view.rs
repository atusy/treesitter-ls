use crate::document::{Document, LanguageLayer, LayerManager};
use std::ops::Deref;

/// Read-only view into a document for analysis operations
pub trait DocumentView {
    /// Access the document text
    fn text(&self) -> &str;

    /// Access the layer manager
    fn layers(&self) -> &LayerManager;

    /// Convenience: language ID of the root layer, if present
    fn language_id(&self) -> Option<&str> {
        self.layers().get_language_id()
    }

    /// Convenience: access the layer covering a byte offset
    fn get_layer_at_offset(&self, offset: usize) -> Option<&LanguageLayer> {
        self.layers().get_layer_at_offset(offset)
    }

    /// Convenience: access the configured injection layers
    fn injection_layers(&self) -> &[LanguageLayer] {
        self.layers().injection_layers()
    }
}

impl<T> DocumentView for T
where
    T: Deref<Target = Document>,
{
    fn text(&self) -> &str {
        self.deref().text()
    }

    fn layers(&self) -> &LayerManager {
        self.deref().layers()
    }
}
