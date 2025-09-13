use super::position_mapper::{PositionMapper, compute_line_starts};
use tower_lsp::lsp_types::Position;

/// Pure range-based coordinate mapper
/// Maps positions between document and layer coordinate systems
/// without any dependency on specific layer types
pub struct RangeMapper<'a> {
    #[allow(dead_code)]
    document_text: &'a str,
    #[allow(dead_code)]
    document_line_starts: Vec<usize>,
    layer_info: LayerInfo<'a>,
}

/// Information about a single layer's ranges and text
pub struct LayerInfo<'a> {
    /// Identifier for this layer (e.g., language name)
    pub id: &'a str,

    /// Byte ranges in the document where this layer applies
    pub ranges: &'a [(usize, usize)],

    /// Extracted text from the ranges
    pub text: String,

    /// Line start offsets in the extracted text
    pub line_starts: Vec<usize>,
}

impl<'a> LayerInfo<'a> {
    /// Create layer info from document text and ranges
    pub fn new(id: &'a str, document_text: &str, ranges: &'a [(usize, usize)]) -> Self {
        let text = extract_text_from_ranges(document_text, ranges);
        let line_starts = compute_line_starts(&text);

        Self {
            id,
            ranges,
            text,
            line_starts,
        }
    }

    /// Check if a document byte offset falls within this layer
    pub fn contains_offset(&self, offset: usize) -> bool {
        self.ranges
            .iter()
            .any(|(start, end)| offset >= *start && offset < *end)
    }

    /// Map document byte offset to layer byte offset
    pub fn doc_to_layer_offset(&self, doc_offset: usize) -> Option<usize> {
        let mut layer_offset = 0;

        for (start, end) in self.ranges {
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

    /// Map layer byte offset to document byte offset
    pub fn layer_to_doc_offset(&self, layer_offset: usize) -> Option<usize> {
        let mut accumulated_length = 0;

        for (start, end) in self.ranges {
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
}

impl<'a> RangeMapper<'a> {
    /// Create a new range mapper for a single layer
    pub fn new(document_text: &'a str, layer_info: LayerInfo<'a>) -> Self {
        let document_line_starts = compute_line_starts(document_text);

        Self {
            document_text,
            document_line_starts,
            layer_info,
        }
    }

    /// Get the layer info
    pub fn layer_info(&self) -> &LayerInfo<'a> {
        &self.layer_info
    }
}

impl<'a> PositionMapper for RangeMapper<'a> {
    fn position_to_byte(&self, position: Position) -> Option<usize> {
        // Convert position in layer coordinates to byte offset
        let line = position.line as usize;
        let character = position.character as usize;

        // Get line start in layer text
        let line_start = self.layer_info.line_starts.get(line)?;
        let line_end = self
            .layer_info
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.layer_info.text.len());

        let line_text = &self.layer_info.text[*line_start..line_end];

        // Convert UTF-16 character offset to byte offset
        let mut byte_offset = 0;
        let mut utf16_offset = 0;

        for ch in line_text.chars() {
            if utf16_offset >= character {
                break;
            }
            utf16_offset += ch.len_utf16();
            byte_offset += ch.len_utf8();
        }

        Some(line_start + byte_offset)
    }

    fn byte_to_position(&self, offset: usize) -> Option<Position> {
        // Convert byte offset in layer to position
        let line = self
            .layer_info
            .line_starts
            .binary_search(&offset)
            .unwrap_or_else(|i| i.saturating_sub(1));

        let line_start = self.layer_info.line_starts[line];
        let line_offset = offset - line_start;

        // Convert to UTF-16 character offset
        let line_end = self
            .layer_info
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.layer_info.text.len());

        let line_text = &self.layer_info.text[line_start..line_end.min(self.layer_info.text.len())];

        let mut utf16_offset = 0;
        let mut byte_count = 0;

        for ch in line_text.chars() {
            if byte_count >= line_offset {
                break;
            }
            byte_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        Some(Position {
            line: line as u32,
            character: utf16_offset as u32,
        })
    }
}

/// Extract text from document for given ranges
fn extract_text_from_ranges(document_text: &str, ranges: &[(usize, usize)]) -> String {
    let mut result = String::new();

    for (start, end) in ranges {
        if *start < document_text.len() && *end <= document_text.len() {
            result.push_str(&document_text[*start..*end]);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_info_contains_offset() {
        let doc_text = "line1\nline2\nline3\n";
        let ranges = vec![(6, 12)]; // "line2\n"
        let layer_info = LayerInfo::new("test", doc_text, &ranges);

        assert!(layer_info.contains_offset(6));
        assert!(layer_info.contains_offset(11));
        assert!(!layer_info.contains_offset(5));
        assert!(!layer_info.contains_offset(12));
    }

    #[test]
    fn test_doc_to_layer_offset() {
        let doc_text = "line1\nline2\nline3\n";
        let ranges = vec![(0, 6), (12, 18)]; // "line1\n" and "line3\n"
        let layer_info = LayerInfo::new("test", doc_text, &ranges);

        // First range
        assert_eq!(layer_info.doc_to_layer_offset(0), Some(0));
        assert_eq!(layer_info.doc_to_layer_offset(5), Some(5));

        // Second range - maps to position after first range in layer
        assert_eq!(layer_info.doc_to_layer_offset(12), Some(6));
        assert_eq!(layer_info.doc_to_layer_offset(17), Some(11));

        // Outside ranges
        assert_eq!(layer_info.doc_to_layer_offset(8), None);
    }

    #[test]
    fn test_layer_to_doc_offset() {
        let doc_text = "line1\nline2\nline3\n";
        let ranges = vec![(6, 12), (12, 18)]; // "line2\n" and "line3\n"
        let layer_info = LayerInfo::new("test", doc_text, &ranges);

        // First part of layer text (from first range)
        assert_eq!(layer_info.layer_to_doc_offset(0), Some(6));
        assert_eq!(layer_info.layer_to_doc_offset(5), Some(11));

        // Second part of layer text (from second range)
        assert_eq!(layer_info.layer_to_doc_offset(6), Some(12));
        assert_eq!(layer_info.layer_to_doc_offset(11), Some(17));
    }
}
