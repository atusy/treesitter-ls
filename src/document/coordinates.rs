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

/// Map document byte offset to layer byte offset for injection ranges
pub fn doc_to_layer_offset(doc_offset: usize, ranges: &[(usize, usize)]) -> Option<usize> {
    let mut layer_offset = 0;

    for (start, end) in ranges {
        if doc_offset >= *start && doc_offset < *end {
            // Found the range containing the offset
            let offset_in_range = doc_offset - start;
            return Some(layer_offset + offset_in_range);
        }

        // If we haven't found it yet, accumulate the layer offset
        if doc_offset >= *end {
            layer_offset += end - start;
        }
    }

    None
}

/// Map layer byte offset to document byte offset for injection ranges
pub fn layer_to_doc_offset(layer_offset: usize, ranges: &[(usize, usize)]) -> Option<usize> {
    let mut accumulated_length = 0;

    for (start, end) in ranges {
        let range_length = end - start;

        if layer_offset < accumulated_length + range_length {
            // This offset falls within this range
            let offset_in_range = layer_offset - accumulated_length;
            return Some(start + offset_in_range);
        }

        accumulated_length += range_length;
    }

    None
}

/// Check if a document byte offset falls within injection ranges
pub fn contains_offset(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(start, end)| offset >= *start && offset < *end)
}

/// Position mapper that handles multiple injection layers
/// This handles the mapping between document coordinates and injection layer coordinates
pub struct InjectionPositionMapper<'a> {
    document_text: &'a str,
    document_line_starts: Vec<usize>,
    /// Layer identifiers for lookups
    layer_ids: Vec<&'a str>,
    /// Ranges for each layer (for position calculations)
    layer_ranges: Vec<&'a [(usize, usize)]>,
    /// Extracted text for each layer (owned to ensure lifetime)
    layer_texts: Vec<String>,
}

impl<'a> InjectionPositionMapper<'a> {
    /// Create a new injection position mapper from language layers
    pub fn new(text: &'a str, layers: &'a [crate::document::LanguageLayer]) -> Self {
        let document_line_starts = compute_line_starts(text);

        let mut layer_ids = Vec::new();
        let mut layer_ranges = Vec::new();
        let mut layer_texts = Vec::new();

        // Extract text and metadata from each layer
        for layer in layers {
            let extracted = extract_text_from_ranges(text, &layer.ranges);
            layer_texts.push(extracted);
            layer_ids.push(layer.language_id.as_str());
            layer_ranges.push(&layer.ranges as &[(usize, usize)]);
        }

        Self {
            document_text: text,
            document_line_starts,
            layer_ids,
            layer_ranges,
            layer_texts,
        }
    }

    /// Find the layer containing the given document position
    pub fn get_layer_at_position(&self, position: Position) -> Option<&str> {
        let byte_offset = self.position_to_byte(position)?;

        for (i, ranges) in self.layer_ranges.iter().enumerate() {
            if contains_offset(byte_offset, ranges) {
                return Some(self.layer_ids[i]);
            }
        }

        None
    }

    /// Get the layer index by its identifier
    pub fn get_layer_index(&self, layer_id: &str) -> Option<usize> {
        self.layer_ids
            .iter()
            .position(|id| *id == layer_id)
    }

    /// Map document position to layer position for a specific layer
    pub fn doc_position_to_layer_position(
        &self,
        position: Position,
        layer_id: &str,
    ) -> Option<Position> {
        let layer_idx = self.get_layer_index(layer_id)?;
        let doc_byte = self.position_to_byte(position)?;
        let layer_byte = doc_to_layer_offset(doc_byte, self.layer_ranges[layer_idx])?;

        // Convert layer byte to position using the layer's extracted text
        let mapper = SimplePositionMapper::new(&self.layer_texts[layer_idx]);
        mapper.byte_to_position(layer_byte)
    }

    /// Map layer position to document position for a specific layer
    pub fn layer_position_to_doc_position(
        &self,
        position: Position,
        layer_id: &str,
    ) -> Option<Position> {
        let layer_idx = self.get_layer_index(layer_id)?;

        // Convert position to byte in layer text
        let mapper = SimplePositionMapper::new(&self.layer_texts[layer_idx]);
        let layer_byte = mapper.position_to_byte(position)?;

        // Map layer byte to document byte
        let doc_byte = layer_to_doc_offset(layer_byte, self.layer_ranges[layer_idx])?;

        self.byte_to_position(doc_byte)
    }
}

impl<'a> PositionMapper for InjectionPositionMapper<'a> {
    fn position_to_byte(&self, position: Position) -> Option<usize> {
        let line = position.line as usize;
        let character = position.character as usize;

        let line_start = self.document_line_starts.get(line)?;
        let line_end = self
            .document_line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.document_text.len());

        let line_text = &self.document_text[*line_start..line_end];

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
        let line = self
            .document_line_starts
            .binary_search(&offset)
            .unwrap_or_else(|i| i.saturating_sub(1));

        let line_start = self.document_line_starts[line];
        let line_offset = offset.saturating_sub(line_start);

