/// Represents offset adjustments for injection boundaries
/// Format: (start_row, start_col, end_row, end_col)
pub type InjectionOffset = (i32, i32, i32, i32);

/// Default offset with no adjustments
pub const DEFAULT_OFFSET: InjectionOffset = (0, 0, 0, 0);

/// Represents an injection capture with optional offset adjustments
#[derive(Debug, Clone, PartialEq)]
pub struct InjectionCapture {
    pub language: String,
    pub content_range: std::ops::Range<usize>,
    pub offset: InjectionOffset,
}

impl InjectionCapture {
    pub fn new(language: String, content_range: std::ops::Range<usize>) -> Self {
        Self {
            language,
            content_range,
            offset: DEFAULT_OFFSET,
        }
    }

    /// Check if a byte position is within the adjusted injection boundaries
    pub fn contains_position(&self, byte_pos: usize) -> bool {
        let adjusted_start = apply_offset(self.content_range.start, self.offset.1);
        let adjusted_end = apply_offset(self.content_range.end, self.offset.3);
        byte_pos >= adjusted_start && byte_pos < adjusted_end
    }

    /// Get the adjusted content range after applying offsets
    pub fn adjusted_range(&self) -> std::ops::Range<usize> {
        let adjusted_start = apply_offset(self.content_range.start, self.offset.1);
        let adjusted_end = apply_offset(self.content_range.end, self.offset.3);
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
    fn test_contains_position_with_offset() {
        let mut capture = InjectionCapture::new("luadoc".to_string(), 0..18);
        capture.offset = (0, 1, 0, 0); // lua->luadoc offset

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
