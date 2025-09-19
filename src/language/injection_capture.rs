/// Represents offset adjustments for injection boundaries in row/column positions
/// Format: (start_row_offset, start_column_offset, end_row_offset, end_column_offset)
///
/// Examples:
/// - (0, 1, 0, -1): Skip first column at start, exclude last column at end (common for quotes)
/// - (1, 0, -1, 0): Skip first row, exclude last row (markdown frontmatter)
/// - (0, 1, 0, 0): Skip first column only (lua comment injection)
///
/// Note: These are ROW/COLUMN offsets, not byte offsets!
pub type InjectionOffset = (i32, i32, i32, i32);

/// Default offset with no adjustments
pub const DEFAULT_OFFSET: InjectionOffset = (0, 0, 0, 0);

/// Represents an injection capture with optional offset adjustments
#[derive(Debug, Clone, PartialEq)]
pub struct InjectionCapture {
    pub language: String,
    pub content_range: std::ops::Range<usize>,
    pub offset: InjectionOffset,
    /// Optional text for proper row/column offset calculation
    pub text: Option<String>,
}

impl InjectionCapture {
    pub fn new(language: String, content_range: std::ops::Range<usize>) -> Self {
        Self {
            language,
            content_range,
            offset: DEFAULT_OFFSET,
            text: None,
        }
    }

    /// Check if a byte position is within the adjusted injection boundaries
    pub fn contains_position(&self, byte_pos: usize) -> bool {
        // Fallback to byte-based offset for backwards compatibility
        let adjusted_start = apply_offset(self.content_range.start, self.offset.1);
        let adjusted_end = apply_offset(self.content_range.end, self.offset.3);
        byte_pos >= adjusted_start && byte_pos < adjusted_end
    }

    /// Check if a byte position is within the adjusted injection boundaries using proper row/column offsets
    pub fn contains_position_with_text(
        &self,
        byte_pos: usize,
        mapper: &crate::text::PositionMapper,
    ) -> bool {
        // Apply offsets in row/column space
        let adjusted_range = self.adjusted_range_with_text(mapper);
        byte_pos >= adjusted_range.start && byte_pos < adjusted_range.end
    }

    /// Get the adjusted content range after applying offsets
    pub fn adjusted_range(&self) -> std::ops::Range<usize> {
        // Fallback to byte-based offset for backwards compatibility
        let adjusted_start = apply_offset(self.content_range.start, self.offset.1);
        let adjusted_end = apply_offset(self.content_range.end, self.offset.3);
        adjusted_start..adjusted_end
    }

    /// Get the adjusted content range after applying row/column offsets
    pub fn adjusted_range_with_text(
        &self,
        mapper: &crate::text::PositionMapper,
    ) -> std::ops::Range<usize> {
        use tower_lsp::lsp_types::Position;

        // Convert start byte to position
        let start_pos = mapper
            .byte_to_position(self.content_range.start)
            .unwrap_or(Position::new(0, 0));

        // Apply row/column offsets to start
        let adjusted_start_pos = Position::new(
            (start_pos.line as i32 + self.offset.0).max(0) as u32,
            (start_pos.character as i32 + self.offset.1).max(0) as u32,
        );

        // Convert end byte to position
        let end_pos = mapper
            .byte_to_position(self.content_range.end)
            .unwrap_or(Position::new(0, 0));

        // Apply row/column offsets to end
        let adjusted_end_pos = Position::new(
            (end_pos.line as i32 + self.offset.2).max(0) as u32,
            (end_pos.character as i32 + self.offset.3).max(0) as u32,
        );

        // Convert adjusted positions back to bytes
        let adjusted_start = mapper
            .position_to_byte(adjusted_start_pos)
            .unwrap_or(self.content_range.start);
        let adjusted_end = mapper
            .position_to_byte(adjusted_end_pos)
            .unwrap_or(self.content_range.end);

        adjusted_start..adjusted_end
    }
}

fn apply_offset(byte_pos: usize, offset: i32) -> usize {
    if offset >= 0 {
        byte_pos + offset as usize
    } else {
        byte_pos.saturating_sub((-offset) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_capture_has_offset_field() {
        let capture = InjectionCapture::new("lua".to_string(), 10..20);

        assert_eq!(
            capture.offset, DEFAULT_OFFSET,
            "offset should default to DEFAULT_OFFSET"
        );
    }

    #[test]
    fn test_offset_with_row_column_positions() {
        use crate::text::PositionMapper;

        // Test text with multiple lines
        let text = "---@param x number\nlocal x = 5\n";
        let mapper = PositionMapper::new(text);

        // Create capture for first line (the comment)
        let mut capture = InjectionCapture::new("luadoc".to_string(), 0..18);
        capture.offset = (0, 1, 0, 0); // Skip first column

        // The text should be available to apply offset correctly
        capture.text = Some(text.to_string());

        // Test that offset is applied in row/column space
        // Position at byte 0 (first hyphen) should be OUTSIDE after offset
        assert!(
            !capture.contains_position_with_text(0, &mapper),
            "First hyphen should be outside after column offset"
        );

        // Position at byte 1 (second hyphen) should be INSIDE after offset
        assert!(
            capture.contains_position_with_text(1, &mapper),
            "Second hyphen should be inside after column offset"
        );

        // Position at byte 2 (third hyphen) should be INSIDE after offset
        assert!(
            capture.contains_position_with_text(2, &mapper),
            "Third hyphen should be inside after column offset"
        );
    }

    #[test]
    fn test_contains_position_with_offset() {
        let mut capture = InjectionCapture::new("luadoc".to_string(), 0..18);
        capture.offset = (0, 1, 0, 0); // lua->luadoc offset

        // WITHOUT text/mapper, falls back to byte-based (incorrect but backwards compatible)
        // Position 0: should NOT be in injection (offset moves start to 1)
        assert!(
            !capture.contains_position(0),
            "Position 0 should be outside adjusted range"
        );

        // Position 1: should be at the start of adjusted injection
        assert!(
            capture.contains_position(1),
            "Position 1 should be at start of adjusted range"
        );

        // Position 17: should be in injection
        assert!(
            capture.contains_position(17),
            "Position 17 should be in adjusted range"
        );

        // Position 18: should NOT be in injection (exclusive end)
        assert!(
            !capture.contains_position(18),
            "Position 18 should be outside adjusted range"
        );
    }

    #[test]
    fn test_adjusted_range() {
        let mut capture = InjectionCapture::new("markdown".to_string(), 3..20);
        capture.offset = (1, 0, -1, 0); // markdown frontmatter offset

        let adjusted = capture.adjusted_range();
        assert_eq!(
            adjusted.start, 3,
            "Start should remain unchanged with row offset"
        );
        assert_eq!(
            adjusted.end, 20,
            "End should remain unchanged with row offset"
        );

        // Test with column offsets
        capture.offset = (0, 2, 0, -3);
        let adjusted = capture.adjusted_range();
        assert_eq!(adjusted.start, 5, "Start should be adjusted by +2");
        assert_eq!(adjusted.end, 17, "End should be adjusted by -3");
    }
}
