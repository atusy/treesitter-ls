use line_index::{LineIndex, WideEncoding, WideLineCol};
use tower_lsp::lsp_types::{Position, Range};

/// Position mapper for converting between LSP positions and byte offsets
pub struct SimplePositionMapper<'a> {
    _text: &'a str,
    line_index: LineIndex,
}

impl<'a> SimplePositionMapper<'a> {
    /// Create a new SimplePositionMapper with pre-computed line starts
    pub fn new(text: &'a str) -> Self {
        let line_index = LineIndex::new(text);
        Self {
            _text: text,
            line_index,
        }
    }
}

impl<'a> SimplePositionMapper<'a> {
    /// Convert LSP Position to byte offset in the document
    pub fn position_to_byte(&self, position: Position) -> Option<usize> {
        // LSP positions are UTF-16 based
        let wide_line_col = WideLineCol {
            line: position.line,
            col: position.character,
        };

        // Convert from UTF-16 position to byte offset
        let line_col = self
            .line_index
            .to_utf8(WideEncoding::Utf16, wide_line_col)?;
        let text_size = self.line_index.offset(line_col)?;
        Some(text_size.into())
    }

    /// Convert byte offset to LSP Position
    pub fn byte_to_position(&self, offset: usize) -> Option<Position> {
        // Convert byte offset to LineCol
        let line_col = self.line_index.try_line_col(offset.try_into().ok()?)?;

        // Convert to UTF-16 position for LSP
        let wide_line_col = self.line_index.to_wide(WideEncoding::Utf16, line_col)?;

        Some(Position::new(wide_line_col.line, wide_line_col.col))
    }

    /// Convert byte range to LSP Range
    pub fn byte_range_to_range(&self, start: usize, end: usize) -> Option<Range> {
        let start_pos = self.byte_to_position(start)?;
        let end_pos = self.byte_to_position(end)?;
        Some(Range::new(start_pos, end_pos))
    }
}

/// Compute line start offsets for efficient position mapping
/// This function is kept for backwards compatibility
pub fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut line_starts = vec![0];

    // Iterate through each line and find its starting position
    let mut current_pos = 0;
    for ch in text.chars() {
        current_pos += ch.len_utf8();
        if ch == '\n' {
            line_starts.push(current_pos);
        }
    }

    line_starts
}

/// Convert UTF-16 position to byte position within a line
/// Returns None if the UTF-16 position is invalid
#[inline(always)]
pub fn convert_utf16_to_byte_in_line(line_text: &str, utf16_pos: usize) -> Option<usize> {
    let mut byte_offset = 0;
    let mut utf16_offset = 0;

    for ch in line_text.chars() {
        if utf16_offset >= utf16_pos {
            return Some(byte_offset);
        }
        utf16_offset += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }

    // If we reached the end and the position matches exactly, return the end position
    if utf16_offset == utf16_pos {
        Some(byte_offset)
    } else {
        // Position is beyond the end of the line
        None
    }
}

/// Convert byte position to UTF-16 position within a line
/// Returns None if the byte position is invalid (e.g., in the middle of a multi-byte character)
#[inline(always)]
pub fn convert_byte_to_utf16_in_line(line_text: &str, byte_pos: usize) -> Option<usize> {
    let mut utf16_offset = 0;
    let mut byte_count = 0;

    for ch in line_text.chars() {
        if byte_count == byte_pos {
            return Some(utf16_offset);
        }
        let ch_bytes = ch.len_utf8();
        if byte_count + ch_bytes > byte_pos {
            // Position is in the middle of a multi-byte character
            return None;
        }
        byte_count += ch_bytes;
        utf16_offset += ch.len_utf16();
    }

    // If we reached the end and the position matches exactly, return the end position
    if byte_count == byte_pos {
        Some(utf16_offset)
    } else {
        // Position is beyond the end of the line
        None
    }
}
