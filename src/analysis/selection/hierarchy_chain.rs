//! Hierarchy chain utilities for SelectionRange manipulation.
//!
//! This module provides pure functions for comparing and chaining LSP SelectionRange
//! hierarchies, including range comparison utilities and parent chain manipulation.

use tower_lsp_server::ls_types::{Range, SelectionRange};

/// Check if two ranges are equal.
///
/// This is a simple equality check for Range start and end positions.
/// Note: Range implements PartialEq, but this explicit function documents the intent.
pub fn ranges_equal(a: &Range, b: &Range) -> bool {
    a.start == b.start && a.end == b.end
}

/// Check if outer range fully contains inner range.
///
/// Returns true if inner is completely within outer (inclusive boundaries).
/// Unlike `is_range_strictly_larger`, this returns true for equal ranges.
pub fn range_contains(outer: &Range, inner: &Range) -> bool {
    (outer.start.line < inner.start.line
        || (outer.start.line == inner.start.line && outer.start.character <= inner.start.character))
        && (outer.end.line > inner.end.line
            || (outer.end.line == inner.end.line && outer.end.character >= inner.end.character))
}

/// Check if range `a` is strictly larger than range `b`.
///
/// A range is strictly larger if it fully contains `b` (starts at or before, ends at or after)
/// AND is not equal to `b`. This is used to ensure LSP selection ranges are strictly expanding.
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

/// Skip host selection ranges until we find one that is strictly larger than the tail range.
///
/// This ensures LSP selection ranges are strictly expanding (no duplicates or contained ranges).
/// Used when chaining injected selection hierarchies to host document hierarchies.
///
/// # Arguments
/// * `host` - The starting host selection range (may be None)
/// * `tail_range` - The range to compare against
///
/// # Returns
/// The first SelectionRange in the host chain that strictly contains `tail_range`, or None
pub fn skip_to_distinct_host(
    host: Option<SelectionRange>,
    tail_range: &Range,
) -> Option<SelectionRange> {
    let mut current = host;
    while let Some(selection) = current {
        if is_range_strictly_larger(&selection.range, tail_range) {
            return Some(selection);
        }
        // This host range is not larger - skip to its parent
        current = selection.parent.map(|p| *p);
    }
    None
}

