use crate::document::LanguageLayer;
use tree_sitter::Tree;

/// Manages language layers for a document without parser pool dependency
#[derive(Debug, Clone)]
pub struct LayerManager {
    root_layer: Option<LanguageLayer>,
}

impl LayerManager {
    /// Create a new empty LayerManager
    pub fn new() -> Self {
        Self { root_layer: None }
    }

    /// Create a LayerManager with a root layer
    pub fn with_root(language_id: String, tree: Tree) -> Self {
        Self {
            root_layer: Some(LanguageLayer::root(language_id, tree)),
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

    /// Get all layers (root only)
    pub fn all_layers(&self) -> Vec<&LanguageLayer> {
        let mut layers = Vec::new();
        if let Some(root) = &self.root_layer {
            layers.push(root);
        }
        layers
    }

    /// Get the language ID at the root layer
    pub fn get_language_id(&self) -> Option<&str> {
        self.root_layer.as_ref().map(|l| l.language_id.as_str())
    }

    /// Clear all layers
    pub fn clear(&mut self) {
        self.root_layer = None;
    }

    /// Check if there's a root layer
    pub fn has_root(&self) -> bool {
        self.root_layer.is_some()
    }

    /// Get layer at a specific byte offset
    pub fn get_layer_at_offset(&self, byte_offset: usize) -> Option<&LanguageLayer> {
        // Check root layer
        if let Some(root) = &self.root_layer
            && root.contains_offset(byte_offset)
        {
            return Some(root);
        }

        None
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}
