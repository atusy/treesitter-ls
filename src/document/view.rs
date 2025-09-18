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

    /// Build a position mapper suited to the document's layering configuration.
    fn position_mapper(&self) -> Box<dyn crate::text::PositionMapper + '_> {
        Box::new(crate::text::SimplePositionMapper::new(self.text()))
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