/// Chain the injected selection hierarchy to the host document hierarchy.
///
/// This function finds the tail (root) of the injected selection chain and connects
/// it to the first host selection range that is strictly larger. This ensures the
/// combined hierarchy has strictly expanding ranges as required by LSP spec.
///
/// # Arguments
/// * `injected` - The injected selection range (will be modified)
/// * `host` - The host document's selection range to connect to
///
/// # Returns
/// The injected SelectionRange with its tail connected to the appropriate host range
pub fn chain_injected_to_host(
    mut injected: SelectionRange,
    host: Option<SelectionRange>,
) -> SelectionRange {
    // Find the end of the injected chain (the injected root) and connect to host
    fn find_and_connect_tail(selection: &mut SelectionRange, host: Option<SelectionRange>) {
        if selection.parent.is_none() {
            // This is the tail - connect to the first host range that is strictly larger
            let tail_range = &selection.range;
            let distinct_host = skip_to_distinct_host(host, tail_range);
            selection.parent = distinct_host.map(Box::new);
        } else if let Some(ref mut parent) = selection.parent {
            find_and_connect_tail(parent, host);
        }
    }

    find_and_connect_tail(&mut injected, host);
    injected
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::Position;

    // Helper to create a simple SelectionRange
    fn make_selection(
        start: (u32, u32),
        end: (u32, u32),
        parent: Option<SelectionRange>,
    ) -> SelectionRange {
        SelectionRange {
            range: Range::new(Position::new(start.0, start.1), Position::new(end.0, end.1)),
            parent: parent.map(Box::new),
        }
    }

    #[test]
    fn test_skip_to_distinct_host_finds_larger_range() {
        // Create a chain: small (2,0)-(5,0) -> medium (1,0)-(8,0) -> large (0,0)-(10,0)
        let large = make_selection((0, 0), (10, 0), None);
        let medium = make_selection((1, 0), (8, 0), Some(large));
        let small = make_selection((2, 0), (5, 0), Some(medium));

        // Looking for something strictly larger than (3,0)-(4,0)
        let tail = Range::new(Position::new(3, 0), Position::new(4, 0));

        let result = skip_to_distinct_host(Some(small), &tail);

        // Should return the first one that strictly contains (3,0)-(4,0), which is small (2,0)-(5,0)
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.range.start, Position::new(2, 0));
        assert_eq!(r.range.end, Position::new(5, 0));
    }

    #[test]
    fn test_skip_to_distinct_host_skips_equal_range() {
        // Create a chain where first element equals the tail
        let large = make_selection((0, 0), (10, 0), None);
        let equal_to_tail = make_selection((3, 0), (4, 0), Some(large));

        let tail = Range::new(Position::new(3, 0), Position::new(4, 0));

        let result = skip_to_distinct_host(Some(equal_to_tail), &tail);

        // Should skip the equal range and return large
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.range.start, Position::new(0, 0));
        assert_eq!(r.range.end, Position::new(10, 0));
    }

    #[test]
    fn test_skip_to_distinct_host_returns_none_when_no_larger() {
        // Create a chain where nothing is larger
        let small = make_selection((5, 0), (6, 0), None);

        // Tail is larger than anything in chain
        let tail = Range::new(Position::new(0, 0), Position::new(10, 0));

        let result = skip_to_distinct_host(Some(small), &tail);

        assert!(result.is_none());
    }

    #[test]
    fn test_skip_to_distinct_host_handles_none_input() {
        let tail = Range::new(Position::new(0, 0), Position::new(5, 0));

        let result = skip_to_distinct_host(None, &tail);

        assert!(result.is_none());
    }

    #[test]
    fn test_chain_injected_to_host_connects_to_larger_range() {
        // Injected chain: inner (5,0)-(6,0) -> middle (4,0)-(7,0) -> (no parent)
        let middle = make_selection((4, 0), (7, 0), None);
        let inner = make_selection((5, 0), (6, 0), Some(middle));

        // Host chain: large (0,0)-(10,0)
        let host = make_selection((0, 0), (10, 0), None);

        let result = chain_injected_to_host(inner, Some(host));

        // Inner should still be (5,0)-(6,0)
        assert_eq!(result.range.start, Position::new(5, 0));
        assert_eq!(result.range.end, Position::new(6, 0));

        // Walk to the tail and check it's connected to host
        let parent = result.parent.expect("should have parent");
        assert_eq!(parent.range.start, Position::new(4, 0)); // middle

        let grandparent = parent.parent.expect("should connect to host");
        assert_eq!(grandparent.range.start, Position::new(0, 0)); // host
    }

    #[test]
    fn test_chain_injected_to_host_with_none_host() {
        let inner = make_selection((5, 0), (6, 0), None);

        let result = chain_injected_to_host(inner, None);

        // Should return unchanged
        assert_eq!(result.range.start, Position::new(5, 0));
        assert!(result.parent.is_none());
    }

    #[test]
    fn test_chain_injected_to_host_skips_smaller_host() {
        // Injected: (2,0)-(8,0)
        let injected = make_selection((2, 0), (8, 0), None);

        // Host chain: small (3,0)-(5,0) -> large (0,0)-(10,0)
        let large = make_selection((0, 0), (10, 0), None);
        let small = make_selection((3, 0), (5, 0), Some(large));

        let result = chain_injected_to_host(injected, Some(small));

        // Should skip small and connect to large
        let parent = result.parent.expect("should connect to host");
        assert_eq!(parent.range.start, Position::new(0, 0));
        assert_eq!(parent.range.end, Position::new(10, 0));
    }

    #[test]
    fn test_ranges_equal_when_equal() {
        let a = Range::new(Position::new(2, 5), Position::new(10, 3));
        let b = Range::new(Position::new(2, 5), Position::new(10, 3));

        assert!(ranges_equal(&a, &b));
    }

    #[test]
    fn test_ranges_equal_when_different_start() {
        let a = Range::new(Position::new(2, 5), Position::new(10, 3));
        let b = Range::new(Position::new(3, 5), Position::new(10, 3));

        assert!(!ranges_equal(&a, &b));
    }

    #[test]
    fn test_ranges_equal_when_different_end() {
        let a = Range::new(Position::new(2, 5), Position::new(10, 3));
        let b = Range::new(Position::new(2, 5), Position::new(11, 3));

        assert!(!ranges_equal(&a, &b));
    }

    #[test]
    fn test_range_contains_when_outer_contains_inner() {
        let outer = Range::new(Position::new(0, 0), Position::new(10, 0));
        let inner = Range::new(Position::new(2, 0), Position::new(5, 0));

        assert!(range_contains(&outer, &inner));
    }

    #[test]
    fn test_range_contains_when_equal() {
        let outer = Range::new(Position::new(2, 5), Position::new(5, 10));
        let inner = Range::new(Position::new(2, 5), Position::new(5, 10));

        assert!(
            range_contains(&outer, &inner),
            "equal ranges should be contained"
        );
    }

    #[test]
    fn test_range_contains_when_disjoint() {
        let outer = Range::new(Position::new(0, 0), Position::new(5, 0));
        let inner = Range::new(Position::new(10, 0), Position::new(15, 0));

        assert!(!range_contains(&outer, &inner));
    }

    #[test]
    fn test_range_contains_when_inner_is_larger() {
        let outer = Range::new(Position::new(2, 0), Position::new(5, 0));
        let inner = Range::new(Position::new(0, 0), Position::new(10, 0));

        assert!(
            !range_contains(&outer, &inner),
            "outer does not contain larger inner"
        );
    }

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

        assert!(!is_range_strictly_larger(&a, &b), "a does not contain b");
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
