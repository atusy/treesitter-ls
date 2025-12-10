use crate::language::injection::InjectionOffset;

/// Represents a byte range with start and end positions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Represents an effective range after applying offset
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveRange {
    pub start: usize,
    pub end: usize,
}

impl EffectiveRange {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Calculates the effective range by applying offset to a byte range
///
/// This version only applies column offsets. For row offsets, use calculate_effective_range_with_text.
pub fn calculate_effective_range(byte_range: ByteRange, offset: InjectionOffset) -> EffectiveRange {
    // Only apply column offsets (row offsets need text for line calculations)
    let effective_start = (byte_range.start as i32 + offset.start_column) as usize;
    let effective_end = (byte_range.end as i32 + offset.end_column) as usize;

    EffectiveRange::new(effective_start, effective_end)
}

/// Calculates the effective range by applying both row and column offsets
///
/// This function clamps the resulting range to `[0, text.len()]` and ensures
/// `start <= end` to prevent panics when slicing. Malformed or malicious
/// query offsets cannot crash the server.
pub fn calculate_effective_range_with_text(
    text: &str,
    byte_range: ByteRange,
    offset: InjectionOffset,
) -> EffectiveRange {
    let text_len = text.len();

    // Calculate raw effective positions
    let (raw_start, raw_end) = if offset.start_row == 0 && offset.end_row == 0 {
        // Column offsets only - apply directly
        let start = (byte_range.start as i32 + offset.start_column).max(0) as usize;
        let end = (byte_range.end as i32 + offset.end_column).max(0) as usize;
        (start, end)
    } else {
        // Row offsets require text scanning
        let start = apply_offset_to_position(
            text,
            byte_range.start,
            offset.start_row,
            offset.start_column,
        );
        let end = apply_offset_to_position(text, byte_range.end, offset.end_row, offset.end_column);
        (start, end)
    };

    // Clamp to valid range [0, text.len()]
    let clamped_start = raw_start.min(text_len);
    let clamped_end = raw_end.min(text_len);

    // Ensure start <= end invariant
    let (final_start, final_end) = if clamped_start <= clamped_end {
        (clamped_start, clamped_end)
    } else {
        // When start > end, return empty range at clamped_end
        (clamped_end, clamped_end)
    };

    EffectiveRange::new(final_start, final_end)
}

/// Apply row and column offset to a byte position in text
fn apply_offset_to_position(
    text: &str,
    byte_pos: usize,
    row_offset: i32,
    col_offset: i32,
) -> usize {
    if row_offset == 0 {
        // No row offset, just apply column offset
        return (byte_pos as i32 + col_offset).max(0) as usize;
    }

    // Find the line containing the byte position
    let mut current_line = 0;
    let mut line_start_byte = 0;

    for (i, ch) in text.char_indices() {
        if i >= byte_pos {
            // Found the line containing our position
            break;
        }
        if ch == '\n' {
            current_line += 1;
            line_start_byte = i + 1;
        }
    }

    // Now find the target line (current_line + row_offset)
    let target_line = (current_line as i32 + row_offset).max(0) as usize;

    if row_offset > 0 {
        // Moving forward - find the target line
        let mut lines_seen = current_line;
        let mut target_line_start = line_start_byte;

        for (i, ch) in text[line_start_byte..].char_indices() {
            if lines_seen == target_line {
                break;
            }
            if ch == '\n' {
                lines_seen += 1;
                target_line_start = line_start_byte + i + 1;
            }
        }

        // Apply column offset from the start of the target line
        (target_line_start as i32 + col_offset).max(0) as usize
    } else {
        // Moving backward - count lines from the beginning
        let mut lines_seen = 0;
        let mut target_line_start = 0;

        for (i, ch) in text.char_indices() {
            if lines_seen == target_line {
                target_line_start = i;
                break;
            }
            if ch == '\n' {
                lines_seen += 1;
            }
        }

        // Apply column offset from the start of the target line
        (target_line_start as i32 + col_offset).max(0) as usize
    }
}

/// Formats an offset as a string representation
pub fn format_offset(offset: InjectionOffset) -> String {
    format!(
        "({}, {}, {}, {})",
        offset.start_row, offset.start_column, offset.end_row, offset.end_column
    )
}

/// Determines the offset label based on whether it came from a query
pub fn get_offset_label(from_query: bool) -> &'static str {
    if from_query {
        "[from query]"
    } else {
        "[default]"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::injection::DEFAULT_OFFSET;

    #[test]
    fn test_offset_extending_past_eof_should_be_clamped() {
        // This test verifies that offsets extending past EOF don't cause panics
        // A malformed query might specify #offset! @injection.content 0 0 0 1
        // which extends the end past the buffer
        let text = "hello";
        let byte_range = ByteRange::new(0, 5); // Entire text
        let offset = InjectionOffset::new(0, 0, 0, 5); // End column +5 (past EOF)

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        // End should be clamped to text.len() = 5
        assert!(
            effective.end <= text.len(),
            "End {} should be clamped to text len {}",
            effective.end,
            text.len()
        );
        // start <= end invariant should hold
        assert!(
            effective.start <= effective.end,
            "Start {} should be <= end {}",
            effective.start,
            effective.end
        );
        // Slicing should not panic
        let _ = &text[effective.start..effective.end];
    }

    #[test]
    fn test_offset_with_start_past_end_should_be_normalized() {
        // Edge case: offset makes start > end
        let text = "hello";
        let byte_range = ByteRange::new(0, 5);
        let offset = InjectionOffset::new(0, 10, 0, 0); // Start offset +10, end +0

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        // start <= end invariant should hold
        assert!(
            effective.start <= effective.end,
            "Start {} should be <= end {}",
            effective.start,
            effective.end
        );
        // Both should be clamped to text.len()
        assert!(
            effective.start <= text.len() && effective.end <= text.len(),
            "Both start {} and end {} should be <= text len {}",
            effective.start,
            effective.end,
            text.len()
        );
    }

    #[test]
    fn test_row_offset_past_eof_should_be_clamped() {
        // Row offset moving end beyond last line
        let text = "line 1\nline 2";
        let byte_range = ByteRange::new(0, 6); // "line 1"
        let offset = InjectionOffset::new(0, 0, 5, 0); // End row +5 (way past last line)

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        assert!(
            effective.end <= text.len(),
            "End {} should be clamped to text len {}",
            effective.end,
            text.len()
        );
        // Slicing should not panic
        let _ = &text[effective.start..effective.end];
    }

    #[test]
    fn test_calculate_effective_range_with_positive_offset() {
        let byte_range = ByteRange::new(10, 20);
        let offset = InjectionOffset::new(0, 5, 0, -3); // Column offsets: +5 start, -3 end

        let effective = calculate_effective_range(byte_range, offset);

        assert_eq!(effective.start, 15);
        assert_eq!(effective.end, 17);
    }

    #[test]
    fn test_calculate_effective_range_with_default_offset() {
        let byte_range = ByteRange::new(10, 20);

        let effective = calculate_effective_range(byte_range, DEFAULT_OFFSET);

        assert_eq!(effective.start, 10);
        assert_eq!(effective.end, 20);
    }

    #[test]
    fn test_format_offset() {
        let offset = InjectionOffset::new(1, 2, 3, 4);
        assert_eq!(format_offset(offset), "(1, 2, 3, 4)");
    }

    #[test]
    fn test_get_offset_label() {
        assert_eq!(get_offset_label(true), "[from query]");
        assert_eq!(get_offset_label(false), "[default]");
    }

    #[test]
    fn test_calculate_effective_range_with_text_column_only() {
        let text = "line 1\nline 2\nline 3";
        let byte_range = ByteRange::new(7, 13); // "line 2"
        let offset = InjectionOffset::new(0, 3, 0, -1); // Column offsets only

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        assert_eq!(effective.start, 10); // 7 + 3
        assert_eq!(effective.end, 12); // 13 - 1
    }

    #[test]
    fn test_calculate_effective_range_with_text_positive_row_offset() {
        let text = "line 1\nline 2\nline 3 with content\nline 4";
        // Node starts at byte 7 (start of "line 2")
        let byte_range = ByteRange::new(7, 13); // "line 2"
        let offset = InjectionOffset::new(1, 0, 0, 0); // Move start down 1 row

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        // Original raw values: start=14, end=13 (start > end)
        // With safety clamping: when start > end, we normalize to empty range at end
        assert_eq!(effective.start, 13);
        assert_eq!(effective.end, 13);
        // Slicing should be safe
        let _ = &text[effective.start..effective.end];
    }

    #[test]
    fn test_calculate_effective_range_with_text_negative_row_offset() {
        let text = "line 1\nline 2\nline 3";
        // Node starts at byte 14 (start of "line 3")
        let byte_range = ByteRange::new(14, 20);
        let offset = InjectionOffset::new(-1, 0, 0, 0); // Move start up 1 row

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        // Should move start to byte 7 (start of "line 2")
        assert_eq!(effective.start, 7);
        assert_eq!(effective.end, 20); // End unchanged
    }

    #[test]
    fn test_calculate_effective_range_with_text_row_and_column_offset() {
        let text = "line 1\nline 2\nline 3 with content";
        // Node starts at byte 7 (start of "line 2")
        let byte_range = ByteRange::new(7, 13);
        // Move start down 1 row and 5 columns right
        let offset = InjectionOffset::new(1, 5, 0, 0);

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        // Original raw values: start=19, end=13 (start > end)
        // With safety clamping: when start > end, we normalize to empty range at end
        assert_eq!(effective.start, 13);
        assert_eq!(effective.end, 13);
        // Slicing should be safe
        let _ = &text[effective.start..effective.end];
    }

    #[test]
    fn test_lua_doc_comment_offset() {
        // Real-world example: Lua doc comment
        // ---@param x number
        // The offset (0, 3, 0, 0) means start at column 3 of the same row
        let text = "---@param x number\nfunction foo(x) end";
        let byte_range = ByteRange::new(0, 18); // The doc comment
        let offset = InjectionOffset::new(0, 3, 0, 0); // Skip "---"

        let effective = calculate_effective_range_with_text(text, byte_range, offset);

        assert_eq!(effective.start, 3); // Skip "---" prefix
        assert_eq!(effective.end, 18);
    }
}

#[test]
fn test_markdown_frontmatter_offset() {
    // Real-world example: Markdown YAML frontmatter
    // Offset (#offset! @injection.content 1 0 -1 0)
    // means skip first row (---) and last row (---)
    let text = "---\ntitle: \"awesome\"\narray: [\"xxxx\"]\n---\n";

    // The minus_metadata node spans the entire frontmatter including ---
    let byte_range = ByteRange::new(0, 41); // "---\ntitle...\n---\n"

    // Offset: start_row=1, start_col=0, end_row=-1, end_col=0
    let offset = InjectionOffset::new(1, 0, -1, 0);

    let effective = calculate_effective_range_with_text(text, byte_range, offset);

    // Start should be at byte 4 (after "---\n")
    assert_eq!(effective.start, 4, "Start should skip the first line");

    // End should be at byte 37 (before "---\n")
    // Line positions:
    // byte 0: "---\n" (4 bytes)
    // byte 4: "title: \"awesome\"\n" (18 bytes) = ends at 21
    // byte 21+1: "array: [\"xxxx\"]\n" (16 bytes) = ends at 37
    // byte 37: "---\n" (4 bytes) = ends at 41
    assert_eq!(effective.end, 37, "End should skip the last line");

    // Verify the content matches expected
    let effective_text = &text[effective.start..effective.end];
    assert_eq!(effective_text, "title: \"awesome\"\narray: [\"xxxx\"]\n");
}
