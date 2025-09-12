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
            self.line_starts[line + 1] - 1  // Exclude the newline
        } else {
            self.text.len()
        };
        
        // Get the line text
        let line_text = &self.text[line_start..line_end];
        
        // Convert UTF-16 character offset to byte offset within the line
        let mut byte_offset = 0;
        let mut utf16_offset = 0;
        
        for ch in line_text.chars() {
            if utf16_offset >= character {
                return Some(line_start + byte_offset);
            }
            utf16_offset += ch.len_utf16();
            byte_offset += ch.len_utf8();
        }
        
        // If we're past the end of the line, return the line end
        Some(line_start + byte_offset.min(line_text.len()))
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
        
        // Convert byte offset to UTF-16 character offset
        let mut utf16_offset = 0;
        let mut byte_count = 0;
        
        for ch in line_text.chars() {
            if byte_count >= line_offset {
                break;
            }
            let ch_bytes = ch.len_utf8();
            if byte_count + ch_bytes > line_offset {
                // We're in the middle of this character
                break;
            }
            byte_count += ch_bytes;
            utf16_offset += ch.len_utf16();
        }
        
        Some(Position {
            line: line as u32,
            character: utf16_offset as u32,
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_mapper_basic() {
        let text = "hello\nworld\n";
        let mapper = SimplePositionMapper::new(text);
        
        // First line
        let pos = Position { line: 0, character: 0 };
        assert_eq!(mapper.position_to_byte(pos), Some(0));
        
        let pos = Position { line: 0, character: 5 };
        assert_eq!(mapper.position_to_byte(pos), Some(5));
        
        // Second line
        let pos = Position { line: 1, character: 0 };
        assert_eq!(mapper.position_to_byte(pos), Some(6));
        
        let pos = Position { line: 1, character: 5 };
        assert_eq!(mapper.position_to_byte(pos), Some(11));
    }
    
    #[test]
    fn test_simple_mapper_utf8() {
        let text = "hello\n世界\n";
        let mapper = SimplePositionMapper::new(text);
        
        // Japanese characters: each is 3 bytes in UTF-8, 1 code unit in UTF-16
        let pos = Position { line: 1, character: 0 };
        assert_eq!(mapper.position_to_byte(pos), Some(6));
        
        let pos = Position { line: 1, character: 1 };
        assert_eq!(mapper.position_to_byte(pos), Some(9)); // After "世"
        
        let pos = Position { line: 1, character: 2 };
        assert_eq!(mapper.position_to_byte(pos), Some(12)); // After "界"
    }
    
    #[test]
    fn test_simple_mapper_utf16_emoji() {
        let text = "hello 👋 world";
        let mapper = SimplePositionMapper::new(text);
        
        // "hello " = 6 bytes, 6 UTF-16 units
        let pos = Position { line: 0, character: 6 };
        assert_eq!(mapper.position_to_byte(pos), Some(6));
        
        // "👋" = 4 bytes, 2 UTF-16 units
        let pos = Position { line: 0, character: 8 };
        assert_eq!(mapper.position_to_byte(pos), Some(10)); // After emoji
        
        // " world" starts after emoji
        let pos = Position { line: 0, character: 9 };
        assert_eq!(mapper.position_to_byte(pos), Some(11));
    }
    
    #[test]
    fn test_byte_to_position_basic() {
        let text = "hello\nworld\n";
        let mapper = SimplePositionMapper::new(text);
        
        // Start of first line
        let pos = mapper.byte_to_position(0).unwrap();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
        
        // End of "hello"
        let pos = mapper.byte_to_position(5).unwrap();
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);
        
        // Start of second line
        let pos = mapper.byte_to_position(6).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
        
        // Middle of "world"
        let pos = mapper.byte_to_position(8).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 2);
    }
    
    #[test]
    fn test_byte_to_position_utf8() {
        let text = "hello\n世界";
        let mapper = SimplePositionMapper::new(text);
        
        // Start of Japanese text
        let pos = mapper.byte_to_position(6).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
        
        // After first Japanese character (3 bytes)
        let pos = mapper.byte_to_position(9).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 1);
        
        // After second Japanese character
        let pos = mapper.byte_to_position(12).unwrap();
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 2);
    }
    
    #[test]
    fn test_byte_range_to_range() {
        let text = "hello\nworld";
        let mapper = SimplePositionMapper::new(text);
        
        let range = mapper.byte_range_to_range(0, 5).unwrap();
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 5);
        
        let range = mapper.byte_range_to_range(6, 11).unwrap();
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 5);
    }
    
    #[test]
    fn test_line_starts_computation() {
        let text = "line1\nline2\nline3";
        let line_starts = compute_line_starts(text);
        
        assert_eq!(line_starts, vec![0, 6, 12]);
        
        let text = "single line";
        let line_starts = compute_line_starts(text);
        assert_eq!(line_starts, vec![0]);
        
        let text = "";
        let line_starts = compute_line_starts(text);
        assert_eq!(line_starts, vec![0]);
    }
}