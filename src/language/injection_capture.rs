/// Represents an injection capture with optional offset adjustments
#[derive(Debug, Clone, PartialEq)]
pub struct InjectionCapture {
    pub language: String,
    pub content_range: std::ops::Range<usize>,
    pub offset: (i32, i32, i32, i32),
}

impl InjectionCapture {
    pub fn new(language: String, content_range: std::ops::Range<usize>) -> Self {
        Self {
            language,
            content_range,
            offset: (0, 0, 0, 0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_injection_capture_has_offset_field() {
        let capture = InjectionCapture::new("lua".to_string(), 10..20);

        // This test should fail initially as offset field doesn't exist yet
        assert_eq!(
            capture.offset,
            (0, 0, 0, 0),
            "offset should default to (0, 0, 0, 0)"
        );
    }
}
