use crate::document::{adjust_ranges_for_edit, edit_affects_ranges, transform_edit_for_injection};
use crate::document::LanguageLayer;
use crate::language::DocumentParserPool;
use tree_sitter::{InputEdit, Parser, Range, Tree};

/// Manages language layers for a document, including root and injection layers
pub struct LayerManager {
    root_layer: Option<LanguageLayer>,
    injection_layers: Vec<LanguageLayer>,
    parser_pool: Option<DocumentParserPool>,
}

impl LayerManager {
    /// Create a new empty LayerManager
    pub fn new() -> Self {
        Self {
            root_layer: None,
            injection_layers: Vec::new(),
            parser_pool: None,
        }
    }

    /// Create a LayerManager with a root layer
    pub fn with_root(language_id: String, tree: Tree) -> Self {
        Self {
            root_layer: Some(LanguageLayer::root(language_id, tree)),
            injection_layers: Vec::new(),
            parser_pool: None,
        }
    }

    /// Get the root layer
    pub fn root_layer(&self) -> Option<&LanguageLayer> {
        self.root_layer.as_ref()
    }

    /// Get mutable reference to root layer
    pub fn root_layer_mut(&mut self) -> Option<&mut LanguageLayer> {
        self.root_layer.as_mut()
    }

    /// Get injection layers
    pub fn injection_layers(&self) -> &[LanguageLayer] {
        &self.injection_layers
    }

    /// Add an injection layer
    pub fn add_injection_layer(&mut self, layer: LanguageLayer) {
        self.injection_layers.push(layer);
    }

    /// Clear all injection layers
    pub fn clear_injections(&mut self) {
        self.injection_layers.clear();
    }

    /// Set the root layer
    pub fn set_root_layer(&mut self, layer: LanguageLayer) {
        self.root_layer = Some(layer);
    }

    /// Get the layer at a specific byte offset
    pub fn get_layer_at_offset(&self, byte_offset: usize) -> Option<&LanguageLayer> {
        // Check injection layers first (they have higher precedence)
        for layer in &self.injection_layers {
            if layer.contains_offset(byte_offset) {
                return Some(layer);
            }
        }
        // Fall back to root layer
        self.root_layer.as_ref()
    }

    /// Get all layers (root + injections)
    pub fn all_layers(&self) -> impl Iterator<Item = &LanguageLayer> {
        self.root_layer.iter().chain(self.injection_layers.iter())
    }

    /// Update the root tree
    pub fn update_root_tree(&mut self, tree: Tree) {
        if let Some(ref mut root) = self.root_layer {
            root.tree = tree;
        }
    }

    /// Apply edits to root layer and return edited tree
    pub fn apply_edits_to_root(&self, edits: &[InputEdit]) -> Option<Tree> {
        self.root_layer.as_ref().map(|layer| {
            let mut tree = layer.tree.clone();
            for edit in edits {
                tree.edit(edit);
            }
            tree
        })
    }

    /// Get language ID of root layer
    pub fn get_language_id(&self) -> Option<&String> {
        self.root_layer.as_ref().map(|l| &l.language_id)
    }

    /// Set parser pool for this document
    pub fn set_parser_pool(&mut self, pool: DocumentParserPool) {
        self.parser_pool = Some(pool);
    }

    /// Check if parser pool is initialized
    pub fn has_parser_pool(&self) -> bool {
        self.parser_pool.is_some()
    }

    /// Acquire a parser for the specified language
    pub fn acquire_parser(&mut self, language_id: &str) -> Option<Parser> {
        self.parser_pool
            .as_mut()
            .and_then(|pool| pool.acquire(language_id))
    }

    /// Release a parser back to the pool
    pub fn release_parser(&mut self, language_id: String, parser: Parser) {
        if let Some(pool) = &mut self.parser_pool {
            pool.release(language_id, parser);
        }
    }

    /// Process edits for injection layers
    pub fn process_injection_edits(&mut self, edit: &InputEdit) {
        // Update injection layer ranges based on edit
        for layer in &mut self.injection_layers {
            if !layer.ranges.is_empty() {
                adjust_ranges_for_edit(&mut layer.ranges, edit);
            }
        }
    }

    /// Check if edits affect any injection layers
    pub fn edits_affect_injections(&self, edit: &InputEdit) -> bool {
        self.injection_layers
            .iter()
            .any(|layer| edit_affects_ranges(edit, &layer.ranges))
    }

    /// Transform edit for a specific injection layer
    pub fn transform_edit_for_layer(
        &self,
        edit: &InputEdit,
        layer: &LanguageLayer,
    ) -> Option<InputEdit> {
        if layer.is_root() {
            Some(*edit)
        } else {
            transform_edit_for_injection(edit, &layer.ranges)
        }
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_layer_contains_all_offsets() {
        // Create a mock tree for testing
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("", None).unwrap();
        let layer = LanguageLayer::root("rust".to_string(), tree);

        assert!(layer.contains_offset(0));
        assert!(layer.contains_offset(100));
        assert!(layer.contains_offset(usize::MAX));
    }

    #[test]
    fn test_injection_layer_range_checking() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("", None).unwrap();
        let ranges = vec![(10, 20), (30, 40)];
        let layer = LanguageLayer::injection("markdown".to_string(), tree, ranges);

        assert!(!layer.contains_offset(0));
        assert!(!layer.contains_offset(9));
        assert!(layer.contains_offset(10));
        assert!(layer.contains_offset(15));
        assert!(layer.contains_offset(19));
        assert!(!layer.contains_offset(20));
        assert!(!layer.contains_offset(25));
        assert!(layer.contains_offset(30));
        assert!(layer.contains_offset(39));
        assert!(!layer.contains_offset(40));
    }

    #[test]
    fn test_tree_at_offset() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("", None).unwrap();
        let layer = LanguageLayer::root("rust".to_string(), tree);

        assert!(layer.tree_at_offset(0).is_some());
        assert!(layer.tree_at_offset(100).is_some());
    }
}
