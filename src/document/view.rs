use crate::document::Document;
use std::ops::Deref;

/// Read-only view into a document for analysis operations
pub trait DocumentView {
    /// Access the document text
    fn text(&self) -> &str;

    /// Access the tree
    fn tree(&self) -> Option<&tree_sitter::Tree>;

    /// Access the language ID
    fn language_id(&self) -> Option<&str>;

    /// Get a position mapper for this document
    fn position_mapper(&self) -> crate::text::SimplePositionMapper<'_> {
        crate::text::SimplePositionMapper::new(self.text())
    }
}

impl<T> DocumentView for T
where
    T: Deref<Target = Document>,
{
    fn text(&self) -> &str {
        self.deref().text()
    }

    fn tree(&self) -> Option<&tree_sitter::Tree> {
        self.deref().tree()
    }

    fn language_id(&self) -> Option<&str> {
        self.deref().language_id()
    }
}
