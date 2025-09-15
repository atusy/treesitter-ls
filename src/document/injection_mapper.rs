use crate::syntax::LanguageLayer;
use crate::text::{
    PositionMapper, compute_line_starts, convert_byte_to_utf16_in_line,
    convert_utf16_to_byte_in_line, extract_text_from_ranges,
};
use tower_lsp::lsp_types::Position;

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
    pub fn new(text: &'a str, layers: &'a [LanguageLayer]) -> Self {
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
        self.layer_ids.iter().position(|&id| id == layer_id)
    }

    /// Map document position to layer position
    pub fn map_position_to_layer(
        &self,
        position: Position,
        layer_index: usize,
    ) -> Option<Position> {
        // Convert document position to byte offset
        let doc_byte = self.position_to_byte(position)?;

        // Map to layer byte offset
        let layer_ranges = self.layer_ranges.get(layer_index)?;
        let layer_byte = doc_to_layer_offset(doc_byte, layer_ranges)?;

        // Convert layer byte offset to position within layer text
        let layer_text = self.layer_texts.get(layer_index)?;
        let layer_line_starts = compute_line_starts(layer_text);

        // Binary search for the line containing this offset
        let line = match layer_line_starts.binary_search(&layer_byte) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };

        let line_start = layer_line_starts.get(line)?;
        let line_offset = layer_byte.saturating_sub(*line_start);

        // Get the line text to calculate UTF-16 character position
        let line_end = if line + 1 < layer_line_starts.len() {
            layer_line_starts[line + 1] - 1
        } else {
            layer_text.len()
        };

        let line_text = &layer_text[*line_start..line_end.min(layer_text.len())];
        let character = convert_byte_to_utf16_in_line(line_text, line_offset).unwrap_or(0);

        Some(Position {
            line: line as u32,
            character: character as u32,
        })
    }

    /// Map layer position back to document position
    pub fn map_position_from_layer(
        &self,
        layer_position: Position,
        layer_index: usize,
    ) -> Option<Position> {
        let layer_text = self.layer_texts.get(layer_index)?;
        let layer_line_starts = compute_line_starts(layer_text);

        // Convert layer position to layer byte offset
        let line = layer_position.line as usize;
        let character = layer_position.character as usize;

        let line_start = layer_line_starts.get(line)?;
        let line_end = if line + 1 < layer_line_starts.len() {
            layer_line_starts[line + 1] - 1
        } else {
            layer_text.len()
        };

        let line_text = &layer_text[*line_start..line_end];
        let line_byte_offset = convert_utf16_to_byte_in_line(line_text, character)?;
        let layer_byte = line_start + line_byte_offset;

        // Map layer byte offset to document byte offset
        let layer_ranges = self.layer_ranges.get(layer_index)?;
        let doc_byte = layer_to_doc_offset(layer_byte, layer_ranges)?;

        // Convert document byte offset to position
        self.byte_to_position(doc_byte)
    }
}

impl<'a> PositionMapper for InjectionPositionMapper<'a> {
    fn position_to_byte(&self, position: Position) -> Option<usize> {
        let line = position.line as usize;
        let character = position.character as usize;

        // Get the start of the target line
        let line_start = self.document_line_starts.get(line)?;

        // Find the end of the line (or end of text)
        let line_end = if line + 1 < self.document_line_starts.len() {
            self.document_line_starts[line + 1] - 1
        } else {
            self.document_text.len()
        };

        // Get the line text
        let line_text = &self.document_text[*line_start..line_end];

        // Convert UTF-16 position to byte offset within the line
        match convert_utf16_to_byte_in_line(line_text, character) {
            Some(byte_offset) => Some(line_start + byte_offset),
            None => Some(line_start + line_text.len()),
        }
    }

    fn byte_to_position(&self, offset: usize) -> Option<Position> {
        // Binary search for the line containing this offset
        let line = match self.document_line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line.saturating_sub(1),
        };

        let line_start = self.document_line_starts.get(line)?;

        // Calculate the UTF-16 character offset within the line
        let line_offset = offset.saturating_sub(*line_start);
        let line_end = if line + 1 < self.document_line_starts.len() {
            self.document_line_starts[line + 1] - 1
        } else {
            self.document_text.len()
        };

        let line_text = &self.document_text[*line_start..line_end.min(self.document_text.len())];

        // Convert byte offset to UTF-16 position within the line
        let character = convert_byte_to_utf16_in_line(line_text, line_offset).unwrap_or(0);

        Some(Position {
            line: line as u32,
            character: character as u32,
        })
    }
}
