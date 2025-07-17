use tower_lsp::lsp_types::Position;

/// Convert LSP position (line/character) to byte offset in text
///
/// # Arguments
/// * `text` - The source text
/// * `position` - LSP position with line and character
///
/// # Returns
/// Byte offset in the text
///
/// # TODO
/// - Add line offset caching for better performance
/// - Handle UTF-16 encoding properly (LSP uses UTF-16 for character offsets)
pub fn position_to_byte_offset(text: &str, position: Position) -> usize {
    let mut byte_offset = 0;
    let mut current_line = 0;
    let mut current_char = 0;

    for ch in text.chars() {
        if current_line == position.line as usize && current_char == position.character as usize {
            return byte_offset;
        }

        if ch == '\n' {
            current_line += 1;
            current_char = 0;
        } else {
            current_char += 1;
        }

        byte_offset += ch.len_utf8();
    }

    byte_offset
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
}
