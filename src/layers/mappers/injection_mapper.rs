use super::position_mapper::{PositionMapper, compute_line_starts};
use crate::layers::LanguageLayer;
use tower_lsp::lsp_types::Position;

/// Position mapper that handles injection layers
/// Supports coordinate transformation between document and injection layers
pub struct InjectionPositionMapper<'a> {
    document_text: &'a str,
    document_line_starts: Vec<usize>,
    layers: Vec<LayerMapping<'a>>,
}

/// Mapping information for a single language layer
struct LayerMapping<'a> {
    layer: &'a LanguageLayer,

    // Document coordinate system
    doc_ranges: Vec<(usize, usize)>,

    // Layer coordinate system
    layer_text: String,
    layer_line_starts: Vec<usize>,
}

impl<'a> LayerMapping<'a> {
    /// Check if a document byte offset falls within this layer
    fn contains_doc_offset(&self, offset: usize) -> bool {
        self.doc_ranges
            .iter()
            .any(|(start, end)| offset >= *start && offset < *end)
    }

    /// Map document byte offset to layer byte offset
    /// Uses lazy computation - calculates offset on demand
    fn map_doc_to_layer_offset(&self, doc_offset: usize) -> Option<usize> {
        let mut layer_offset = 0;

        for (start, end) in &self.doc_ranges {
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
    /// Uses lazy computation - calculates offset on demand
    fn map_layer_to_doc_offset(&self, layer_offset: usize) -> Option<usize> {
        let mut accumulated_length = 0;

        for (start, end) in &self.doc_ranges {
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

impl<'a> InjectionPositionMapper<'a> {
    /// Create a new injection position mapper
    pub fn new(text: &'a str, layers: &'a [LanguageLayer]) -> Self {
        let document_line_starts = compute_line_starts(text);

        let layer_mappings = layers
            .iter()
            .map(|layer| {
                let layer_text = extract_layer_text(text, &layer.ranges);
                let layer_line_starts = compute_line_starts(&layer_text);

                LayerMapping {
                    layer,
                    doc_ranges: layer.ranges.clone(),
                    layer_text,
                    layer_line_starts,
                }
            })
            .collect();

        Self {
            document_text: text,
            document_line_starts,
            layers: layer_mappings,
        }
    }

    /// Find the layer containing a document position
    pub fn find_layer_at_position(&self, position: Position) -> Option<&LanguageLayer> {
        let doc_offset = self.position_to_byte(position)?;

        for mapping in &self.layers {
            if mapping.contains_doc_offset(doc_offset) {
                return Some(mapping.layer);
            }
        }

        None
    }

    /// Map a document position to a layer-local position
    pub fn map_to_layer_position(
        &self,
        doc_position: Position,
        layer_id: &str,
    ) -> Option<Position> {
        // Find the matching layer
        let layer_mapping = self
            .layers
            .iter()
            .find(|m| m.layer.language_id == layer_id)?;

        // Convert document position to byte offset
        let doc_offset = self.position_to_byte(doc_position)?;

        // Check if this offset is within the layer
        if !layer_mapping.contains_doc_offset(doc_offset) {
            return None;
        }

        // Map to layer offset
        let layer_offset = layer_mapping.map_doc_to_layer_offset(doc_offset)?;

        // Convert layer offset to position
        doc_byte_to_position(
            &layer_mapping.layer_text,
            &layer_mapping.layer_line_starts,
            layer_offset,
        )
    }

    /// Map a layer-local position to document position
    pub fn map_from_layer_position(
        &self,
        layer_position: Position,
        layer_id: &str,
    ) -> Option<Position> {
        // Find the matching layer
        let layer_mapping = self
            .layers
            .iter()
            .find(|m| m.layer.language_id == layer_id)?;

        // Convert layer position to byte offset
        let layer_offset = doc_position_to_byte(
            &layer_mapping.layer_text,
            &layer_mapping.layer_line_starts,
            layer_position,
        )?;

        // Map to document offset
        let doc_offset = layer_mapping.map_layer_to_doc_offset(layer_offset)?;

        // Convert document offset to position
        self.byte_to_position(doc_offset)
    }
}

impl<'a> PositionMapper for InjectionPositionMapper<'a> {
    fn position_to_byte(&self, position: Position) -> Option<usize> {
        doc_position_to_byte(self.document_text, &self.document_line_starts, position)
    }

    fn byte_to_position(&self, offset: usize) -> Option<Position> {
        doc_byte_to_position(self.document_text, &self.document_line_starts, offset)
    }
}

// Helper functions

/// Extract text from document ranges
fn extract_layer_text(document: &str, ranges: &[(usize, usize)]) -> String {
    let mut result = String::new();

    for (start, end) in ranges {
        if *start < document.len() && *end <= document.len() {
            result.push_str(&document[*start..*end]);
        }
    }

    result
}

/// Convert document position to byte offset
fn doc_position_to_byte(text: &str, line_starts: &[usize], position: Position) -> Option<usize> {
    let line = position.line as usize;
    let character = position.character as usize;

    let line_start = line_starts.get(line)?;
    let line_end = if line + 1 < line_starts.len() {
        line_starts[line + 1] - 1
    } else {
        text.len()
    };

    let line_text = &text[*line_start..line_end.min(text.len())];

    let mut byte_offset = 0;
    let mut utf16_offset = 0;

    for ch in line_text.chars() {
        if utf16_offset >= character {
            return Some(line_start + byte_offset);
        }
        utf16_offset += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }

    Some(line_start + byte_offset.min(line_text.len()))
}

/// Convert document byte offset to position
fn doc_byte_to_position(text: &str, line_starts: &[usize], offset: usize) -> Option<Position> {
    let line = match line_starts.binary_search(&offset) {
        Ok(line) => line,
        Err(line) => line.saturating_sub(1),
    };

    let line_start = line_starts.get(line)?;
    let line_offset = offset.saturating_sub(*line_start);

    // Calculate the line end
    let line_end = if line + 1 < line_starts.len() {
        line_starts[line + 1] - 1
    } else {
        text.len()
    };

    let line_text = &text[*line_start..line_end.min(text.len())];

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

// compute_line_starts is already exported from position_mapper

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn create_mock_tree() -> tree_sitter::Tree {
        // Create a valid tree using a real parser
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse("", None).unwrap()
    }

    #[test]
    fn test_simple_injection() {
        // Document with a code block
        let document = "# Title\n```rust\nfn main() {}\n```\nEnd";

        // Create a mock layer for the Rust code
        let layer = LanguageLayer {
            language_id: "rust".to_string(),
            tree: create_mock_tree(),
            ranges: vec![(16, 29)], // "fn main() {}\n"
            depth: 1,
            parent_injection_node: None,
        };

        let layers = [layer];
        let mapper = InjectionPositionMapper::new(document, &layers);

        // Test finding layer at position
        let pos = Position {
            line: 2,
            character: 0,
        }; // Start of "fn main()"
        let found_layer = mapper.find_layer_at_position(pos);
        assert!(found_layer.is_some());
        assert_eq!(found_layer.unwrap().language_id, "rust");

        // Test mapping to layer position
        let layer_pos = mapper.map_to_layer_position(pos, "rust");
        assert!(layer_pos.is_some());
        let layer_pos = layer_pos.unwrap();
        assert_eq!(layer_pos.line, 0); // First line in the layer
        assert_eq!(layer_pos.character, 0);
    }

    #[test]
    fn test_multiple_injections() {
        let document = "```rust\ncode1\n```\nText\n```js\ncode2\n```";

        let rust_layer = LanguageLayer {
            language_id: "rust".to_string(),
            tree: create_mock_tree(),
            ranges: vec![(8, 14)], // "code1\n"
            depth: 1,
            parent_injection_node: None,
        };

        let js_layer = LanguageLayer {
            language_id: "javascript".to_string(),
            tree: create_mock_tree(),
            ranges: vec![(29, 35)], // "code2\n"
            depth: 1,
            parent_injection_node: None,
        };

        let layers = [rust_layer, js_layer];
        let mapper = InjectionPositionMapper::new(document, &layers);

        // Test first injection
        let pos1 = Position {
            line: 1,
            character: 0,
        }; // "code1"
        let layer1 = mapper.find_layer_at_position(pos1);
        assert!(layer1.is_some());
        assert_eq!(layer1.unwrap().language_id, "rust");

        // Test second injection
        let pos2 = Position {
            line: 5,
            character: 0,
        }; // "code2"
        let layer2 = mapper.find_layer_at_position(pos2);
        assert!(layer2.is_some());
        assert_eq!(layer2.unwrap().language_id, "javascript");
    }

    #[test]
    fn test_layer_offset_mapping() {
        // Create a temporary LanguageLayer for testing
        let test_layer = LanguageLayer {
            language_id: "test".to_string(),
            tree: create_mock_tree(),
            ranges: vec![(10, 20), (30, 40)], // Two ranges
            depth: 1,
            parent_injection_node: None,
        };

        let mapping = LayerMapping {
            layer: &test_layer,
            doc_ranges: vec![(10, 20), (30, 40)],
            layer_text: String::new(),
            layer_line_starts: vec![],
        };

        // Test mapping within first range
        assert_eq!(mapping.map_doc_to_layer_offset(15), Some(5)); // 15 - 10 = 5

        // Test mapping within second range
        assert_eq!(mapping.map_doc_to_layer_offset(35), Some(15)); // 10 + (35 - 30) = 15

        // Test mapping outside ranges
        assert_eq!(mapping.map_doc_to_layer_offset(5), None);
        assert_eq!(mapping.map_doc_to_layer_offset(25), None);

        // Test reverse mapping
        assert_eq!(mapping.map_layer_to_doc_offset(5), Some(15)); // 10 + 5 = 15
        assert_eq!(mapping.map_layer_to_doc_offset(15), Some(35)); // 30 + (15 - 10) = 35
    }
}
