use tree_sitter::Tree;

/// Unified document structure combining text, parsing, and LSP state
pub struct Document {
    text: String,
    version: Option<i32>,
    language_id: Option<String>,
    tree: Option<Tree>,
    /// Previous tree for changed_ranges comparison during incremental parsing
    previous_tree: Option<Tree>,
}

impl Document {
    /// Create a new document with just text
    pub fn new(text: String) -> Self {
        Self {
            text,
            version: None,
            language_id: None,
            tree: None,
            previous_tree: None,
        }
    }

    /// Create a new document with version
    pub fn with_version(text: String, version: i32) -> Self {
        Self {
            text,
            version: Some(version),
            language_id: None,
            tree: None,
            previous_tree: None,
        }
    }

    /// Create with language and tree
    pub fn with_tree(text: String, language_id: String, tree: Tree) -> Self {
        Self {
            text,
            version: None,
            language_id: Some(language_id),
            tree: Some(tree),
            previous_tree: None,
        }
    }

    /// Get the text content
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the text content as owned String
    pub fn into_text(self) -> String {
        self.text
    }

    /// Get the document version
    pub fn version(&self) -> Option<i32> {
        self.version
    }

    /// Set the document version
    pub fn set_version(&mut self, version: Option<i32>) {
        self.version = version;
    }

    /// Get the language ID
    pub fn language_id(&self) -> Option<&str> {
        self.language_id.as_deref()
    }

    /// Get the tree
    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    /// Get a position mapper for this document
    pub fn position_mapper(&self) -> crate::text::PositionMapper {
        crate::text::PositionMapper::new(self.text())
    }

    /// Get mutable tree
    pub fn tree_mut(&mut self) -> Option<&mut Tree> {
        self.tree.as_mut()
    }

    /// Get the previous tree (used for changed_ranges comparison)
    pub fn previous_tree(&self) -> Option<&Tree> {
        self.previous_tree.as_ref()
    }

    /// Update tree, moving current tree to previous_tree
    ///
    /// This preserves the previous tree for changed_ranges comparison
    /// during incremental parsing optimization.
    pub fn update_tree(&mut self, new_tree: Tree) {
        self.previous_tree = self.tree.take();
        self.tree = Some(new_tree);
    }

    /// Set the tree and language
    pub fn set_tree(&mut self, language_id: String, tree: Tree) {
        self.language_id = Some(language_id);
        self.tree = Some(tree);
    }

    /// Clear the tree and language
    pub fn clear_tree(&mut self) {
        self.language_id = None;
        self.tree = None;
        self.previous_tree = None;
    }

    /// Update text and clear layers/state
    pub fn update_text(&mut self, text: String) {
        self.text = text;
        // Note: Tree needs to be rebuilt after text change
        self.tree = None;
        self.previous_tree = None;
    }

    /// Get the length in bytes
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Check if the document is empty
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document::new("hello world".to_string());
        assert_eq!(doc.text(), "hello world");
        assert_eq!(doc.version(), None);
        assert_eq!(doc.len(), 11);
        assert!(!doc.is_empty());
    }

    #[test]
    fn test_document_with_version() {
        let doc = Document::with_version("test".to_string(), 42);
        assert_eq!(doc.text(), "test");
        assert_eq!(doc.version(), Some(42));
    }

    #[test]
    fn test_document_with_layer() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("fn main() {}", None).unwrap();

        let doc = Document::with_tree("fn main() {}".to_string(), "rust".to_string(), tree);

        assert_eq!(doc.text(), "fn main() {}");
        assert!(doc.tree().is_some());
        assert_eq!(doc.language_id(), Some("rust"));
    }

    #[test]
    fn test_update_text() {
        let mut doc = Document::new("initial".to_string());
        doc.update_text("updated".to_string());
        assert_eq!(doc.text(), "updated");
        assert!(doc.tree().is_none());
    }

    #[test]
    fn test_document_preserves_previous_tree() {
        // Create document with initial tree
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree1 = parser.parse("fn main() {}", None).unwrap();

        let mut doc = Document::with_tree("fn main() {}".to_string(), "rust".to_string(), tree1);

        // Initially no previous tree
        assert!(doc.previous_tree().is_none());

        // Update tree - old should become previous
        let tree2 = parser.parse("fn main() { let x = 1; }", None).unwrap();
        doc.update_tree(tree2);

        // Now previous tree should exist
        assert!(doc.previous_tree().is_some());
        // Current tree should be the new one
        assert!(doc.tree().is_some());
    }
}
