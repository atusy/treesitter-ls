use tree_sitter::{InputEdit, Range, Tree};

/// A single language layer within a document
#[derive(Debug, Clone)]
pub struct LanguageLayer {
    pub language_id: String,
    pub tree: Tree,
}

impl LanguageLayer {
    /// Create a root layer (covers entire document)
    pub fn root(language_id: String, tree: Tree) -> Self {
        Self { language_id, tree }
    }

    /// Check if a byte offset is within this layer's ranges
    pub fn contains_offset(&self, _byte_offset: usize) -> bool {
        true
    }

    /// Get the tree at a specific offset
    pub fn tree_at_offset(&self, byte_offset: usize) -> Option<&Tree> {
        if self.contains_offset(byte_offset) {
            Some(&self.tree)
        } else {
            None
        }
    }

    /// Apply edits to the tree
    pub fn apply_edits(&mut self, edits: &[InputEdit]) -> Tree {
        for edit in edits {
            self.tree.edit(edit);
        }
        self.tree.clone()
    }

    /// Update tree ranges for tree-sitter parsing
    pub fn update_tree_ranges(&mut self, _ranges: &[Range]) {
        // This would be used to set included ranges on the tree
        // Currently just a placeholder for future implementation
    }
}
