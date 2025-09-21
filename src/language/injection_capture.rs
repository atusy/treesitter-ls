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
}
