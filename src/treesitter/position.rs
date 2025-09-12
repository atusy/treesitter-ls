use tower_lsp::lsp_types::{Position, Range};

/// Convert LSP position (line/character) to byte offset in text
///
/// # Arguments
/// * `text` - The source text
/// * `position` - LSP position with line and character (character is UTF-16 code unit offset)
///
/// # Returns
/// Byte offset in the text
///
/// # TODO
/// - Add line offset caching for better performance
pub fn position_to_byte_offset(text: &str, position: Position) -> usize {
    let mut byte_offset = 0;
    let mut current_line = 0;
    let mut current_char_utf16 = 0;

    for ch in text.chars() {
        // Check if we've reached the target position
        if current_line == position.line as usize {
            if current_char_utf16 == position.character as usize {
                return byte_offset;
            }
            // If we're past the requested character position on the target line,
            // clamp to the current position
            if current_char_utf16 > position.character as usize {
                // Need to backtrack - this shouldn't happen with valid positions
                return byte_offset;
            }
        }

        // If we're past the target line, return current offset
        if current_line > position.line as usize {
            return byte_offset;
        }

        // Update position tracking
        if ch == '\n' {
            // If we're at the end of the target line, return the position before newline
            if current_line == position.line as usize {
                return byte_offset;
            }
            current_line += 1;
            current_char_utf16 = 0;
        } else if ch == '\r' {
            // Handle CRLF - don't count \r in character position
            // Just add to byte offset
        } else {
            current_char_utf16 += ch.len_utf16();
        }

        byte_offset += ch.len_utf8();
    }

    byte_offset
}

/// Convert a byte offset into an LSP `Position` (line and UTF-16 code unit character).
pub fn byte_offset_to_position(text: &str, byte_offset: usize) -> Position {
    let mut current_line = 0usize;
    let mut current_char_utf16 = 0usize;
    let mut processed_bytes = 0usize;

    for ch in text.chars() {
        if processed_bytes >= byte_offset {
            return Position {
                line: current_line as u32,
                character: current_char_utf16 as u32,
            };
        }

        processed_bytes += ch.len_utf8();

        if ch == '\n' {
            // If the byte offset is exactly after the newline,
            // we're at the start of the next line
            if processed_bytes == byte_offset {
                return Position {
                    line: (current_line + 1) as u32,
                    character: 0,
                };
            }
            current_line += 1;
            current_char_utf16 = 0;
        } else if ch == '\r' {
            // Handle CRLF - don't count \r in character position
            // Just processed the byte
        } else {
            current_char_utf16 += ch.len_utf16();
        }
    }

    // If offset is at or beyond end, clamp to end position
    Position {
        line: current_line as u32,
        character: current_char_utf16 as u32,
    }
}

/// Convert a byte range [start, end) into an LSP `Range`.
pub fn byte_range_to_range(text: &str, start: usize, end: usize) -> Range {
    let start_pos = byte_offset_to_position(text, start);
    let end_pos = byte_offset_to_position(text, end);
    Range {
        start: start_pos,
        end: end_pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_byte_offset_basic() {
        let text = "hello\nworld";
        let pos = Position {
            line: 1,
            character: 2,
        };
        assert_eq!(position_to_byte_offset(text, pos), 8); // "hello\n" = 6 bytes, "wo" = 2 bytes
    }

    #[test]
    fn test_position_to_byte_offset_utf8() {
        let text = "hello\n世界";
        let pos = Position {
            line: 1,
            character: 1,
        };
        assert_eq!(position_to_byte_offset(text, pos), 9); // "hello\n" = 6 bytes, "世" = 3 bytes
    }

    #[test]
    fn test_position_to_byte_offset_start() {
        let text = "hello world";
        let pos = Position {
            line: 0,
            character: 0,
        };
        assert_eq!(position_to_byte_offset(text, pos), 0);
    }

    #[test]
    fn test_position_to_byte_offset_end() {
        let text = "hello";
        let pos = Position {
            line: 0,
            character: 10,
        }; // Beyond end
        assert_eq!(position_to_byte_offset(text, pos), 5); // Returns text length
    }

    #[test]
    fn test_position_to_byte_offset_utf16_encoding() {
        // LSP spec states that Position.character is a UTF-16 code unit offset
        // This test demonstrates the bug where we're currently using char count instead

        // Text with emoji that takes 2 UTF-16 code units
        let text = "hello 👋 world";

        // Position after the emoji - LSP would send character: 8
        // "hello " = 6 UTF-16 code units, "👋" = 2 UTF-16 code units
        let pos = Position {
            line: 0,
            character: 8,
        };

        // Expected byte offset: "hello " = 6 bytes, "👋" = 4 bytes, " " = 1 byte
        // So position after emoji should be at byte 10
        assert_eq!(position_to_byte_offset(text, pos), 10);
    }

    #[test]
    fn test_position_with_crlf() {
        let text = "hello\r\nworld";

        // Position at start of second line
        let pos = Position {
            line: 1,
            character: 0,
        };
        assert_eq!(position_to_byte_offset(text, pos), 7); // "hello\r\n" = 7 bytes

        // Convert back
        let result_pos = byte_offset_to_position(text, 7);
        assert_eq!(result_pos.line, 1);
        assert_eq!(result_pos.character, 0);
    }

    #[test]
    fn test_position_with_japanese_text() {
        let text = "こんにちは\n世界";

        // Japanese characters: each is 3 bytes in UTF-8, 1 code unit in UTF-16
        // Position after "こん" (2 chars)
        let pos = Position {
            line: 0,
            character: 2,
        };
        assert_eq!(position_to_byte_offset(text, pos), 6); // 2 chars * 3 bytes = 6

        // Position at start of second line
        let pos = Position {
            line: 1,
            character: 0,
        };
        assert_eq!(position_to_byte_offset(text, pos), 16); // "こんにちは" = 15 bytes + "\n" = 1 byte
    }

    #[test]
    fn test_position_at_line_end() {
        let text = "hello\nworld";

        // Position at end of first line (after 'o', before '\n')
        let pos = Position {
            line: 0,
            character: 5,
        };
        assert_eq!(position_to_byte_offset(text, pos), 5);

        // Position at the newline itself should return position before newline
        let pos = Position {
            line: 0,
            character: 10, // Past the line end
        };
        assert_eq!(position_to_byte_offset(text, pos), 5); // Clamps to end of line
    }

    #[test]
    fn test_byte_offset_to_position_edge_cases() {
        let text = "hello\n世界";

        // At newline (before the \n character)
        let pos = byte_offset_to_position(text, 5);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);

        // Right after newline
        let pos = byte_offset_to_position(text, 6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // After first Japanese character
        let pos = byte_offset_to_position(text, 9); // "hello\n" = 6 bytes, "世" = 3 bytes
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 1); // After "世", before "界"
    }
}
