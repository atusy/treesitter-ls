use tree_sitter::Tree;

/// Represents a language layer within a document, supporting injection
#[derive(Clone)]
pub struct LanguageLayer {
    /// The language identifier (e.g., "rust", "markdown", "html")
    pub language_id: String,
    
    /// The parsed tree for this language layer
    pub tree: Tree,
    
    /// Byte ranges where this layer is active (for injections)
    /// Empty vector means the entire document
    pub ranges: Vec<(usize, usize)>,
    
    /// Depth in the injection hierarchy (0 for root layer)
    pub depth: usize,
    
    /// Node ID in parent layer that triggered this injection
    pub parent_injection_node: Option<usize>,
}

impl LanguageLayer {
    /// Create a root language layer that covers the entire document
    pub fn root(language_id: String, tree: Tree) -> Self {
        Self {
            language_id,
            tree,
            ranges: vec![],
            depth: 0,
            parent_injection_node: None,
        }
    }
    
    /// Create an injection layer with specific ranges
    pub fn injection(
        language_id: String,
        tree: Tree,
        ranges: Vec<(usize, usize)>,
        depth: usize,
        parent_node: usize,
    ) -> Self {
        Self {
            language_id,
            tree,
            ranges,
            depth,
            parent_injection_node: Some(parent_node),
        }
    }
    
    /// Check if a byte offset falls within this layer's ranges
    pub fn contains_offset(&self, byte_offset: usize) -> bool {
        if self.ranges.is_empty() {
            // Root layer covers everything
            return true;
        }
        
        self.ranges.iter().any(|(start, end)| {
            byte_offset >= *start && byte_offset < *end
        })
    }
    
    /// Get the tree if the offset is within this layer
    pub fn tree_at_offset(&self, byte_offset: usize) -> Option<&Tree> {
        if self.contains_offset(byte_offset) {
            Some(&self.tree)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn test_root_layer_contains_all_offsets() {
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse("fn main() {}", None).unwrap();
        
        let layer = LanguageLayer::root("rust".to_string(), tree);
        
        assert!(layer.contains_offset(0));
        assert!(layer.contains_offset(100));
        assert!(layer.contains_offset(usize::MAX));
        assert_eq!(layer.depth, 0);
        assert!(layer.parent_injection_node.is_none());
    }
    
    #[test]
    fn test_injection_layer_range_checking() {
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse("let x = 1;", None).unwrap();
        
        let layer = LanguageLayer::injection(
            "rust".to_string(),
            tree,
            vec![(10, 20), (30, 40)],
            1,
            42,
        );
        
        // Inside ranges
        assert!(layer.contains_offset(10));
        assert!(layer.contains_offset(15));
        assert!(layer.contains_offset(19));
        assert!(layer.contains_offset(30));
        assert!(layer.contains_offset(35));
        assert!(layer.contains_offset(39));
        
        // Outside ranges
        assert!(!layer.contains_offset(9));
        assert!(!layer.contains_offset(20));
        assert!(!layer.contains_offset(25));
        assert!(!layer.contains_offset(29));
        assert!(!layer.contains_offset(40));
        assert!(!layer.contains_offset(50));
        
        assert_eq!(layer.depth, 1);
        assert_eq!(layer.parent_injection_node, Some(42));
    }
    
    #[test]
    fn test_tree_at_offset() {
        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();
        let tree = parser.parse("fn test() {}", None).unwrap();
        
        let layer = LanguageLayer::injection(
            "rust".to_string(),
            tree,
            vec![(5, 10)],
            1,
            0,
        );
        
        assert!(layer.tree_at_offset(7).is_some());
        assert!(layer.tree_at_offset(3).is_none());
        assert!(layer.tree_at_offset(12).is_none());
    }
}