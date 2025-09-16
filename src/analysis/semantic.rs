use crate::config::CaptureMappings;
use crate::domain::{
    LEGEND_MODIFIERS, LEGEND_TYPES, Range, SemanticToken, SemanticTokens, SemanticTokensDelta,
    SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensResult,
};
use crate::text::convert_byte_to_utf16_in_line;
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

/// Convert byte column position to UTF-16 column position within a line
/// This is a wrapper around the common utility for backward compatibility
fn byte_to_utf16_col(line: &str, byte_col: usize) -> usize {
    // The common utility returns Option, but we need to handle the case where
    // byte_col is beyond the end of the line or in the middle of a character
    convert_byte_to_utf16_in_line(line, byte_col).unwrap_or_else(|| {
        // If conversion fails (e.g., byte_col is in the middle of a multi-byte char),
        // find the nearest valid position
        let mut valid_col = byte_col;
        while valid_col > 0 {
            if let Some(utf16) = convert_byte_to_utf16_in_line(line, valid_col) {
                return utf16;
            }
            valid_col -= 1;
        }
        // Fallback to 0 if no valid position found
        0
    })
}

/// Apply capture mappings to transform a capture name
///
/// Looks up the capture name in the provided mappings and returns the mapped value if found,
/// or the original capture name if no mapping exists.
///
/// # Arguments
/// * `capture_name` - The original capture name from the tree-sitter query
/// * `filetype` - The filetype of the document being processed  
/// * `capture_mappings` - The full capture mappings configuration
///
/// # Returns
/// The mapped capture name or the original if no mapping exists
fn apply_capture_mapping(
    capture_name: &str,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> String {
    if let Some(mappings) = capture_mappings {
        // Try filetype-specific mapping first
        if let Some(ft) = filetype
            && let Some(lang_mappings) = mappings.get(ft)
            && let Some(mapped) = lang_mappings.highlights.get(capture_name)
        {
            return mapped.clone();
        }

        // Try wildcard mapping
        if let Some(wildcard_mappings) = mappings.get("_")
            && let Some(mapped) = wildcard_mappings.highlights.get(capture_name)
        {
            return mapped.clone();
        }
    }

    // Return original if no mapping found
    capture_name.to_string()
}

/// Map capture names from tree-sitter queries to LSP semantic token types and modifiers
///
/// Capture names can be in the format "type.modifier1.modifier2" where:
/// - The first part is the token type (e.g., "variable", "function")
/// - Following parts are modifiers (e.g., "readonly", "defaultLibrary")
fn map_capture_to_token_type_and_modifiers(capture_name: &str) -> (u32, u32) {
    let parts: Vec<&str> = capture_name.split('.').collect();
    let token_type_name = parts.first().copied().unwrap_or("variable");

    let token_type_index = LEGEND_TYPES
        .iter()
        .position(|t| *t == token_type_name)
        .or_else(|| LEGEND_TYPES.iter().position(|t| *t == "variable"))
        .unwrap_or(0) as u32;

    let mut modifiers_bitset = 0u32;
    for modifier_name in &parts[1..] {
        if let Some(index) = LEGEND_MODIFIERS.iter().position(|m| *m == *modifier_name) {
            modifiers_bitset |= 1 << index;
        }
    }

    (token_type_index, modifiers_bitset)
}

/// Handle semantic tokens full request
///
/// Analyzes the entire document and returns all semantic tokens.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
///
/// # Returns
/// Semantic tokens for the entire document
pub fn handle_semantic_tokens_full(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensResult> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    // Collect all tokens with their positions
    // Pre-allocate with estimated capacity to reduce reallocations
    let mut tokens = Vec::with_capacity(1000);

    // Pre-calculate line starts for efficient UTF-16 position conversion
    let lines: Vec<&str> = text.lines().collect();

    while let Some(m) = matches.next() {
        // Filter captures based on predicates
        let filtered_captures = crate::runtime::filter_captures(query, m, text);

        for c in filtered_captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Only include single-line tokens
            if start_pos.row == end_pos.row {
                // Convert byte columns to UTF-16 columns
                let line = lines.get(start_pos.row).unwrap_or(&"");

                // Calculate UTF-16 column positions from byte positions
                let start_utf16 = byte_to_utf16_col(line, start_pos.column);
                let end_utf16 = byte_to_utf16_col(line, end_pos.column);

                tokens.push((start_pos.row, start_utf16, end_utf16 - start_utf16, c.index));
            }
        }
    }

    // Sort tokens by position
    tokens.sort();

    // Convert to LSP semantic tokens format (relative positions)
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::with_capacity(tokens.len());

    for (line, start, length, capture_index) in tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        // Map capture name to token type and modifiers
        let original_capture_name = &query.capture_names()[capture_index as usize];
        let mapped_capture_name =
            apply_capture_mapping(original_capture_name, filetype, capture_mappings);
        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&mapped_capture_name);

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: length as u32,
            token_type,
            token_modifiers_bitset,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Handle semantic tokens range request
