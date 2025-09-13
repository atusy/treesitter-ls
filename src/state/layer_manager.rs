use crate::state::language_layer::LanguageLayer;
use crate::state::parser_pool::DocumentParserPool;
use tree_sitter::{InputEdit, Parser, Tree};

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

    /// Clear injection layers
    pub fn clear_injection_layers(&mut self) {
        self.injection_layers.clear();
    }

    /// Get the primary language layer at a specific byte offset
    pub fn get_layer_at_offset(&self, byte_offset: usize) -> Option<&LanguageLayer> {
        // Check injection layers first (they have higher priority)
        for layer in &self.injection_layers {
            if layer.contains_offset(byte_offset) {
                return Some(layer);
            }
        }

        // Fall back to root layer
        self.root_layer.as_ref()
    }

    /// Get all language layers (root + injections)
    pub fn all_layers(&self) -> impl Iterator<Item = &LanguageLayer> {
        self.root_layer.iter().chain(self.injection_layers.iter())
    }

    /// Update the root layer's tree
    pub fn update_root_tree(&mut self, tree: Tree) {
        if let Some(root) = &mut self.root_layer {
            root.tree = tree;
        }
    }

    /// Apply edits to the root tree and return the edited tree
    pub fn apply_edits_to_root(&self, edits: &[InputEdit]) -> Option<Tree> {
        self.root_layer.as_ref().map(|layer| {
            let mut tree = layer.tree.clone();
            for edit in edits {
                tree.edit(edit);
            }
            tree
        })
    }

    /// Apply edits to all layers (root and injections)
    pub fn apply_edits_to_all(&mut self, edits: &[InputEdit]) {
        // Apply to root layer
        if let Some(root) = &mut self.root_layer {
            for edit in edits {
                root.tree.edit(edit);
            }
        }

        // Apply to injection layers
        // Note: This is simplified - injection layers may need different edit handling
        // based on their position in the document
        for layer in &mut self.injection_layers {
            for edit in edits {
                layer.tree.edit(edit);
            }
        }
    }

    /// Get the language_id from root layer
    pub fn get_language_id(&self) -> Option<&String> {
        self.root_layer.as_ref().map(|layer| &layer.language_id)
    }

    /// Set the parser pool
    pub fn set_parser_pool(&mut self, pool: DocumentParserPool) {
        self.parser_pool = Some(pool);
    }

    /// Get the parser pool
    pub fn parser_pool(&self) -> Option<&DocumentParserPool> {
        self.parser_pool.as_ref()
    }

    /// Get mutable parser pool
    pub fn parser_pool_mut(&mut self) -> Option<&mut DocumentParserPool> {
        self.parser_pool.as_mut()
    }

    /// Acquire a parser for the specified language
    /// Returns None if no parser pool is set
    pub fn acquire_parser(&mut self, language_id: &str) -> Option<Parser> {
        self.parser_pool.as_mut()?.acquire(language_id)
    }

    /// Acquire an injection parser for the specified language
    /// Returns None if no parser pool is set
    pub fn acquire_injection_parser(
        &mut self,
        language_id: &str,
        timeout_micros: Option<u64>,
    ) -> Option<Parser> {
        self.parser_pool
            .as_mut()?
            .acquire_injection(language_id, timeout_micros)
    }

    /// Release a parser back to the pool
    pub fn release_parser(&mut self, language_id: String, parser: Parser) {
        if let Some(pool) = self.parser_pool.as_mut() {
            pool.release(language_id, parser);
        }
    }

    /// Parse text for a specific layer
    /// This method manages parser acquisition and release automatically
    pub fn parse_layer(
        &mut self,
        language_id: &str,
        text: &str,
        old_tree: Option<&Tree>,
    ) -> Option<Tree> {
        let mut parser = self.acquire_parser(language_id)?;
        let tree = parser.parse(text, old_tree);
        self.release_parser(language_id.to_string(), parser);
        tree
    }
}

impl Default for LayerManager {
    fn default() -> Self {
        Self::new()
    }
}
