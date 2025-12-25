//! Incremental tokenization using Tree-sitter's changed_ranges API.

use tree_sitter::{Range as TsRange, Tree};

/// Determine if the changes are too large for incremental tokenization.
/// Returns true if full re-tokenization is more efficient.
///
/// Heuristics:
/// - More than 10 changed ranges = likely a large structural change
/// - Changed bytes exceed 30% of document = significant rewrite
pub fn is_large_structural_change(ranges: &[TsRange], document_len: usize) -> bool {
    const MAX_RANGES: usize = 10;
    const MAX_CHANGE_RATIO: f64 = 0.30;

    // Too many changed ranges
    if ranges.len() > MAX_RANGES {
        return true;
    }

    // Calculate total changed bytes
    let total_changed: usize = ranges
        .iter()
        .map(|r| r.end_byte.saturating_sub(r.start_byte))
        .sum();

    // More than 30% of document changed
    if document_len > 0 {
        let ratio = total_changed as f64 / document_len as f64;
        return ratio > MAX_CHANGE_RATIO;
    }

    false
}

/// Get the byte ranges that changed between two trees.
/// Returns a vector of ranges that differ between the old and new trees.
///
/// Note: For best results, the old tree should have been edited via `tree.edit()`
/// before the new tree was parsed. Without proper edit information, Tree-sitter
/// may return larger ranges than strictly necessary.
pub fn get_changed_ranges(old_tree: &Tree, new_tree: &Tree) -> Vec<TsRange> {
    old_tree.changed_ranges(new_tree).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    // Helper to create a range for testing
    fn make_range(start_byte: usize, end_byte: usize) -> TsRange {
        TsRange {
            start_byte,
            end_byte,
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 0, column: 0 },
        }
    }

    #[test]
    fn test_heuristic_large_change_triggers_full_recompute() {
        // Test 1: Few small changes - should NOT be large
        let small_changes = vec![make_range(0, 10), make_range(50, 60)];
        assert!(!is_large_structural_change(&small_changes, 1000));

        // Test 2: More than 10 ranges - should be large
        let many_ranges: Vec<_> = (0..15).map(|i| make_range(i * 10, i * 10 + 5)).collect();
        assert!(is_large_structural_change(&many_ranges, 1000));

        // Test 3: >30% of document changed - should be large
        let large_change = vec![make_range(0, 400)]; // 400 bytes out of 1000 = 40%
        assert!(is_large_structural_change(&large_change, 1000));

        // Test 4: Exactly 30% - should NOT be large (boundary)
        let boundary_change = vec![make_range(0, 300)]; // 300 bytes out of 1000 = 30%
        assert!(!is_large_structural_change(&boundary_change, 1000));
    }

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
