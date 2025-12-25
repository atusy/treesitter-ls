//! Incremental tokenization using Tree-sitter's changed_ranges API.

use tree_sitter::{Range as TsRange, Tree};

/// Get the byte ranges that changed between two trees.
/// Returns a vector of ranges that differ between the old and new trees.
///
/// Note: For best results, the old tree should have been edited via `tree.edit()`
/// before the new tree was parsed. Without proper edit information, Tree-sitter
/// may return larger ranges than strictly necessary.
pub fn get_changed_ranges(_old_tree: &Tree, _new_tree: &Tree) -> Vec<TsRange> {
    // TODO: Implement
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_changed_ranges_returns_affected_regions() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Parse original code
        let old_tree = parser.parse("fn main() {}", None).unwrap();

        // Parse new code with additional content
        let new_tree = parser.parse("fn main() { let x = 1; }", None).unwrap();

        // Get changed ranges
        let ranges = get_changed_ranges(&old_tree, &new_tree);

        // Should have at least one changed range
        assert!(!ranges.is_empty(), "Should detect changes between trees");
    }
}
