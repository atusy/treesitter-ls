use tree_sitter::{InputEdit, Range, Tree};

/// A single language layer within a document
#[derive(Debug, Clone)]
pub struct LanguageLayer {
    pub language_id: String,
    pub tree: Tree,
    pub ranges: Vec<(usize, usize)>, // Byte ranges where this layer applies
}

impl LanguageLayer {
    /// Create a root layer (covers entire document)
    pub fn root(language_id: String, tree: Tree) -> Self {
        Self {
            language_id,
            tree,
            ranges: vec![],
        }
    }

    /// Create an injection layer with specific ranges
    pub fn injection(language_id: String, tree: Tree, ranges: Vec<(usize, usize)>) -> Self {
        Self {
            language_id,
            tree,
            ranges,
        }
    }

    /// Check if this is a root layer
    pub fn is_root(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Check if a byte offset is within this layer's ranges
    pub fn contains_offset(&self, byte_offset: usize) -> bool {
        if self.is_root() {
            true
        } else {
            self.ranges
                .iter()
                .any(|(start, end)| byte_offset >= *start && byte_offset < *end)
        }
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