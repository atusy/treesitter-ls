use tower_lsp::lsp_types::SemanticToken;

/// Maps semantic tokens from injection layer coordinates to document coordinates
pub struct SemanticTokenMapper<'a> {
    ranges: &'a [(usize, usize)],
    document_text: &'a str,
}

impl<'a> SemanticTokenMapper<'a> {
    pub fn new(ranges: &'a [(usize, usize)], document_text: &'a str) -> Self {
        Self {
            ranges,
            document_text,
        }
    }

    /// Map a semantic token from injection layer position to document position
    /// Returns None if the token falls outside the injection ranges
    pub fn map_token(&self, token: SemanticToken) -> Option<SemanticToken> {
        // For injection layers, tokens are relative to the concatenated injection text
        // We need to map them back to their position in the original document

        // Calculate the absolute position in the injection text
        let injection_line = token.delta_line;
        let injection_start = token.delta_start;

        // Find which range this token belongs to and map to document position
        let (doc_line, doc_start) =
            self.injection_to_document_position(injection_line, injection_start)?;

        Some(SemanticToken {
            delta_line: doc_line,
            delta_start: doc_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers_bitset,
        })
    }

    /// Convert injection layer line/column to document line/column
    fn injection_to_document_position(
        &self,
        injection_line: u32,
        injection_col: u32,
    ) -> Option<(u32, u32)> {
        if self.ranges.is_empty() {
            return None;
        }

        // Track current position in injection text
        let mut current_injection_line = 0u32;
        let mut current_injection_col = 0u32;

        for (range_start, range_end) in self.ranges {
            let range_text = &self.document_text[*range_start..*range_end];

            // Count lines in this range
            for (line_idx, line) in range_text.lines().enumerate() {
                if current_injection_line == injection_line {
                    // Found the target line
                    if current_injection_col + (line.len() as u32) >= injection_col {
                        // Token is in this line
                        let byte_offset = *range_start
                            + range_text
                                .lines()
                                .take(line_idx)
                                .map(|l| l.len() + 1)
                                .sum::<usize>()
                            + (injection_col - current_injection_col) as usize;

                        // Convert byte offset to document line/column
                        return self.byte_offset_to_line_col(byte_offset);
                    }
                }

                if current_injection_line < injection_line {
                    current_injection_line += 1;
                    current_injection_col = 0;
                } else {
                    current_injection_col += line.len() as u32 + 1; // +1 for newline
                }
            }
        }

        None
    }

    /// Convert byte offset in document to line/column
    fn byte_offset_to_line_col(&self, byte_offset: usize) -> Option<(u32, u32)> {
        let mut line = 0u32;
        let mut current_offset = 0;

        for doc_line in self.document_text.lines() {
            let line_len = doc_line.len() + 1; // +1 for newline

            if current_offset + line_len > byte_offset {
                // Token is in this line
                let col = (byte_offset - current_offset) as u32;
                return Some((line, col));
            }

            current_offset += line_len;
            line += 1;
        }

        // Handle last line without newline
        if current_offset <= byte_offset && byte_offset <= self.document_text.len() {
            let col = (byte_offset - current_offset) as u32;
            return Some((line, col));
        }

        None
    }

    /// Map multiple tokens from injection layer to document coordinates
    pub fn map_tokens(&self, tokens: Vec<SemanticToken>) -> Vec<SemanticToken> {
        tokens
            .into_iter()
            .filter_map(|token| self.map_token(token))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_range_mapping() {
        let document_text = "line1\nline2\nline3\nline4\n";
        let ranges = vec![(6, 18)]; // "line2\nline3\n"
        let mapper = SemanticTokenMapper::new(&ranges, document_text);

        // Token at start of injection (line2)
        let token = SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 0,
            token_modifiers_bitset: 0,
        };

        let mapped = mapper.map_token(token).unwrap();
        assert_eq!(mapped.delta_line, 1); // line2 is at line index 1
        assert_eq!(mapped.delta_start, 0);
        assert_eq!(mapped.length, 5);
    }

    #[test]
    fn test_multiple_ranges_mapping() {
        let document_text = "line1\nline2\nline3\nline4\n";
        let ranges = vec![(0, 6), (12, 18)]; // "line1\n" and "line3\n"
        let mapper = SemanticTokenMapper::new(&ranges, document_text);

        // Token in second range (line3)
        let token = SemanticToken {
            delta_line: 1, // Second line of injection text
            delta_start: 0,
            length: 5,
            token_type: 0,
            token_modifiers_bitset: 0,
        };

        let mapped = mapper.map_token(token).unwrap();
        assert_eq!(mapped.delta_line, 2); // line3 is at line index 2
        assert_eq!(mapped.delta_start, 0);
    }

    #[test]
    fn test_empty_ranges() {
        let document_text = "line1\nline2\n";
        let ranges = vec![];
        let mapper = SemanticTokenMapper::new(&ranges, document_text);

        let token = SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 5,
            token_type: 0,
            token_modifiers_bitset: 0,
        };

        assert!(mapper.map_token(token).is_none());
    }
}
