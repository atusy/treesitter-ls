use tree_sitter::InputEdit;

/// Transform an edit to be relative to injection layer ranges
/// Returns None if the edit doesn't affect the injection layer
pub fn transform_edit_for_injection(
    edit: &InputEdit,
    ranges: &[(usize, usize)],
) -> Option<InputEdit> {
    if ranges.is_empty() {
        // Root layer - return edit as-is
        return Some(*edit);
    }

    // Check if edit affects any of the injection ranges
    let mut layer_offset = 0;
    let mut affected = false;
    let mut transformed_start = 0;
    let mut transformed_old_end = 0;
    let mut transformed_new_end = 0;

    for (range_start, range_end) in ranges {
        let range_len = range_end - range_start;

        // Check if edit starts within this range
        if edit.start_byte >= *range_start && edit.start_byte < *range_end {
            affected = true;
            transformed_start = layer_offset + (edit.start_byte - range_start);

            // Calculate old_end_byte relative to layer
            if edit.old_end_byte <= *range_end {
                transformed_old_end = layer_offset + (edit.old_end_byte - range_start);
            } else {
                // Edit extends beyond this range
                transformed_old_end = layer_offset + range_len;
            }

            // Calculate new_end_byte
            let edit_delta = edit.new_end_byte as i64 - edit.old_end_byte as i64;
            transformed_new_end = (transformed_old_end as i64 + edit_delta).max(0) as usize;

            break;
        }

        layer_offset += range_len;
    }

    if !affected {
        return None;
    }

    Some(InputEdit {
        start_byte: transformed_start,
        old_end_byte: transformed_old_end,
        new_end_byte: transformed_new_end,
        start_position: edit.start_position,
        old_end_position: edit.old_end_position,
        new_end_position: edit.new_end_position,
    })
}

/// Adjust injection ranges based on an edit
pub fn adjust_ranges_for_edit(ranges: &mut Vec<(usize, usize)>, edit: &InputEdit) {
    let edit_delta = edit.new_end_byte as i64 - edit.old_end_byte as i64;

    let mut i = 0;
    while i < ranges.len() {
        let (start, end) = &mut ranges[i];

        // Case 1: Range is completely before the edit - no change needed
        if *end <= edit.start_byte {
            i += 1;
            continue;
        }

        // Case 2: Range is completely after the edit - shift by delta
        if *start >= edit.old_end_byte {
            *start = (*start as i64 + edit_delta).max(0) as usize;
            *end = (*end as i64 + edit_delta).max(0) as usize;
            i += 1;
            continue;
        }

        // Case 3: Edit starts before range but ends within or after it
        if edit.start_byte < *start && edit.old_end_byte > *start {
            if edit.old_end_byte >= *end {
                // Range is completely within the edit - remove it
                ranges.remove(i);
                continue;
            } else {
                // Edit partially overlaps the beginning of the range
                *start = edit.new_end_byte;
                *end = (*end as i64 + edit_delta).max(0) as usize;
            }
        }
        // Case 4: Edit is completely within the range
        else if edit.start_byte >= *start && edit.old_end_byte <= *end {
            *end = (*end as i64 + edit_delta).max(0) as usize;
        }
        // Case 5: Edit starts within range but ends after it
        else if edit.start_byte >= *start && edit.start_byte < *end && edit.old_end_byte > *end {
            *end = edit.start_byte + (edit.new_end_byte - edit.start_byte);
        }

        i += 1;
    }

    // Remove any zero-length ranges
    ranges.retain(|(start, end)| start < end);
}

