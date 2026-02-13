//! Token post-processing and LSP encoding.
//!
//! This module handles the final steps of semantic token processing:
//! - Filtering zero-length tokens
//! - Sorting by position (line, column, depth, pattern_index)
//! - Deduplicating tokens at the same position
//! - Converting to LSP SemanticToken format with delta-relative positions
//!
//! Note: The term "delta encoding" in this module refers to the LSP protocol's
//! relative position encoding (delta_line, delta_start), not the
//! SemanticTokensDelta optimization which is handled by the `delta` module.

use tower_lsp_server::ls_types::{SemanticToken, SemanticTokens, SemanticTokensResult};

use super::legend::map_capture_to_token_type_and_modifiers;
use super::token_collector::RawToken;

/// Post-process and delta-encode raw tokens into SemanticTokensResult.
///
/// This shared helper:
/// 1. Filters zero-length tokens
/// 2. Sorts by position
/// 3. Deduplicates tokens at same position
/// 4. Delta-encodes for LSP protocol
pub(super) fn finalize_tokens(mut all_tokens: Vec<RawToken>) -> Option<SemanticTokensResult> {
    // Filter out zero-length tokens BEFORE dedup.
    // Unknown captures are already filtered at collection time (apply_capture_mapping returns None).
    all_tokens.retain(|token| token.length > 0);

    // Sort by position (line, column, then prefer deeper tokens, then later patterns)
    all_tokens.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.column.cmp(&b.column))
            .then(b.depth.cmp(&a.depth))
            .then(b.pattern_index.cmp(&a.pattern_index))
    });

    // Deduplicate at same position
    all_tokens.dedup_by(|a, b| a.line == b.line && a.column == b.column);

    if all_tokens.is_empty() {
        return None;
    }

    // Delta-encode
    let mut data = Vec::with_capacity(all_tokens.len());
    let mut last_line = 0usize;
    let mut last_start = 0usize;

    for token in all_tokens {
        // Unknown types are already filtered at collection time (apply_capture_mapping returns None),
        // so map_capture_to_token_type_and_modifiers should always return Some here.
        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&token.mapped_name)
                .expect("all tokens should have known types after apply_capture_mapping filtering");

        let delta_line = token.line - last_line;
        let delta_start = if delta_line == 0 {
            token.column - last_start
        } else {
            token.column
        };

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: token.length as u32,
            token_type,
            token_modifiers_bitset,
        });

        last_line = token.line;
        last_start = token.column;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    /// Helper to create a RawToken for testing
    fn make_token(
        line: usize,
        column: usize,
        length: usize,
        name: &str,
        depth: usize,
        pattern_index: usize,
    ) -> RawToken {
        RawToken {
            line,
            column,
            length,
            mapped_name: name.to_string(),
            depth,
            pattern_index,
        }
    }

    #[test]
    fn finalize_tokens_returns_none_for_empty_input() {
        let tokens: Vec<RawToken> = vec![];
        assert!(finalize_tokens(tokens).is_none());
    }

    #[test]
    fn finalize_tokens_filters_zero_length_tokens() {
        let tokens = vec![
            make_token(0, 0, 0, "keyword", 0, 0), // zero length - should be filtered
            make_token(0, 5, 3, "variable", 0, 0), // valid
        ];
        let result = finalize_tokens(tokens);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(semantic_tokens)) = result {
            assert_eq!(semantic_tokens.data.len(), 1);
            assert_eq!(semantic_tokens.data[0].delta_start, 5);
        } else {
            panic!("Expected Tokens variant");
        }
    }

    #[test]
    fn finalize_tokens_returns_none_when_all_tokens_are_zero_length() {
        let tokens = vec![
            make_token(0, 0, 0, "keyword", 0, 0),
            make_token(1, 5, 0, "variable", 0, 0),
        ];
        assert!(finalize_tokens(tokens).is_none());
    }

    #[test]
    fn finalize_tokens_sorts_by_position() {
        let tokens = vec![
            make_token(1, 0, 3, "keyword", 0, 0),  // line 1
            make_token(0, 10, 3, "string", 0, 0),  // line 0, col 10
            make_token(0, 0, 3, "function", 0, 0), // line 0, col 0
        ];
        let result = finalize_tokens(tokens);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(semantic_tokens)) = result {
            assert_eq!(semantic_tokens.data.len(), 3);
            // First token: line 0, col 0 (delta_line=0, delta_start=0)
            assert_eq!(semantic_tokens.data[0].delta_line, 0);
            assert_eq!(semantic_tokens.data[0].delta_start, 0);
            // Second token: line 0, col 10 (delta_line=0, delta_start=10)
            assert_eq!(semantic_tokens.data[1].delta_line, 0);
            assert_eq!(semantic_tokens.data[1].delta_start, 10);
            // Third token: line 1, col 0 (delta_line=1, delta_start=0)
            assert_eq!(semantic_tokens.data[2].delta_line, 1);
            assert_eq!(semantic_tokens.data[2].delta_start, 0);
        } else {
            panic!("Expected Tokens variant");
        }
    }

    #[test]
    fn finalize_tokens_delta_encoding_same_line() {
        // Multiple tokens on the same line use delta_start relative to previous token
        let tokens = vec![
            make_token(0, 0, 3, "keyword", 0, 0),
            make_token(0, 5, 4, "function", 0, 0),
            make_token(0, 12, 2, "variable", 0, 0),
        ];
        let result = finalize_tokens(tokens);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(semantic_tokens)) = result {
            assert_eq!(semantic_tokens.data.len(), 3);
            // First: delta_line=0, delta_start=0
            assert_eq!(semantic_tokens.data[0].delta_line, 0);
            assert_eq!(semantic_tokens.data[0].delta_start, 0);
            // Second: delta_line=0, delta_start=5 (relative to previous start=0)
            assert_eq!(semantic_tokens.data[1].delta_line, 0);
            assert_eq!(semantic_tokens.data[1].delta_start, 5);
            // Third: delta_line=0, delta_start=7 (12 - 5 = 7)
            assert_eq!(semantic_tokens.data[2].delta_line, 0);
            assert_eq!(semantic_tokens.data[2].delta_start, 7);
        } else {
            panic!("Expected Tokens variant");
        }
    }

    #[test]
    fn finalize_tokens_delta_encoding_new_line_resets_column() {
        // When moving to a new line, delta_start is absolute column (not relative)
        let tokens = vec![
            make_token(0, 5, 3, "keyword", 0, 0),
            make_token(1, 10, 4, "function", 0, 0),
        ];
        let result = finalize_tokens(tokens);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(semantic_tokens)) = result {
            assert_eq!(semantic_tokens.data.len(), 2);
            // First: delta_line=0, delta_start=5
            assert_eq!(semantic_tokens.data[0].delta_line, 0);
            assert_eq!(semantic_tokens.data[0].delta_start, 5);
            // Second: delta_line=1, delta_start=10 (absolute, not relative)
            assert_eq!(semantic_tokens.data[1].delta_line, 1);
            assert_eq!(semantic_tokens.data[1].delta_start, 10);
        } else {
            panic!("Expected Tokens variant");
        }
    }

    // At same position, dedup keeps the winner after sorting by (depth DESC, pattern_index DESC).
    #[rstest]
    #[case::deeper_injection_wins(
        ("string", 0, 0), ("keyword", 1, 0), "keyword"
    )]
    #[case::latest_pattern_wins(
        ("variable", 0, 0), ("type.builtin", 0, 10), "type.builtin"
    )]
    #[case::latest_pattern_wins_reversed_insertion(
        ("type.builtin", 0, 10), ("variable", 0, 0), "type.builtin"
    )]
    #[case::depth_beats_pattern_index(
        ("variable", 0, 99), ("keyword", 1, 0), "keyword"
    )]
    fn finalize_tokens_dedup_priority(
        #[case] token_a: (&str, usize, usize),
        #[case] token_b: (&str, usize, usize),
        #[case] expected_winner: &str,
    ) {
        let tokens = vec![
            make_token(0, 0, 5, token_a.0, token_a.1, token_a.2),
            make_token(0, 0, 5, token_b.0, token_b.1, token_b.2),
        ];
        let result = finalize_tokens(tokens);

        let SemanticTokensResult::Tokens(semantic_tokens) = result.expect("should produce tokens")
        else {
            panic!("Expected Tokens variant");
        };
        assert_eq!(semantic_tokens.data.len(), 1);
        let (expected_type, _) = map_capture_to_token_type_and_modifiers(expected_winner)
            .expect("expected_winner should be a known capture");
        assert_eq!(semantic_tokens.data[0].token_type, expected_type);
    }
}
