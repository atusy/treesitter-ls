use tower_lsp::lsp_types::{Position, Range};

/// Trait for mapping between LSP positions and byte offsets
/// Supports both simple text mapping and complex injection scenarios
pub trait PositionMapper {
    /// Convert LSP Position to byte offset in the document
    fn position_to_byte(&self, position: Position) -> Option<usize>;

    /// Convert byte offset to LSP Position
    fn byte_to_position(&self, offset: usize) -> Option<Position>;

    /// Convert byte range to LSP Range
    fn byte_range_to_range(&self, start: usize, end: usize) -> Option<Range> {
        let start_pos = self.byte_to_position(start)?;
        let end_pos = self.byte_to_position(end)?;
        Some(Range {
            start: start_pos,
            end: end_pos,
        })
    }
}

/// Simple position mapper for single-language documents
/// This implements the current position mapping logic
pub struct SimplePositionMapper<'a> {
    text: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> SimplePositionMapper<'a> {
    /// Create a new SimplePositionMapper with pre-computed line starts
    pub fn new(text: &'a str) -> Self {
        let line_starts = compute_line_starts(text);
        Self { text, line_starts }
    }

    /// Get the byte offset of a line start
    fn get_line_start(&self, line: usize) -> Option<usize> {
        self.line_starts.get(line).copied()
    }
}

impl<'a> PositionMapper for SimplePositionMapper<'a> {
    fn position_to_byte(&self, position: Position) -> Option<usize> {
        let line = position.line as usize;
        let character = position.character as usize;

        // Get the start of the target line
        let line_start = self.get_line_start(line)?;

        // Find the end of the line (or end of text)
        let line_end = if line + 1 < self.line_starts.len() {
            self.line_starts[line + 1] - 1 // Exclude the newline
        } else {
            self.text.len()
        };

        // Get the line text
        let line_text = &self.text[line_start..line_end];

        // Use the common utility function for UTF-16 to byte conversion
        match convert_utf16_to_byte_in_line(line_text, character) {
            Some(byte_offset) => Some(line_start + byte_offset),
            None => {
                // If position is beyond line end, return the line end
                Some(line_start + line_text.len())
            }
        }
    }

    fn byte_to_position(&self, offset: usize) -> Option<Position> {
        // Binary search for the line containing this offset
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };

        let line_start = self.get_line_start(line)?;

        // Calculate the UTF-16 character offset within the line
        let line_offset = offset.saturating_sub(line_start);
        let line_end = if line + 1 < self.line_starts.len() {
            self.line_starts[line + 1] - 1
        } else {
            self.text.len()
        };

        let line_text = &self.text[line_start..line_end.min(self.text.len())];

        // Use the common utility function for byte to UTF-16 conversion
        let character = match convert_byte_to_utf16_in_line(line_text, line_offset) {
            Some(utf16_offset) => utf16_offset,
            None => {
                // If we're in the middle of a character, find the character start
                let mut valid_offset = line_offset;
                while valid_offset > 0 {
                    valid_offset -= 1;
                    if let Some(utf16) = convert_byte_to_utf16_in_line(line_text, valid_offset) {
                        return Some(Position {
                            line: line as u32,
                            character: utf16 as u32,
                        });
                    }
                }
                // Fallback to start of line
                0
            }
        };

        Some(Position {
            line: line as u32,
            character: character as u32,
        })
    }
}

/// Compute line start offsets for efficient position mapping
pub fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut line_starts = vec![0];
    let mut offset = 0;

    for ch in text.chars() {
        offset += ch.len_utf8();
        if ch == '\n' {
            line_starts.push(offset);
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

/// Extract text from document for given ranges
pub fn extract_text_from_ranges(document_text: &str, ranges: &[(usize, usize)]) -> String {
    let mut result = String::new();

    for (start, end) in ranges {
        if *start < document_text.len() && *end <= document_text.len() {
            result.push_str(&document_text[*start..*end]);
        }
    }

    result
}
