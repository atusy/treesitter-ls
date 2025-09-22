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
pub fn calculate_effective_range_with_text(
    text: &str,
    byte_range: ByteRange,
    offset: InjectionOffset,
) -> EffectiveRange {
    // If no row offsets, just apply column offsets
    if offset.start_row == 0 && offset.end_row == 0 {
        return calculate_effective_range(byte_range, offset);
    }

    // Calculate line positions for proper row offset application
    let effective_start = apply_offset_to_position(
        text,
        byte_range.start,
        offset.start_row,
        offset.start_column,
    );
    let effective_end =
        apply_offset_to_position(text, byte_range.end, offset.end_row, offset.end_column);

    EffectiveRange::new(effective_start, effective_end)
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

        // Should move start to byte 14 (start of "line 3")
        assert_eq!(effective.start, 14);
        assert_eq!(effective.end, 13); // End unchanged (no row offset for end)
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

        // Should move to byte 14 (start of "line 3") + 5 = 19
        assert_eq!(effective.start, 19);
        assert_eq!(effective.end, 13);
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
