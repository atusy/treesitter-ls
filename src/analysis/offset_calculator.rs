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
/// Currently only applies column offsets (start_column and end_column).
/// Row offsets (start_row and end_row) would require line calculations.
pub fn calculate_effective_range(byte_range: ByteRange, offset: InjectionOffset) -> EffectiveRange {
    // For now, only apply column offsets (row offsets would need line calculations)
    let effective_start = (byte_range.start as i32 + offset.start_column) as usize;
    let effective_end = (byte_range.end as i32 + offset.end_column) as usize;

    EffectiveRange::new(effective_start, effective_end)
}

/// Formats an offset as a string representation
pub fn format_offset(offset: InjectionOffset) -> String {
    format!("({}, {}, {}, {})", offset.start_row, offset.start_column, offset.end_row, offset.end_column)
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
}