/// Check if an edit affects any of the given ranges
pub fn edit_affects_ranges(edit: &InputEdit, ranges: &[(usize, usize)]) -> bool {
    if ranges.is_empty() {
        return true; // Root layer is always affected
    }

    ranges.iter().any(|(start, end)| {
        // Edit overlaps with range if:
        // - Edit starts before range ends AND
        // - Edit ends after range starts
        edit.start_byte < *end && edit.old_end_byte > *start
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_edit(start: usize, old_end: usize, new_end: usize) -> InputEdit {
        InputEdit {
            start_byte: start,
            old_end_byte: old_end,
            new_end_byte: new_end,
            start_position: tree_sitter::Point { row: 0, column: 0 },
            old_end_position: tree_sitter::Point { row: 0, column: 0 },
            new_end_position: tree_sitter::Point { row: 0, column: 0 },
        }
    }

    #[test]
    fn test_transform_edit_for_root_layer() {
        let edit = create_edit(10, 15, 20);
        let ranges = vec![];

        let transformed = transform_edit_for_injection(&edit, &ranges);
        assert!(transformed.is_some());
        let transformed = transformed.unwrap();
        assert_eq!(transformed.start_byte, 10);
        assert_eq!(transformed.old_end_byte, 15);
        assert_eq!(transformed.new_end_byte, 20);
    }

    #[test]
    fn test_transform_edit_within_single_range() {
        let edit = create_edit(15, 20, 25); // Edit within range [10, 30)
        let ranges = vec![(10, 30)];

        let transformed = transform_edit_for_injection(&edit, &ranges);
        assert!(transformed.is_some());
        let transformed = transformed.unwrap();
        assert_eq!(transformed.start_byte, 5); // 15 - 10
        assert_eq!(transformed.old_end_byte, 10); // 20 - 10
        assert_eq!(transformed.new_end_byte, 15); // 25 - 10
    }

    #[test]
    fn test_transform_edit_outside_ranges() {
        let edit = create_edit(5, 10, 15); // Edit before range [20, 30)
        let ranges = vec![(20, 30)];

        let transformed = transform_edit_for_injection(&edit, &ranges);
        assert!(transformed.is_none());
    }

    #[test]
    fn test_adjust_ranges_insertion() {
        let mut ranges = vec![(10, 20), (30, 40)];
        let edit = create_edit(5, 5, 10); // Insert 5 bytes at position 5

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(15, 25), (35, 45)]);
    }

    #[test]
    fn test_adjust_ranges_deletion() {
        let mut ranges = vec![(10, 20), (30, 40)];
        let edit = create_edit(5, 10, 5); // Delete 5 bytes starting at position 5

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(5, 15), (25, 35)]);
    }

    #[test]
    fn test_adjust_ranges_deletion_within_range() {
        let mut ranges = vec![(10, 30)];
        let edit = create_edit(15, 20, 15); // Delete 5 bytes within the range

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(10, 25)]);
    }

    #[test]
    fn test_adjust_ranges_deletion_spanning_range() {
        let mut ranges = vec![(10, 20), (30, 40), (50, 60)];
        let edit = create_edit(15, 35, 15); // Delete from middle of first range to middle of second

        adjust_ranges_for_edit(&mut ranges, &edit);

        // First range should be truncated, second range should be adjusted, third shifted
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[0], (10, 15)); // Truncated at edit start
        assert_eq!(ranges[1], (15, 20)); // Remainder of second range, moved to edit position
        assert_eq!(ranges[2], (30, 40)); // Third range shifted by -20 (from 50,60)
    }

    #[test]
    fn test_edit_affects_ranges() {
        let ranges = vec![(10, 20), (30, 40)];

        // Edit before all ranges
        assert!(!edit_affects_ranges(&create_edit(0, 5, 10), &ranges));

        // Edit between ranges
        assert!(!edit_affects_ranges(&create_edit(22, 28, 25), &ranges));

        // Edit overlapping first range
        assert!(edit_affects_ranges(&create_edit(15, 25, 20), &ranges));

        // Edit within second range
        assert!(edit_affects_ranges(&create_edit(32, 35, 40), &ranges));

        // Edit spanning both ranges
        assert!(edit_affects_ranges(&create_edit(15, 35, 20), &ranges));
    }

    // Additional edge case tests

    #[test]
    fn test_adjust_ranges_boundary_exact_start() {
        let mut ranges = vec![(10, 20), (30, 40)];
        let edit = create_edit(10, 10, 15); // Insert at exact start of first range

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(15, 25), (35, 45)]);
    }

    #[test]
    fn test_adjust_ranges_boundary_exact_end() {
        let mut ranges = vec![(10, 20), (30, 40)];
        let edit = create_edit(20, 20, 25); // Insert at exact end of first range

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(10, 20), (35, 45)]);
    }

    #[test]
    fn test_adjust_ranges_delete_entire_range() {
        let mut ranges = vec![(10, 20), (30, 40), (50, 60)];
        let edit = create_edit(10, 20, 10); // Delete entire first range

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(20, 30), (40, 50)]);
    }

    #[test]
    fn test_adjust_ranges_delete_multiple_ranges() {
        let mut ranges = vec![(10, 20), (30, 40), (50, 60)];
        let edit = create_edit(5, 55, 5); // Delete across multiple ranges

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(5, 10)]);
    }

    #[test]
    fn test_adjust_ranges_zero_length_removal() {
        let mut ranges = vec![(10, 20), (30, 40)];
        let edit = create_edit(15, 25, 15); // Delete creates zero-length range

        adjust_ranges_for_edit(&mut ranges, &edit);

        // Should remove any zero-length ranges
        assert_eq!(ranges.len(), 2);
        assert!(ranges.iter().all(|(s, e)| s < e));
    }

    #[test]
    fn test_transform_edit_across_range_boundary() {
        let edit = create_edit(25, 35, 40); // Edit spans from first to second range
        let ranges = vec![(10, 30), (40, 60)];

        let transformed = transform_edit_for_injection(&edit, &ranges);
        assert!(transformed.is_some());
        let transformed = transformed.unwrap();
        assert_eq!(transformed.start_byte, 15); // 25 - 10
        assert_eq!(transformed.old_end_byte, 20); // Clamped to range end
    }

    #[test]
    fn test_transform_edit_empty_ranges() {
        let edit = create_edit(10, 15, 20);
        let ranges = vec![];

        let transformed = transform_edit_for_injection(&edit, &ranges);
        assert!(transformed.is_some());
        assert_eq!(transformed.unwrap().start_byte, 10);
    }

    #[test]
    fn test_adjust_ranges_replace_text() {
        let mut ranges = vec![(10, 20), (30, 40)];
        let edit = create_edit(15, 18, 25); // Replace 3 bytes with 10 bytes

        adjust_ranges_for_edit(&mut ranges, &edit);

        assert_eq!(ranges, vec![(10, 27), (37, 47)]);
    }

    #[test]
    fn test_edit_affects_ranges_boundary_cases() {
        let ranges = vec![(10, 20)];

        // Edit exactly at start boundary
        assert!(edit_affects_ranges(&create_edit(10, 15, 20), &ranges));

        // Edit exactly at end boundary (should not affect)
        assert!(!edit_affects_ranges(&create_edit(20, 25, 30), &ranges));

        // Edit touching both boundaries
        assert!(edit_affects_ranges(&create_edit(10, 20, 25), &ranges));
    }
}