        let line_end = self
            .document_line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.document_text.len());

        let line_text = &self.document_text[line_start..line_end.min(self.document_text.len())];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_mapper_basic() {
        let text = "hello\nworld\n";
        let mapper = SimplePositionMapper::new(text);

        // First line
        let pos = Position {
            line: 0,
            character: 0,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(0));

        let pos = Position {
            line: 0,
            character: 5,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(5));

        // Second line
        let pos = Position {
            line: 1,
            character: 0,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(6));

        let pos = Position {
            line: 1,
            character: 5,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(11));
    }

    #[test]
    fn test_simple_mapper_utf8() {
        let text = "hello\n世界\n";
        let mapper = SimplePositionMapper::new(text);

        // Japanese characters: each is 3 bytes in UTF-8, 1 code unit in UTF-16
        let pos = Position {
            line: 1,
            character: 0,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(6));

        let pos = Position {
            line: 1,
            character: 1,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(9)); // After "世"

        let pos = Position {
            line: 1,
            character: 2,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(12)); // After "界"
    }

    #[test]
    fn test_simple_mapper_utf16_emoji() {
        let text = "hello 👋 world";
        let mapper = SimplePositionMapper::new(text);

        // "hello " = 6 bytes, 6 UTF-16 units
        let pos = Position {
            line: 0,
            character: 6,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(6));

        // "👋" = 4 bytes, 2 UTF-16 units
        let pos = Position {
            line: 0,
            character: 8,
        };
        assert_eq!(mapper.position_to_byte(pos), Some(10)); // After emoji

        // " world" starts after emoji
        let pos = Position {
            line: 0,
            character: 9,
        };
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

    #[test]
    fn test_convert_utf16_to_byte_in_line() {
        // ASCII text
        let text = "hello world";
        assert_eq!(convert_utf16_to_byte_in_line(text, 0), Some(0));
        assert_eq!(convert_utf16_to_byte_in_line(text, 5), Some(5));
        assert_eq!(convert_utf16_to_byte_in_line(text, 11), Some(11));
        assert_eq!(convert_utf16_to_byte_in_line(text, 12), None); // Beyond end

        // UTF-8 text (Japanese)
        let text = "世界"; // Each char is 3 bytes in UTF-8, 1 unit in UTF-16
        assert_eq!(convert_utf16_to_byte_in_line(text, 0), Some(0));
        assert_eq!(convert_utf16_to_byte_in_line(text, 1), Some(3));
        assert_eq!(convert_utf16_to_byte_in_line(text, 2), Some(6));
        assert_eq!(convert_utf16_to_byte_in_line(text, 3), None);

        // Emoji (surrogate pair)
        let text = "👋"; // 4 bytes in UTF-8, 2 units in UTF-16
        assert_eq!(convert_utf16_to_byte_in_line(text, 0), Some(0));
        assert_eq!(convert_utf16_to_byte_in_line(text, 2), Some(4));
        assert_eq!(convert_utf16_to_byte_in_line(text, 3), None);

        // Mixed text
        let text = "a世b👋c";
        assert_eq!(convert_utf16_to_byte_in_line(text, 0), Some(0)); // 'a'
        assert_eq!(convert_utf16_to_byte_in_line(text, 1), Some(1)); // '世'
        assert_eq!(convert_utf16_to_byte_in_line(text, 2), Some(4)); // 'b'
        assert_eq!(convert_utf16_to_byte_in_line(text, 3), Some(5)); // '👋'
        assert_eq!(convert_utf16_to_byte_in_line(text, 5), Some(9)); // 'c'
        assert_eq!(convert_utf16_to_byte_in_line(text, 6), Some(10)); // end
    }

    #[test]
    fn test_extract_text_from_ranges() {
        let doc_text = "line1\nline2\nline3\n";

        // Single range
        let ranges = vec![(6, 12)]; // "line2\n"
        let extracted = extract_text_from_ranges(doc_text, &ranges);
        assert_eq!(extracted, "line2\n");

        // Multiple ranges
        let ranges = vec![(0, 6), (12, 18)]; // "line1\n" and "line3\n"
        let extracted = extract_text_from_ranges(doc_text, &ranges);
        assert_eq!(extracted, "line1\nline3\n");

        // Empty ranges
        let ranges = vec![];
        let extracted = extract_text_from_ranges(doc_text, &ranges);
        assert_eq!(extracted, "");
    }

    #[test]
    fn test_doc_to_layer_offset() {
        let ranges = vec![(0, 6), (12, 18)]; // Two separate ranges

        // First range
        assert_eq!(doc_to_layer_offset(0, &ranges), Some(0));
        assert_eq!(doc_to_layer_offset(5, &ranges), Some(5));

        // Second range - maps to position after first range in layer
        assert_eq!(doc_to_layer_offset(12, &ranges), Some(6));
        assert_eq!(doc_to_layer_offset(17, &ranges), Some(11));

        // Outside ranges
        assert_eq!(doc_to_layer_offset(8, &ranges), None);
        assert_eq!(doc_to_layer_offset(20, &ranges), None);
    }

    #[test]
    fn test_layer_to_doc_offset() {
        let ranges = vec![(6, 12), (12, 18)]; // Two consecutive ranges

        // First part of layer text (from first range)
        assert_eq!(layer_to_doc_offset(0, &ranges), Some(6));
        assert_eq!(layer_to_doc_offset(5, &ranges), Some(11));

        // Second part of layer text (from second range)
        assert_eq!(layer_to_doc_offset(6, &ranges), Some(12));
        assert_eq!(layer_to_doc_offset(11, &ranges), Some(17));

        // Beyond layer text
        assert_eq!(layer_to_doc_offset(15, &ranges), None);
    }

    #[test]
    fn test_contains_offset() {
        let ranges = vec![(10, 20), (30, 40)];

        assert!(!contains_offset(0, &ranges));
        assert!(!contains_offset(9, &ranges));
        assert!(contains_offset(10, &ranges));
        assert!(contains_offset(15, &ranges));
        assert!(contains_offset(19, &ranges));
        assert!(!contains_offset(20, &ranges));
        assert!(!contains_offset(25, &ranges));
        assert!(contains_offset(30, &ranges));
        assert!(contains_offset(39, &ranges));
        assert!(!contains_offset(40, &ranges));
    }

    #[test]
    fn test_injection_position_mapper() {
        // Create mock language layers for testing
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("test", None).unwrap();

        let text = "0123456789abcdefghij0123456789";
        let layers = vec![
            crate::document::LanguageLayer::injection("rust".to_string(), tree.clone(), vec![(0, 10), (20, 30)]),
            crate::document::LanguageLayer::injection("comment".to_string(), tree, vec![(10, 20)]),
        ];
        let mapper = InjectionPositionMapper::new(text, &layers);

        // Test finding layer at position
        let pos = Position {
            line: 0,
            character: 5,
        };
        let layer = mapper.get_layer_at_position(pos);
        assert_eq!(layer, Some("rust"));

        let pos = Position {
            line: 0,
            character: 15,
        };
        let layer = mapper.get_layer_at_position(pos);
        assert_eq!(layer, Some("comment"));
    }

    #[test]
    fn test_injection_layer_offset_mapping() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("test", None).unwrap();

        let text = "0123456789abcdefghij0123456789";
        let layers = vec![
            crate::document::LanguageLayer::injection("rust".to_string(), tree, vec![(0, 10), (20, 30)]),
        ];
        let mapper = InjectionPositionMapper::new(text, &layers);

        // Map position in document to layer position
        let doc_pos = Position {
            line: 0,
            character: 5,
        };
        let layer_pos = mapper.doc_position_to_layer_position(doc_pos, "rust");
        assert!(layer_pos.is_some());

        // Map back from layer to document
        let back = mapper.layer_position_to_doc_position(layer_pos.unwrap(), "rust");
        assert_eq!(back, Some(doc_pos));
    }

    #[test]
    fn test_convert_byte_to_utf16_in_line() {
        // ASCII text
        let text = "hello world";
        assert_eq!(convert_byte_to_utf16_in_line(text, 0), Some(0));
        assert_eq!(convert_byte_to_utf16_in_line(text, 5), Some(5));
        assert_eq!(convert_byte_to_utf16_in_line(text, 11), Some(11));
        assert_eq!(convert_byte_to_utf16_in_line(text, 12), None); // Beyond end

        // UTF-8 text (Japanese)
        let text = "世界"; // Each char is 3 bytes in UTF-8
        assert_eq!(convert_byte_to_utf16_in_line(text, 0), Some(0));
        assert_eq!(convert_byte_to_utf16_in_line(text, 3), Some(1));
        assert_eq!(convert_byte_to_utf16_in_line(text, 6), Some(2));
        assert_eq!(convert_byte_to_utf16_in_line(text, 1), None); // Middle of char
        assert_eq!(convert_byte_to_utf16_in_line(text, 2), None); // Middle of char

        // Emoji (surrogate pair)
        let text = "👋"; // 4 bytes in UTF-8, 2 units in UTF-16
        assert_eq!(convert_byte_to_utf16_in_line(text, 0), Some(0));
        assert_eq!(convert_byte_to_utf16_in_line(text, 4), Some(2));
        assert_eq!(convert_byte_to_utf16_in_line(text, 2), None); // Middle of emoji

        // Mixed text
        let text = "a世b👋c";
        assert_eq!(convert_byte_to_utf16_in_line(text, 0), Some(0)); // 'a'
        assert_eq!(convert_byte_to_utf16_in_line(text, 1), Some(1)); // '世'
        assert_eq!(convert_byte_to_utf16_in_line(text, 4), Some(2)); // 'b'
        assert_eq!(convert_byte_to_utf16_in_line(text, 5), Some(3)); // '👋'
        assert_eq!(convert_byte_to_utf16_in_line(text, 9), Some(5)); // 'c'
        assert_eq!(convert_byte_to_utf16_in_line(text, 10), Some(6)); // end
    }
}