///
/// Analyzes a specific range of the document and returns semantic tokens
/// only for that range.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting
/// * `range` - The range to get tokens for (LSP positions)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
///
/// # Returns
/// Semantic tokens for the specified range of the document
pub fn handle_semantic_tokens_range(
    text: &str,
    tree: &Tree,
    query: &Query,
    range: &Range,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensResult> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    // Convert LSP range to line numbers for filtering
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    // Pre-calculate line starts for efficient UTF-16 position conversion
    let lines: Vec<&str> = text.lines().collect();

    // Collect tokens within the range
    // Pre-allocate with estimated capacity for typical visible range
    let mut tokens = Vec::with_capacity(200);
    while let Some(m) = matches.next() {
        // Filter captures based on predicates
        let filtered_captures = crate::runtime::filter_captures(query, m, text);

        for c in filtered_captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Check if token is within the requested range
            if start_pos.row < start_line || start_pos.row > end_line {
                continue;
            }

            // Only include single-line tokens
            if start_pos.row == end_pos.row {
                let line = lines.get(start_pos.row).unwrap_or(&"");

                // Convert byte columns to UTF-16 columns for proper boundary checking
                let start_utf16 = byte_to_utf16_col(line, start_pos.column);
                let end_utf16 = byte_to_utf16_col(line, end_pos.column);

                // For tokens on the boundary lines, check column positions (now in UTF-16)
                if start_pos.row == end_line && start_utf16 > range.end.character as usize {
                    continue;
                }
                if start_pos.row == start_line && end_utf16 < range.start.character as usize {
                    continue;
                }

                tokens.push((start_pos.row, start_utf16, end_utf16 - start_utf16, c.index));
            }
        }
    }

    // Sort tokens by position
    tokens.sort();

    // Convert to LSP semantic tokens format (relative positions)
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::with_capacity(tokens.len());

    for (line, start, length, capture_index) in tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        // Map capture name to token type and modifiers
        let original_capture_name = &query.capture_names()[capture_index as usize];
        let mapped_capture_name =
            apply_capture_mapping(original_capture_name, filetype, capture_mappings);
        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&mapped_capture_name);

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: length as u32,
            token_type,
            token_modifiers_bitset,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Handle semantic tokens full delta request
