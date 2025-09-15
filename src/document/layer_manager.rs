use crate::document::LanguageLayer;
use crate::text::{adjust_ranges_for_edit, edit_affects_ranges, transform_edit_for_injection};
use tree_sitter::{InputEdit, Tree};

/// Manages language layers for a document without parser pool dependency
#[derive(Debug, Clone)]
pub struct LayerManager {
    root_layer: Option<LanguageLayer>,
    injection_layers: Vec<LanguageLayer>,
}

impl LayerManager {
    /// Create a new empty LayerManager
    pub fn new() -> Self {
        Self {
            root_layer: None,
            injection_layers: Vec::new(),
        }
    }

    /// Create a LayerManager with a root layer
    pub fn with_root(language_id: String, tree: Tree) -> Self {
        Self {
            root_layer: Some(LanguageLayer::root(language_id, tree)),
            injection_layers: Vec::new(),
        }
    }

    /// Set the root layer
    pub fn set_root_layer(&mut self, layer: LanguageLayer) {
        self.root_layer = Some(layer);
    }

    /// Get the root layer
    pub fn root_layer(&self) -> Option<&LanguageLayer> {
        self.root_layer.as_ref()
    }

    /// Get mutable root layer
    pub fn root_layer_mut(&mut self) -> Option<&mut LanguageLayer> {
        self.root_layer.as_mut()
    }

    /// Get all layers (root + injections)
    pub fn all_layers(&self) -> Vec<&LanguageLayer> {
        let mut layers = Vec::new();
        if let Some(root) = &self.root_layer {
            layers.push(root);
        }
        for layer in &self.injection_layers {
            layers.push(layer);
        }
        layers
    }

    /// Get the language ID at the root layer
    pub fn get_language_id(&self) -> Option<&str> {
        self.root_layer.as_ref().map(|l| l.language_id.as_str())
    }

    /// Add an injection layer
    pub fn add_injection_layer(&mut self, layer: LanguageLayer) {
        self.injection_layers.push(layer);
    }

    /// Get injection layers
    pub fn injection_layers(&self) -> &[LanguageLayer] {
        &self.injection_layers
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
            return Some(*edit);
        }

        if !layer.ranges.is_empty() {
            transform_edit_for_injection(edit, &layer.ranges)
        } else {
            None
        }
    }

    /// Clear all layers
    pub fn clear(&mut self) {
        self.root_layer = None;
        self.injection_layers.clear();
    }

    /// Check if there's a root layer
    pub fn has_root(&self) -> bool {
        self.root_layer.is_some()
    }

    /// Get layer at a specific byte offset
    pub fn get_layer_at_offset(&self, byte_offset: usize) -> Option<&LanguageLayer> {
        // Check injection layers first (they have higher priority)
        for layer in &self.injection_layers {
            if layer.contains_offset(byte_offset) {
                return Some(layer);
            }
        }

        // Fall back to root layer
        if let Some(root) = &self.root_layer
            && root.contains_offset(byte_offset)
        {
            return Some(root);
        }

        None
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

    /// Deprecated: Parser pool is now managed separately
    /// This method is kept for backward compatibility but does nothing
    pub fn set_parser_pool(&mut self, _pool: crate::language::DocumentParserPool) {
        // Parser pool is now managed at a different level
        // This method is kept only for compatibility
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}
