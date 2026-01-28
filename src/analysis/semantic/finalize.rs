//! Token post-processing and delta encoding.
//!
//! This module handles the final steps of semantic token processing:
//! filtering, sorting, deduplication, and LSP delta encoding.

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

    // Sort by position (line, column, then prefer deeper tokens)
    all_tokens.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.column.cmp(&b.column))
            .then(b.depth.cmp(&a.depth))
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