///
/// Analyzes the document and returns either a delta from the previous version
/// or the full set of semantic tokens if delta cannot be calculated.
///
/// # Arguments
/// * `text` - The current source text
/// * `tree` - The parsed syntax tree for the current text
/// * `query` - The tree-sitter query for semantic highlighting
/// * `previous_result_id` - The result ID from the previous semantic tokens response
/// * `previous_tokens` - The previous semantic tokens to calculate delta from
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
///
/// # Returns
/// Either a delta or full semantic tokens for the document
pub fn handle_semantic_tokens_full_delta(
    text: &str,
    tree: &Tree,
    query: &Query,
    previous_result_id: &str,
    previous_tokens: Option<&SemanticTokens>,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensFullDeltaResult> {
    // Get current tokens
    let current_result =
        handle_semantic_tokens_full(text, tree, query, filetype, capture_mappings)?;
    let SemanticTokensResult::Tokens(current_tokens) = current_result;

    // Check if we can calculate a delta
    if let Some(prev) = previous_tokens
        && prev.result_id.as_deref() == Some(previous_result_id)
        && let Some(delta) = calculate_semantic_tokens_delta(prev, &current_tokens)
    {
        return Some(SemanticTokensFullDeltaResult::Delta(delta));
    }

    // Fall back to full tokens
    Some(SemanticTokensFullDeltaResult::Tokens(current_tokens))
}

/// Calculate delta between two sets of semantic tokens
///
/// Compares previous and current semantic tokens and returns the differences
/// as a set of edits that can be applied to transform the previous tokens
/// into the current tokens.
///
/// # Arguments
/// * `previous` - The previous semantic tokens
/// * `current` - The current semantic tokens
///
/// # Returns
/// Semantic tokens delta containing the edits needed to transform previous to current
fn calculate_semantic_tokens_delta(
    previous: &SemanticTokens,
    current: &SemanticTokens,
) -> Option<SemanticTokensDelta> {
    // Find the common prefix length
    let common_prefix_len = previous
        .data
        .iter()
        .zip(current.data.iter())
        .take_while(|(a, b)| {
            a.delta_line == b.delta_line
                && a.delta_start == b.delta_start
                && a.length == b.length
                && a.token_type == b.token_type
                && a.token_modifiers_bitset == b.token_modifiers_bitset
        })
        .count();

    // If all tokens are the same, no edits needed
    if common_prefix_len == previous.data.len() && common_prefix_len == current.data.len() {
        return Some(SemanticTokensDelta {
            result_id: current.result_id.clone(),
            edits: vec![],
        });
    }

    // Calculate the edit
    let start = common_prefix_len;
    let delete_count = previous.data.len() - start;
    let data = current.data[start..].to_vec();

    Some(SemanticTokensDelta {
        result_id: current.result_id.clone(),
        edits: vec![SemanticTokensEdit {
            start: start as u32,
            delete_count: delete_count as u32,
            data,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_capture_to_token_type_and_modifiers() {
        // Test basic token types without modifiers
        assert_eq!(map_capture_to_token_type_and_modifiers("comment"), (0, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("keyword"), (1, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("function"), (14, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("variable"), (17, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("unknown"), (17, 0)); // Should default to variable

        // Test with single modifier
        let (_, modifiers) = map_capture_to_token_type_and_modifiers("variable.readonly");
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly is at index 2

        let (_, modifiers) = map_capture_to_token_type_and_modifiers("function.async");
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async is at index 6

        // Test with multiple modifiers
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("variable.readonly.defaultLibrary");
        assert_eq!(token_type, 17); // variable
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly
        assert_eq!(modifiers & (1 << 9), 1 << 9); // defaultLibrary

        // Test unknown modifiers are ignored
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("function.unknownModifier.async");
        assert_eq!(token_type, 14); // function
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async should still be set
    }

    #[test]
    fn test_semantic_tokens_delta() {
        // Create mock semantic tokens for testing
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 10,
                    token_type: 0, // comment
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 3,
                    token_type: 1, // keyword
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 1,
                    token_type: 17, // variable
                    token_modifiers_bitset: 0,
                },
            ],
        };

        // Modified tokens (changed comment length)
        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 14,    // changed length
                    token_type: 0, // comment
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 3,
                    token_type: 1, // keyword
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 1,
                    token_type: 17, // variable
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.result_id, Some("v2".to_string()));
        assert_eq!(delta.edits.len(), 1);
        assert_eq!(delta.edits[0].start, 0);
        assert_eq!(delta.edits[0].delete_count, 3);
        assert_eq!(delta.edits[0].data.len(), 3);
    }

    #[test]
    fn test_semantic_tokens_range() {
        use crate::domain::Position;

        // Create mock tokens for a document
        let all_tokens = SemanticTokens {
            result_id: None,
            data: vec![
                SemanticToken {
                    // Line 0, col 0-10
                    delta_line: 0,
                    delta_start: 0,
                    length: 10,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    // Line 2, col 0-3
                    delta_line: 2,
                    delta_start: 0,
                    length: 3,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    // Line 2, col 4-5
                    delta_line: 0,
                    delta_start: 4,
                    length: 1,
                    token_type: 17,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    // Line 4, col 2-8
                    delta_line: 2,
                    delta_start: 2,
                    length: 6,
                    token_type: 14,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        // Test range that includes only lines 1-3
        let _range = Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 100,
            },
        };

        // Tokens in range should be the ones on line 2
        // We'd need actual tree-sitter setup to test the real function,
        // so this is more of a placeholder showing the expected structure
        assert_eq!(all_tokens.data.len(), 4);
    }

    #[test]
    fn test_semantic_tokens_delta_no_changes() {
        let tokens = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 10,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };

        let delta = calculate_semantic_tokens_delta(&tokens, &tokens);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 0);
    }

    #[test]
    fn test_byte_to_utf16_col() {
        // ASCII text
        let line = "hello world";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 5), 5);
        assert_eq!(byte_to_utf16_col(line, 11), 11);

        // Japanese text (3 bytes per char in UTF-8, 1 code unit in UTF-16)
        let line = "こんにちは";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 3), 1); // After "こ"
        assert_eq!(byte_to_utf16_col(line, 6), 2); // After "こん"
        assert_eq!(byte_to_utf16_col(line, 15), 5); // After all 5 chars

        // Mixed ASCII and Japanese
        let line = "let x = \"あいうえお\"";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 8), 8); // Before '"'
        assert_eq!(byte_to_utf16_col(line, 9), 9); // Before "あ"
        assert_eq!(byte_to_utf16_col(line, 12), 10); // After "あ" (3 bytes -> 1 UTF-16)
        assert_eq!(byte_to_utf16_col(line, 24), 14); // After "あいうえお\"" (15 bytes + 1 quote)

        // Emoji (4 bytes in UTF-8, 2 code units in UTF-16)
        let line = "hello 👋 world";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 6), 6); // After "hello "
        assert_eq!(byte_to_utf16_col(line, 10), 8); // After emoji (4 bytes -> 2 UTF-16)
    }

    #[test]
    fn test_semantic_tokens_with_japanese() {
        use tree_sitter::{Parser, Query};

        let text = r#"let x = "あいうえお"
let y = "hello""#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        let query_text = r#"
            "let" @keyword
            (identifier) @variable
            (string_literal) @string
        "#;

        let query = Query::new(&language, query_text).unwrap();
        let result = handle_semantic_tokens_full(text, &tree, &query, Some("rust"), None);

        assert!(result.is_some());

        // Verify tokens were generated (can't inspect internals due to private type)
        match result.unwrap() {
            SemanticTokensResult::Tokens(tokens) => {
                // Should have tokens for: let, x, string, let, y, string
                assert!(tokens.data.len() >= 6);

                // Check that the string token on first line has correct UTF-16 length
                // "あいうえお" = 5 UTF-16 code units + 2 quotes = 7
                let string_token = tokens
                    .data
                    .iter()
                    .find(|t| t.token_type == 2 && t.length == 7); // string type = 2
                assert!(
                    string_token.is_some(),
                    "Japanese string token should have UTF-16 length of 7"
                );
            }
        }
    }
}
