//! Hierarchy chain utilities for SelectionRange manipulation.
//!
//! This module provides pure functions for comparing and chaining LSP SelectionRange
//! hierarchies, including range comparison utilities and parent chain manipulation.

use tower_lsp::lsp_types::Range;

/// Check if range `a` is strictly larger than range `b`.
///
/// A range is strictly larger if it fully contains `b` (starts at or before, ends at or after)
/// AND is not equal to `b`. This is used to ensure LSP selection ranges are strictly expanding.
///
/// # Arguments
/// * `a` - The potentially containing range
/// * `b` - The range to check containment of
///
/// # Returns
/// `true` if `a` strictly contains `b` (contains but not equal)
pub fn is_range_strictly_larger(a: &Range, b: &Range) -> bool {
    let a_start = (a.start.line, a.start.character);
    let a_end = (a.end.line, a.end.character);
    let b_start = (b.start.line, b.start.character);
    let b_end = (b.end.line, b.end.character);

    // a contains b: a_start <= b_start && a_end >= b_end
    let contains = a_start <= b_start && a_end >= b_end;
    // a is not equal to b
    let not_equal = a_start != b_start || a_end != b_end;

    contains && not_equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[test]
    fn test_is_range_strictly_larger_when_a_contains_b() {
        // Range a: lines 0-10, Range b: lines 2-5
        let a = Range::new(Position::new(0, 0), Position::new(10, 0));
        let b = Range::new(Position::new(2, 0), Position::new(5, 0));

        assert!(
            is_range_strictly_larger(&a, &b),
            "a should strictly contain b"
        );
    }

    #[test]
    fn test_is_range_strictly_larger_when_equal() {
        // Equal ranges should NOT be strictly larger
        let a = Range::new(Position::new(2, 5), Position::new(5, 10));
        let b = Range::new(Position::new(2, 5), Position::new(5, 10));

        assert!(
            !is_range_strictly_larger(&a, &b),
            "equal ranges should not be strictly larger"
        );
    }

    #[test]
    fn test_is_range_strictly_larger_when_b_contains_a() {
        // b contains a - should return false
        let a = Range::new(Position::new(2, 0), Position::new(5, 0));
        let b = Range::new(Position::new(0, 0), Position::new(10, 0));

        assert!(
            !is_range_strictly_larger(&a, &b),
            "a does not contain b"
        );
    }

    #[test]
    fn test_is_range_strictly_larger_when_disjoint() {
        // Disjoint ranges
        let a = Range::new(Position::new(0, 0), Position::new(5, 0));
        let b = Range::new(Position::new(10, 0), Position::new(15, 0));

        assert!(
            !is_range_strictly_larger(&a, &b),
            "disjoint ranges should not be strictly larger"
        );
    }

    #[test]
    fn test_is_range_strictly_larger_same_start_different_end() {
        // Same start, a ends later
        let a = Range::new(Position::new(2, 5), Position::new(10, 0));
        let b = Range::new(Position::new(2, 5), Position::new(5, 0));

        assert!(
            is_range_strictly_larger(&a, &b),
            "same start but a ends later should be strictly larger"
        );
    }

    #[test]
    fn test_is_range_strictly_larger_same_end_different_start() {
        // Same end, a starts earlier
        let a = Range::new(Position::new(0, 0), Position::new(10, 0));
        let b = Range::new(Position::new(5, 0), Position::new(10, 0));

        assert!(
            is_range_strictly_larger(&a, &b),
            "same end but a starts earlier should be strictly larger"
        );
    }
}
