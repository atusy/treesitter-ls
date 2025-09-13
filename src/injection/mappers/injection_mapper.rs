use super::position_mapper::{PositionMapper, compute_line_starts};
use super::range_mapper::{LayerInfo, RangeMapper};
use tower_lsp::lsp_types::Position;

/// Position mapper that handles multiple injection layers
/// This is a facade that converts LanguageLayer references to pure range mappers
pub struct InjectionPositionMapper<'a> {
    document_text: &'a str,
    document_line_starts: Vec<usize>,
    /// Range mappers for each layer
    mappers: Vec<RangeMapper<'a>>,
    /// Layer identifiers for lookups
    layer_ids: Vec<&'a str>,
    /// Ranges for each layer (for position calculations)
    layer_ranges: Vec<&'a [(usize, usize)]>,
}

impl<'a> InjectionPositionMapper<'a> {
    /// Create a new injection position mapper from language layers
    /// This extracts only the necessary data (ranges and IDs) from the layers
    pub fn new(text: &'a str, layers: &'a [crate::injection::LanguageLayer]) -> Self {
        let document_line_starts = compute_line_starts(text);

        let mut mappers = Vec::new();
        let mut layer_ids = Vec::new();
        let mut layer_ranges = Vec::new();

        for layer in layers {
            let layer_info = LayerInfo::new(&layer.language_id, text, &layer.ranges);
            let mapper = RangeMapper::new(text, layer_info);

            mappers.push(mapper);
            layer_ids.push(layer.language_id.as_str());
            layer_ranges.push(&layer.ranges as &[(usize, usize)]);
        }

        Self {
            document_text: text,
            document_line_starts,
            mappers,
            layer_ids,
            layer_ranges,
        }
    }

    /// Find the layer containing the given document position
    pub fn get_layer_at_position(&self, position: Position) -> Option<&str> {
        let byte_offset = self.position_to_byte(position)?;

        for (i, ranges) in self.layer_ranges.iter().enumerate() {
            for (start, end) in *ranges {
                if byte_offset >= *start && byte_offset < *end {
                    return Some(self.layer_ids[i]);
                }
            }
        }

        None
    }

    /// Get the layer by its identifier
    pub fn get_layer_by_id(&self, layer_id: &str) -> Option<&RangeMapper<'a>> {
        self.layer_ids
            .iter()
            .position(|id| *id == layer_id)
            .map(|i| &self.mappers[i])
    }

    /// Map document position to layer position for a specific layer
    pub fn doc_position_to_layer_position(
        &self,
        position: Position,
        layer_id: &str,
    ) -> Option<Position> {
        let mapper = self.get_layer_by_id(layer_id)?;
        let doc_byte = self.position_to_byte(position)?;

        let layer_info = mapper.layer_info();
        let layer_byte = layer_info.doc_to_layer_offset(doc_byte)?;

        mapper.byte_to_position(layer_byte)
    }

    /// Map layer position to document position for a specific layer
    pub fn layer_position_to_doc_position(
        &self,
        position: Position,
        layer_id: &str,
    ) -> Option<Position> {
        let mapper = self.get_layer_by_id(layer_id)?;
        let layer_byte = mapper.position_to_byte(position)?;

        let layer_info = mapper.layer_info();
        let doc_byte = layer_info.layer_to_doc_offset(layer_byte)?;

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

        // Convert UTF-16 character offset to byte offset
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

        // Convert byte offset to UTF-16 character offset
        let mut utf16_offset = 0;
        let mut byte_count = 0;

        for ch in line_text.chars() {
            if byte_count >= line_offset {
                break;
            }
            let ch_bytes = ch.len_utf8();
            if byte_count + ch_bytes > line_offset {
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

/// Helper function to convert document byte to position for layers
pub fn doc_byte_to_position(text: &str, byte_offset: usize) -> Option<Position> {
    let line_starts = compute_line_starts(text);

    let line = line_starts
        .binary_search(&byte_offset)
        .unwrap_or_else(|i| i.saturating_sub(1));

    if line >= line_starts.len() {
        return None;
    }

    let line_start = line_starts[line];
    let line_offset = byte_offset.saturating_sub(line_start);

    // Get the line end (start of next line or end of document)
    let line_end = line_starts.get(line + 1).copied().unwrap_or(text.len());

    // Get the line text
    let line_text = &text[line_start..line_end.min(text.len())];

    // Convert byte offset to UTF-16 character offset
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::injection::LanguageLayer;

    fn create_test_layers() -> Vec<LanguageLayer> {
        // Create mock language layers for testing
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("test", None).unwrap();

        vec![
            LanguageLayer {
                language_id: "rust".to_string(),
                tree: tree.clone(),
                ranges: vec![(0, 10), (20, 30)],
                depth: 1,
                parent_injection_node: None,
            },
            LanguageLayer {
                language_id: "comment".to_string(),
                tree,
                ranges: vec![(10, 20)],
                depth: 1,
                parent_injection_node: None,
            },
        ]
    }

    #[test]
    fn test_simple_injection() {
        let text = "0123456789abcdefghij0123456789";
        let layers = create_test_layers();
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
    fn test_layer_offset_mapping() {
        let text = "0123456789abcdefghij0123456789";
        let layers = create_test_layers();
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
    fn test_multiple_injections() {
        let text = "fn main() {\n    // comment\n    let x = 1;\n}";
        let layers = vec![LanguageLayer {
            language_id: "rust".to_string(),
            tree: {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_rust::LANGUAGE.into())
                    .unwrap();
                parser.parse("test", None).unwrap()
            },
            ranges: vec![(0, 12), (27, 44)],
            depth: 1,
            parent_injection_node: None,
        }];

        let mapper = InjectionPositionMapper::new(text, &layers);

        // Test position in first range
        let pos = Position {
            line: 0,
            character: 3,
        };
        let layer = mapper.get_layer_at_position(pos);
        assert_eq!(layer, Some("rust"));

        // Test position outside any layer
        let pos = Position {
            line: 1,
            character: 5,
        };
        let layer = mapper.get_layer_at_position(pos);
        assert_eq!(layer, None);
    }
}
