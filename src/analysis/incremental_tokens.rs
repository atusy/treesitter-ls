//! Incremental tokenization using Tree-sitter's changed_ranges API.

use tree_sitter::{Range as TsRange, Tree};

/// Decision for which tokenization strategy to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalDecision {
    /// Use incremental tokenization - only re-tokenize changed regions
    UseIncremental,
    /// Use full tokenization - re-tokenize the entire document
    UseFull,
}

/// Decide whether to use incremental or full tokenization.
///
/// Returns `UseIncremental` when:
/// - previous_tree is available (Some)
/// - changed ranges indicate a localized edit (not a large structural change)
///
/// Returns `UseFull` when:
/// - No previous tree available
/// - Changes are too large/scattered for incremental to be beneficial
pub fn decide_tokenization_strategy(
    previous_tree: Option<&Tree>,
    current_tree: &Tree,
    document_len: usize,
) -> IncrementalDecision {
    let Some(prev_tree) = previous_tree else {
        log::debug!(
            target: "kakehashi::incremental",
            "Strategy=UseFull: no previous tree"
        );
        return IncrementalDecision::UseFull;
    };

    let changed_ranges = get_changed_ranges(prev_tree, current_tree);

    // Log diagnostic information about the change
    let total_changed: usize = changed_ranges
        .iter()
        .map(|r| r.end_byte.saturating_sub(r.start_byte))
        .sum();
    let ratio = if document_len > 0 {
        total_changed as f64 / document_len as f64
    } else {
        0.0
    };

    if is_large_structural_change(&changed_ranges, total_changed, document_len) {
        log::debug!(
            target: "kakehashi::incremental",
            "Strategy=UseFull: large change detected (ranges={}, bytes={}, ratio={:.2}%, doc_len={})",
            changed_ranges.len(),
            total_changed,
            ratio * 100.0,
            document_len
        );
        IncrementalDecision::UseFull
    } else {
        log::debug!(
            target: "kakehashi::incremental",
            "Strategy=UseIncremental: small change (ranges={}, bytes={}, ratio={:.2}%)",
            changed_ranges.len(),
            total_changed,
            ratio * 100.0
        );
        IncrementalDecision::UseIncremental
    }
}

/// Determine if the changes are too large for incremental tokenization.
/// Returns true if full re-tokenization is more efficient.
///
/// Heuristics:
/// - More than 10 changed ranges = likely a large structural change
/// - Changed bytes exceed 30% of document = significant rewrite
///
/// # Arguments
/// * `ranges` - The changed byte ranges
/// * `total_changed` - Pre-computed total changed bytes (sum of range sizes)
/// * `document_len` - Total document length in bytes
pub fn is_large_structural_change(
    ranges: &[TsRange],
    total_changed: usize,
    document_len: usize,
) -> bool {
    const MAX_RANGES: usize = 10;
    const MAX_CHANGE_RATIO: f64 = 0.30;

    // Too many changed ranges
    if ranges.len() > MAX_RANGES {
        return true;
    }

    // More than 30% of document changed
    if document_len > 0 {
        let ratio = total_changed as f64 / document_len as f64;
        return ratio > MAX_CHANGE_RATIO;
    }

    false
}

/// Get the byte ranges that changed between two trees.
/// Returns a vector of ranges that differ between the old and new trees.
///
/// Note: For best results, the old tree should have been edited via `tree.edit()`
/// before the new tree was parsed. Without proper edit information, Tree-sitter
/// may return larger ranges than strictly necessary.
pub fn get_changed_ranges(old_tree: &Tree, new_tree: &Tree) -> Vec<TsRange> {
    old_tree.changed_ranges(new_tree).collect()
}

/// Convert byte ranges to the set of affected line numbers.
/// Returns a set of line indices that overlap with any of the changed byte ranges.
///
/// Handles edge cases where Tree-sitter returns invalid ranges (start > end)
/// by treating them as covering the entire document.
pub fn changed_ranges_to_lines(text: &str, ranges: &[TsRange]) -> std::collections::HashSet<usize> {
    let mut affected_lines = std::collections::HashSet::new();
    let text_len = text.len();

    // Build a mapping of byte offset -> line number
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.bytes().enumerate().filter_map(
            |(i, b)| {
                if b == b'\n' { Some(i + 1) } else { None }
            },
        ))
        .collect();
    let total_lines = line_starts.len();

    // For each range, find which lines it touches
    for range in ranges {
        // Validate range: start_byte must be <= end_byte
        // Invalid ranges can occur when trees are from mismatched document states,
        // which is possible during rapid edits. This is not necessarily a bug -
        // tree-sitter's changed_ranges() may produce invalid ranges when the old
        // tree was not properly edited via tree.edit() before comparison.
        if range.start_byte > range.end_byte {
            log::debug!(
                target: "kakehashi::incremental",
                "Invalid range from changed_ranges(): start_byte {} > end_byte {} - treating as full document change",
                range.start_byte,
                range.end_byte
            );
            // Treat invalid range as affecting all lines
            for line in 0..total_lines {
                affected_lines.insert(line);
            }
            continue;
        }

        // Clamp byte positions to text bounds
        let start_byte = range.start_byte.min(text_len);
        let end_byte = range.end_byte.min(text_len);

        let start_line = line_starts
            .partition_point(|&start| start <= start_byte)
            .saturating_sub(1);
        let end_line = line_starts
            .partition_point(|&start| start <= end_byte)
            .saturating_sub(1);

        for line in start_line..=end_line {
            affected_lines.insert(line);
        }
    }

    affected_lines
}

/// Represents a semantic token in absolute position format (before delta encoding).
/// This is used for incremental tokenization where we need to merge token lists.
#[derive(Clone, Debug, PartialEq)]
pub struct AbsoluteToken {
    pub line: u32,
    pub start: u32,
    pub length: u32,
    pub token_type: u32,
    pub token_modifiers_bitset: u32,
}

/// Convert LSP SemanticTokens (delta-encoded) to AbsoluteTokens.
/// This allows us to work with cached tokens for incremental computation.
pub fn decode_semantic_tokens(
    tokens: &tower_lsp_server::ls_types::SemanticTokens,
) -> Vec<AbsoluteToken> {
    let mut result = Vec::with_capacity(tokens.data.len());
    let mut current_line = 0u32;
    let mut current_col = 0u32;

    for token in &tokens.data {
        current_line += token.delta_line;
        if token.delta_line > 0 {
            current_col = token.delta_start;
        } else {
            current_col += token.delta_start;
        }

        result.push(AbsoluteToken {
            line: current_line,
            start: current_col,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers_bitset,
        });
    }

    result
}

/// Convert AbsoluteTokens back to delta-encoded SemanticTokens.
pub fn encode_semantic_tokens(
    tokens: &[AbsoluteToken],
    result_id: Option<String>,
) -> tower_lsp_server::ls_types::SemanticTokens {
    let mut data = Vec::with_capacity(tokens.len());
    let mut last_line = 0u32;
    let mut last_col = 0u32;

    for token in tokens {
        let delta_line = token.line - last_line;
        let delta_start = if delta_line > 0 {
            token.start
        } else {
            token.start - last_col
        };

        data.push(tower_lsp_server::ls_types::SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.token_modifiers_bitset,
        });

        last_line = token.line;
        last_col = token.start;
    }

    tower_lsp_server::ls_types::SemanticTokens { result_id, data }
}

/// Merge old tokens with newly computed tokens for changed regions.
///
/// This function:
/// 1. Keeps tokens from `old_tokens` that are outside the changed line ranges
/// 2. Uses tokens from `new_tokens` for the changed regions
/// 3. Adjusts line numbers for tokens after the changed region if lines were inserted/deleted
///
/// # Arguments
/// * `old_tokens` - Previous tokens in absolute position format
/// * `new_tokens` - Newly computed tokens for the entire document
/// * `changed_lines` - Set of line indices that were modified
/// * `line_delta` - Change in total line count (positive = lines added, negative = removed)
///
/// # Returns
/// Merged tokens with unchanged regions preserved and changed regions updated
pub fn merge_tokens(
    old_tokens: &[AbsoluteToken],
    new_tokens: &[AbsoluteToken],
    changed_lines: &std::collections::HashSet<usize>,
    line_delta: i32,
) -> Vec<AbsoluteToken> {
    if changed_lines.is_empty() {
        // If line count changed but no changed lines were detected, fall back to full tokens.
        if line_delta != 0 {
            return new_tokens.to_vec();
        }
        // No changes, return old tokens as-is
        return old_tokens.to_vec();
    }

    let min_changed_line = *changed_lines.iter().min().unwrap_or(&0);
    let max_changed_line = *changed_lines.iter().max().unwrap_or(&0);

    // Convert changed region bounds to old coordinates
    // changed_lines are in NEW text coordinates
    // For tokens after the change, we need to know where the boundary is in OLD coordinates
    let max_changed_line_old = (max_changed_line as i32 - line_delta).max(0) as usize;

    let mut result = Vec::new();

    // Process old tokens:
    // - Keep tokens BEFORE the changed region (line < min_changed_line)
    // - For tokens IN the [min, max_old] range: keep only if not in changed_lines (for disjoint gaps)
    // - Keep tokens AFTER the changed region (line > max_changed_line_old), with line adjustment
    for token in old_tokens {
        let old_line = token.line as usize;

        if old_line < min_changed_line {
            // Before the first change - keep as-is (no line adjustment needed)
            result.push(token.clone());
        } else if old_line <= max_changed_line_old {
            // Within the changed region (in old coordinates)
            // Check if this old line maps to a NEW line that is NOT in changed_lines
            // For lines in the gap between disjoint changes, preserve them
            //
            // Note: When line_delta != 0, lines shift. But for disjoint changes with line_delta=0,
            // old_line == new_line, so we can directly check changed_lines.
            // When line_delta != 0, the affected lines in old coordinates should NOT be preserved
            // because their content has been deleted or shifted.
            //
            // For simplicity: when line_delta != 0, don't preserve tokens in the [min, max_old] range
            // (let new_tokens provide the correct tokens for this region)
            if line_delta == 0 && !changed_lines.contains(&old_line) {
                // No line count change, and this line is not in changed_lines
                // This is a gap between disjoint changes - preserve the token
                result.push(token.clone());
            }
            // When line_delta != 0, skip tokens in this range - they'll be replaced by new_tokens
        } else {
            // After the changed region - keep with line adjustment
            let mut adjusted = token.clone();
            adjusted.line = ((token.line as i32) + line_delta) as u32;
            result.push(adjusted);
        }
    }

    // Add new tokens ONLY for lines that are IN changed_lines
    // (not all lines in the [min, max] range)
    for token in new_tokens {
        let line = token.line as usize;
        if changed_lines.contains(&line) {
            result.push(token.clone());
        }
    }

    // Sort by position (line, then column)
    result.sort_by(|a, b| a.line.cmp(&b.line).then(a.start.cmp(&b.start)));

    result
}

/// Result of incremental tokenization computation.
pub struct IncrementalTokensResult {
    /// The merged tokens (unchanged regions preserved, changed regions updated)
    pub tokens: Vec<AbsoluteToken>,
    /// The line ranges that were changed
    pub changed_lines: std::collections::HashSet<usize>,
    /// The line count delta (positive = lines added, negative = removed)
    pub line_delta: i32,
}

/// Compute incremental tokens by merging cached tokens with newly computed ones.
///
/// This function orchestrates the incremental tokenization process:
/// 1. Get changed byte ranges using Tree-sitter's changed_ranges API
/// 2. Convert byte ranges to affected line numbers
/// 3. Merge old tokens with new tokens for changed regions only
///
/// # Arguments
/// * `old_tokens` - Previously cached tokens in absolute position format
/// * `previous_tree` - The previous Tree-sitter parse tree
/// * `current_tree` - The current Tree-sitter parse tree
/// * `old_text` - The previous document text (for line counting)
/// * `new_text` - The current document text
/// * `new_tokens` - Newly computed tokens for the entire document
///
/// # Returns
/// The merged tokens where unchanged regions are preserved from old_tokens
/// and changed regions use new_tokens
pub fn compute_incremental_tokens(
    old_tokens: &[AbsoluteToken],
    previous_tree: &Tree,
    current_tree: &Tree,
    old_text: &str,
    new_text: &str,
    new_tokens: &[AbsoluteToken],
) -> IncrementalTokensResult {
    // Get changed byte ranges from Tree-sitter
    let changed_ranges = get_changed_ranges(previous_tree, current_tree);

    // Convert byte ranges to affected line numbers (using new text)
    let changed_lines = changed_ranges_to_lines(new_text, &changed_ranges);

    // Calculate line count delta
    let old_line_count = old_text.lines().count() as i32;
    let new_line_count = new_text.lines().count() as i32;
    let line_delta = new_line_count - old_line_count;

    // Merge tokens
    let tokens = merge_tokens(old_tokens, new_tokens, &changed_lines, line_delta);

    IncrementalTokensResult {
        tokens,
        changed_lines,
        line_delta,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    // Helper to create a range for testing
    fn make_range(start_byte: usize, end_byte: usize) -> TsRange {
        TsRange {
            start_byte,
            end_byte,
            start_point: Point { row: 0, column: 0 },
            end_point: Point { row: 0, column: 0 },
        }
    }

    // Helper to create an AbsoluteToken for testing
    fn make_token(line: u32, start: u32, length: u32, token_type: u32) -> AbsoluteToken {
        AbsoluteToken {
            line,
            start,
            length,
            token_type,
            token_modifiers_bitset: 0,
        }
    }

    // Helper to calculate total changed bytes for a range list
    fn calc_total_changed(ranges: &[TsRange]) -> usize {
        ranges
            .iter()
            .map(|r| r.end_byte.saturating_sub(r.start_byte))
            .sum()
    }

    #[test]
    fn test_heuristic_large_change_triggers_full_recompute() {
        // Test 1: Few small changes - should NOT be large
        let small_changes = vec![make_range(0, 10), make_range(50, 60)];
        let total = calc_total_changed(&small_changes); // 20 bytes
        assert!(!is_large_structural_change(&small_changes, total, 1000));

        // Test 2: More than 10 ranges - should be large
        let many_ranges: Vec<_> = (0..15).map(|i| make_range(i * 10, i * 10 + 5)).collect();
        let total = calc_total_changed(&many_ranges); // 75 bytes, but too many ranges
        assert!(is_large_structural_change(&many_ranges, total, 1000));

        // Test 3: >30% of document changed - should be large
        let large_change = vec![make_range(0, 400)]; // 400 bytes out of 1000 = 40%
        let total = calc_total_changed(&large_change);
        assert!(is_large_structural_change(&large_change, total, 1000));

        // Test 4: Exactly 30% - should NOT be large (boundary)
        let boundary_change = vec![make_range(0, 300)]; // 300 bytes out of 1000 = 30%
        let total = calc_total_changed(&boundary_change);
        assert!(!is_large_structural_change(&boundary_change, total, 1000));
    }

    #[test]
    fn test_changed_ranges_returns_affected_regions() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Parse original code
        let old_tree = parser.parse("fn main() {}", None).unwrap();

        // Parse new code with additional content
        let new_tree = parser.parse("fn main() { let x = 1; }", None).unwrap();

        // Get changed ranges
        let ranges = get_changed_ranges(&old_tree, &new_tree);

        // Should have at least one changed range
        assert!(!ranges.is_empty(), "Should detect changes between trees");
    }

    #[test]
    fn test_changed_ranges_to_lines() {
        // Document with 4 lines:
        // Line 0: "fn main() {\n"  (bytes 0-12)
        // Line 1: "    let x = 1;\n" (bytes 12-27)
        // Line 2: "    let y = 2;\n" (bytes 27-42)
        // Line 3: "}\n" (bytes 42-44)
        let text = "fn main() {\n    let x = 1;\n    let y = 2;\n}\n";

        // Change on line 1 only (bytes 16-17, the 'x')
        let ranges = vec![make_range(16, 17)];
        let affected = changed_ranges_to_lines(text, &ranges);
        assert_eq!(affected.len(), 1, "Should affect exactly one line");
        assert!(affected.contains(&1), "Should affect line 1");

        // Change spanning lines 1-2
        let ranges = vec![make_range(16, 30)];
        let affected = changed_ranges_to_lines(text, &ranges);
        assert!(affected.contains(&1), "Should affect line 1");
        assert!(affected.contains(&2), "Should affect line 2");
    }

    #[test]
    fn test_incremental_tokens_preserves_unchanged() {
        // Scenario: Document with 5 lines, edit on line 2
        // Old tokens: lines 0, 1, 2, 3, 4
        // Changed: line 2 only
        // Expected: tokens on lines 0, 1, 4 preserved from old; line 2 from new

        let old_tokens = vec![
            make_token(0, 0, 5, 1), // "fn" on line 0
            make_token(0, 6, 4, 2), // "main" on line 0
            make_token(1, 4, 3, 3), // "let" on line 1
            make_token(2, 4, 3, 3), // "let" on line 2 (will be replaced)
            make_token(3, 0, 1, 4), // "}" on line 3
        ];

        let new_tokens = vec![
            make_token(0, 0, 5, 1), // "fn" on line 0 (same)
            make_token(0, 6, 4, 2), // "main" on line 0 (same)
            make_token(1, 4, 3, 3), // "let" on line 1 (same)
            make_token(2, 4, 5, 3), // "const" on line 2 (CHANGED - different length)
            make_token(3, 0, 1, 4), // "}" on line 3 (same)
        ];

        let mut changed_lines = std::collections::HashSet::new();
        changed_lines.insert(2);

        let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, 0);

        assert_eq!(result.len(), 5, "Should have 5 tokens");

        // Line 0 tokens preserved from old
        assert_eq!(
            result[0], old_tokens[0],
            "Line 0 token 0 should be from old"
        );
        assert_eq!(
            result[1], old_tokens[1],
            "Line 0 token 1 should be from old"
        );

        // Line 1 token preserved from old
        assert_eq!(result[2], old_tokens[2], "Line 1 token should be from old");

        // Line 2 token should be from new (different length)
        assert_eq!(result[3].length, 5, "Line 2 token should have new length");

        // Line 3 token preserved from old (but needs adjustment check - no change in line count)
        assert_eq!(result[4].line, 3, "Line 3 token should still be on line 3");
    }

    #[test]
    fn test_merge_tokens_falls_back_when_line_delta_without_changes() {
        let old_tokens = vec![
            make_token(0, 0, 2, 1),
            make_token(1, 0, 2, 1),
            make_token(2, 0, 2, 1),
        ];

        let new_tokens = vec![make_token(0, 0, 2, 2), make_token(1, 0, 2, 2)];

        let changed_lines = std::collections::HashSet::new();
        let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, -1);

        assert_eq!(result, new_tokens, "Should fall back to new tokens");
    }

    #[test]
    fn test_incremental_tokens_handles_line_insertion() {
        // Scenario: Line inserted at position 2, pushing old line 2 to line 3
        // Old: 4 lines with tokens at 0, 1, 2, 3
        // New: 5 lines with tokens at 0, 1, 2 (new), 3 (was 2), 4 (was 3)
        // Changed lines: 2 (in old document terms)
        // Line delta: +1

        let old_tokens = vec![
            make_token(0, 0, 5, 1), // line 0
            make_token(1, 0, 5, 2), // line 1
            make_token(2, 0, 5, 3), // line 2 (will shift to 3)
            make_token(3, 0, 5, 4), // line 3 (will shift to 4)
        ];

        let new_tokens = vec![
            make_token(0, 0, 5, 1), // line 0 (same)
            make_token(1, 0, 5, 2), // line 1 (same)
            make_token(2, 0, 6, 5), // line 2 (NEW - inserted line)
            make_token(3, 0, 5, 3), // line 3 (was line 2)
            make_token(4, 0, 5, 4), // line 4 (was line 3)
        ];

        let mut changed_lines = std::collections::HashSet::new();
        changed_lines.insert(2); // Line 2 is where the insertion happened

        let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, 1);

        assert_eq!(result.len(), 5, "Should have 5 tokens after insertion");

        // Lines 0, 1 unchanged
        assert_eq!(result[0].line, 0);
        assert_eq!(result[1].line, 1);

        // Line 2 is the new inserted token
        assert_eq!(result[2].line, 2);
        assert_eq!(result[2].length, 6, "New token on line 2");
        assert_eq!(result[2].token_type, 5, "New token type");

        // Line 3 is the new token (was line 2 in new_tokens - from changed region)
        assert_eq!(result[3].line, 3);

        // Old line 3 shifted to line 4
        assert_eq!(result[4].line, 4);
        assert_eq!(result[4].token_type, 4, "Token type preserved");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        use tower_lsp_server::ls_types::{SemanticToken, SemanticTokens};

        // Create some delta-encoded tokens
        let original = SemanticTokens {
            result_id: Some("test".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 4,
                    token_type: 2,
                    token_modifiers_bitset: 1,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 4,
                    length: 3,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 2,
                    delta_start: 0,
                    length: 1,
                    token_type: 4,
                    token_modifiers_bitset: 2,
                },
            ],
        };

        // Decode to absolute
        let decoded = decode_semantic_tokens(&original);
        assert_eq!(decoded.len(), 4);

        // Check absolute positions
        assert_eq!(decoded[0].line, 0);
        assert_eq!(decoded[0].start, 0);
        assert_eq!(decoded[1].line, 0);
        assert_eq!(decoded[1].start, 6);
        assert_eq!(decoded[2].line, 1);
        assert_eq!(decoded[2].start, 4);
        assert_eq!(decoded[3].line, 3); // 0 + 1 + 2 = 3
        assert_eq!(decoded[3].start, 0);

        // Encode back
        let encoded = encode_semantic_tokens(&decoded, Some("roundtrip".to_string()));

        // Data should match original
        assert_eq!(encoded.data.len(), original.data.len());
        for (i, (enc, orig)) in encoded.data.iter().zip(original.data.iter()).enumerate() {
            assert_eq!(
                enc.delta_line, orig.delta_line,
                "delta_line mismatch at {}",
                i
            );
            assert_eq!(
                enc.delta_start, orig.delta_start,
                "delta_start mismatch at {}",
                i
            );
            assert_eq!(enc.length, orig.length, "length mismatch at {}", i);
            assert_eq!(
                enc.token_type, orig.token_type,
                "token_type mismatch at {}",
                i
            );
            assert_eq!(
                enc.token_modifiers_bitset, orig.token_modifiers_bitset,
                "modifiers mismatch at {}",
                i
            );
        }
    }

    #[test]
    fn test_incremental_path_chosen_when_small_change() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Parse original code - a small function
        let old_code = "fn main() {\n    let x = 1;\n}\n";
        let old_tree = parser.parse(old_code, None).unwrap();

        // Parse with a small edit (change x to y)
        let new_code = "fn main() {\n    let y = 1;\n}\n";
        let new_tree = parser.parse(new_code, None).unwrap();

        // With previous tree and small change, should choose incremental
        let decision = decide_tokenization_strategy(Some(&old_tree), &new_tree, new_code.len());
        assert_eq!(
            decision,
            IncrementalDecision::UseIncremental,
            "Small change with previous_tree should use incremental"
        );

        // Without previous tree, should choose full
        let decision = decide_tokenization_strategy(None, &new_tree, new_code.len());
        assert_eq!(
            decision,
            IncrementalDecision::UseFull,
            "No previous_tree should use full tokenization"
        );
    }

    #[test]
    fn test_full_path_chosen_when_large_change() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Parse original code
        let old_code = "fn main() {}";
        let old_tree = parser.parse(old_code, None).unwrap();

        // Parse with a large change (completely different code)
        let new_code = "struct Foo { a: i32, b: i32, c: i32, d: i32 }\nimpl Foo { fn new() -> Self { Foo { a: 0, b: 0, c: 0, d: 0 } } }";
        let new_tree = parser.parse(new_code, None).unwrap();

        // Large change should choose full even with previous tree
        let decision = decide_tokenization_strategy(Some(&old_tree), &new_tree, new_code.len());
        assert_eq!(
            decision,
            IncrementalDecision::UseFull,
            "Large structural change should use full tokenization"
        );
    }

    #[test]
    fn test_compute_incremental_tokens_preserves_unchanged_regions() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Original code with tokens on lines 0, 1, 2
        let old_code = "fn main() {\n    let x = 1;\n}\n";
        let old_tree = parser.parse(old_code, None).unwrap();

        // Modified code: change 'x' to 'y' on line 1
        let new_code = "fn main() {\n    let y = 1;\n}\n";
        let new_tree = parser.parse(new_code, None).unwrap();

        // Simulate old tokens
        let old_tokens = vec![
            make_token(0, 0, 2, 1), // "fn" on line 0
            make_token(0, 3, 4, 2), // "main" on line 0
            make_token(1, 4, 3, 3), // "let" on line 1
            make_token(1, 8, 1, 4), // "x" on line 1
            make_token(2, 0, 1, 5), // "}" on line 2
        ];

        // Simulate new tokens (same structure, just 'y' instead of 'x')
        let new_tokens = vec![
            make_token(0, 0, 2, 1), // "fn" on line 0
            make_token(0, 3, 4, 2), // "main" on line 0
            make_token(1, 4, 3, 3), // "let" on line 1
            make_token(1, 8, 1, 4), // "y" on line 1 (different char, same position)
            make_token(2, 0, 1, 5), // "}" on line 2
        ];

        let result = compute_incremental_tokens(
            &old_tokens,
            &old_tree,
            &new_tree,
            old_code,
            new_code,
            &new_tokens,
        );

        // Should have same number of tokens
        assert_eq!(result.tokens.len(), 5, "Should have 5 tokens");

        // Line 0 tokens should be preserved from old (unchanged region)
        assert_eq!(result.tokens[0], old_tokens[0], "Line 0 token 0 preserved");
        assert_eq!(result.tokens[1], old_tokens[1], "Line 0 token 1 preserved");

        // Line 2 token should be preserved from old (unchanged region)
        assert_eq!(result.tokens[4], old_tokens[4], "Line 2 token preserved");

        // Line delta should be 0 (same number of lines)
        assert_eq!(result.line_delta, 0, "No line delta for in-place edit");
    }

    #[test]
    fn test_compute_incremental_tokens_handles_line_insertion() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Original code: 2 lines
        let old_code = "fn main() {\n}\n";
        let old_tree = parser.parse(old_code, None).unwrap();

        // Modified code: insert a line in the middle
        let new_code = "fn main() {\n    let x = 1;\n}\n";
        let new_tree = parser.parse(new_code, None).unwrap();

        // Simulate old tokens
        let old_tokens = vec![
            make_token(0, 0, 2, 1), // "fn" on line 0
            make_token(0, 3, 4, 2), // "main" on line 0
            make_token(1, 0, 1, 5), // "}" on line 1
        ];

        // Simulate new tokens with inserted line
        let new_tokens = vec![
            make_token(0, 0, 2, 1), // "fn" on line 0
            make_token(0, 3, 4, 2), // "main" on line 0
            make_token(1, 4, 3, 3), // "let" on line 1 (new!)
            make_token(1, 8, 1, 4), // "x" on line 1 (new!)
            make_token(2, 0, 1, 5), // "}" on line 2 (was line 1)
        ];

        let result = compute_incremental_tokens(
            &old_tokens,
            &old_tree,
            &new_tree,
            old_code,
            new_code,
            &new_tokens,
        );

        // Should have 5 tokens now
        assert_eq!(
            result.tokens.len(),
            5,
            "Should have 5 tokens after insertion"
        );

        // Line 0 tokens should be preserved
        assert_eq!(result.tokens[0].line, 0);
        assert_eq!(result.tokens[1].line, 0);

        // Line delta should be +1 (one line added)
        assert_eq!(result.line_delta, 1, "Should detect line insertion");
    }

    #[test]
    fn test_merge_tokens_with_disjoint_changes() {
        // Scenario: Changes on lines 1 and 4 only (disjoint - not contiguous)
        // Lines 2 and 3 are UNCHANGED and should preserve old tokens
        //
        // Bug: Current implementation treats min=1, max=4 as a contiguous range,
        // replacing ALL tokens in [1,4] including unchanged lines 2-3
        //
        // Expected behavior: Only replace tokens on lines that are IN changed_lines set

        let old_tokens = vec![
            make_token(0, 0, 5, 1), // line 0 - unchanged
            make_token(1, 0, 5, 2), // line 1 - CHANGED
            make_token(2, 0, 5, 3), // line 2 - unchanged (should be preserved!)
            make_token(3, 0, 5, 4), // line 3 - unchanged (should be preserved!)
            make_token(4, 0, 5, 5), // line 4 - CHANGED
        ];

        // New tokens have different token_types for changed lines
        let new_tokens = vec![
            make_token(0, 0, 5, 1),  // line 0 - same
            make_token(1, 0, 5, 12), // line 1 - different (type 12 vs 2)
            make_token(2, 0, 5, 13), // line 2 - different (type 13 vs 3) BUT should NOT be used!
            make_token(3, 0, 5, 14), // line 3 - different (type 14 vs 4) BUT should NOT be used!
            make_token(4, 0, 5, 15), // line 4 - different (type 15 vs 5)
        ];

        let mut changed_lines = std::collections::HashSet::new();
        changed_lines.insert(1); // Only line 1 changed
        changed_lines.insert(4); // Only line 4 changed
        // Lines 2 and 3 are NOT in changed_lines

        let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, 0);

        assert_eq!(result.len(), 5, "Should have 5 tokens");

        // Line 0: unchanged, should be from old
        assert_eq!(
            result[0].token_type, 1,
            "Line 0 should preserve old token (type 1)"
        );

        // Line 1: changed, should be from new
        assert_eq!(
            result[1].token_type, 12,
            "Line 1 should use new token (type 12)"
        );

        // Line 2: NOT changed, should preserve old token (type 3, not 13)
        assert_eq!(
            result[2].token_type, 3,
            "Line 2 should preserve old token (type 3), not new (type 13)"
        );

        // Line 3: NOT changed, should preserve old token (type 4, not 14)
        assert_eq!(
            result[3].token_type, 4,
            "Line 3 should preserve old token (type 4), not new (type 14)"
        );

        // Line 4: changed, should be from new
        assert_eq!(
            result[4].token_type, 15,
            "Line 4 should use new token (type 15)"
        );
    }

    #[test]
    fn test_merge_tokens_with_line_deletion() {
        // Scenario: Line 2 is deleted from a 5-line document
        // Old: lines 0, 1, 2, 3, 4
        // New: lines 0, 1, 2, 3 (old line 2 deleted, old lines 3,4 become 2,3)
        //
        // changed_lines = {1} (the deletion point, where the change occurred)
        // line_delta = -1 (one line removed)
        //
        // Expected:
        // - Line 0 preserved from old
        // - Line 1 uses new tokens (changed)
        // - Lines 2, 3 in result are old lines 3, 4 shifted down

        let old_tokens = vec![
            make_token(0, 0, 5, 1), // line 0
            make_token(1, 0, 5, 2), // line 1 - will be changed
            make_token(2, 0, 5, 3), // line 2 - will be deleted
            make_token(3, 0, 5, 4), // line 3 - shifts to line 2
            make_token(4, 0, 5, 5), // line 4 - shifts to line 3
        ];

        let new_tokens = vec![
            make_token(0, 0, 5, 1),  // line 0 - same
            make_token(1, 0, 5, 12), // line 1 - changed (different type)
            make_token(2, 0, 5, 4),  // line 2 - was old line 3
            make_token(3, 0, 5, 5),  // line 3 - was old line 4
        ];

        let mut changed_lines = std::collections::HashSet::new();
        changed_lines.insert(1); // The deletion happened at/around line 1

        let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, -1);

        assert_eq!(result.len(), 4, "Should have 4 tokens after deletion");

        // Line 0: preserved from old
        assert_eq!(
            result[0].token_type, 1,
            "Line 0 should preserve old token (type 1)"
        );
        assert_eq!(result[0].line, 0);

        // Line 1: from new tokens (changed region)
        assert_eq!(
            result[1].token_type, 12,
            "Line 1 should use new token (type 12)"
        );
        assert_eq!(result[1].line, 1);

        // Line 2: was old line 3, shifted down
        assert_eq!(
            result[2].token_type, 4,
            "Line 2 should be old line 3's token (type 4)"
        );
        assert_eq!(result[2].line, 2);

        // Line 3: was old line 4, shifted down
        assert_eq!(
            result[3].token_type, 5,
            "Line 3 should be old line 4's token (type 5)"
        );
        assert_eq!(result[3].line, 3);
    }

    #[test]
    fn test_merge_tokens_at_document_boundaries() {
        // Test 1: Change at line 0 (first line of document)
        {
            let old_tokens = vec![
                make_token(0, 0, 5, 1), // line 0 - CHANGED
                make_token(1, 0, 5, 2), // line 1 - unchanged
                make_token(2, 0, 5, 3), // line 2 - unchanged
            ];

            let new_tokens = vec![
                make_token(0, 0, 5, 11), // line 0 - new
                make_token(1, 0, 5, 2),  // line 1 - same
                make_token(2, 0, 5, 3),  // line 2 - same
            ];

            let mut changed_lines = std::collections::HashSet::new();
            changed_lines.insert(0); // First line changed

            let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, 0);

            assert_eq!(result.len(), 3, "Should have 3 tokens");
            assert_eq!(
                result[0].token_type, 11,
                "Line 0 should use new token (type 11)"
            );
            assert_eq!(
                result[1].token_type, 2,
                "Line 1 should preserve old token (type 2)"
            );
            assert_eq!(
                result[2].token_type, 3,
                "Line 2 should preserve old token (type 3)"
            );
        }

        // Test 2: Change at last line
        {
            let old_tokens = vec![
                make_token(0, 0, 5, 1), // line 0 - unchanged
                make_token(1, 0, 5, 2), // line 1 - unchanged
                make_token(2, 0, 5, 3), // line 2 - CHANGED (last line)
            ];

            let new_tokens = vec![
                make_token(0, 0, 5, 1),  // line 0 - same
                make_token(1, 0, 5, 2),  // line 1 - same
                make_token(2, 0, 5, 13), // line 2 - new
            ];

            let mut changed_lines = std::collections::HashSet::new();
            changed_lines.insert(2); // Last line changed

            let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, 0);

            assert_eq!(result.len(), 3, "Should have 3 tokens");
            assert_eq!(
                result[0].token_type, 1,
                "Line 0 should preserve old token (type 1)"
            );
            assert_eq!(
                result[1].token_type, 2,
                "Line 1 should preserve old token (type 2)"
            );
            assert_eq!(
                result[2].token_type, 13,
                "Line 2 should use new token (type 13)"
            );
        }

        // Test 3: Line insertion at document start (new line 0)
        {
            let old_tokens = vec![
                make_token(0, 0, 5, 1), // old line 0 -> new line 1
                make_token(1, 0, 5, 2), // old line 1 -> new line 2
            ];

            let new_tokens = vec![
                make_token(0, 0, 5, 10), // new line 0 (inserted)
                make_token(1, 0, 5, 1),  // new line 1 (was old line 0)
                make_token(2, 0, 5, 2),  // new line 2 (was old line 1)
            ];

            let mut changed_lines = std::collections::HashSet::new();
            changed_lines.insert(0); // Line 0 is where insertion happened

            let result = merge_tokens(&old_tokens, &new_tokens, &changed_lines, 1);

            assert_eq!(result.len(), 3, "Should have 3 tokens after insertion");
            assert_eq!(
                result[0].token_type, 10,
                "Line 0 should be new inserted token (type 10)"
            );
            // Old tokens should be shifted
            assert_eq!(result[1].line, 1, "Old line 0 should shift to line 1");
            assert_eq!(result[2].line, 2, "Old line 1 should shift to line 2");
        }
    }

    #[test]
    fn test_incremental_tokenization_performance() {
        use std::time::Instant;

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();

        // Generate a ~1000 line Rust file
        let mut code_lines: Vec<String> = vec!["fn main() {".to_string()];
        for i in 0..998 {
            code_lines.push(format!("    let x{} = {};", i, i));
        }
        code_lines.push("}".to_string());
        let old_code = code_lines.join("\n");

        // Parse original
        let old_tree = parser.parse(&old_code, None).unwrap();

        // Simulate old tokens (just a few for the test)
        let old_tokens: Vec<AbsoluteToken> = (0..1000).map(|i| make_token(i, 4, 3, 1)).collect();

        // Make a small edit on line 500
        code_lines[500] = "    let modified = 999;".to_string();
        let new_code = code_lines.join("\n");

        // Parse new version
        let new_tree = parser.parse(&new_code, None).unwrap();

        // Simulate new tokens
        let new_tokens: Vec<AbsoluteToken> = (0..1000)
            .map(|i| {
                if i == 500 {
                    make_token(i, 4, 8, 1) // "modified" has different length
                } else {
                    make_token(i, 4, 3, 1)
                }
            })
            .collect();

        // Benchmark incremental tokenization
        let start = Instant::now();
        let _result = compute_incremental_tokens(
            &old_tokens,
            &old_tree,
            &new_tree,
            &old_code,
            &new_code,
            &new_tokens,
        );
        let duration = start.elapsed();

        // Assert that incremental tokenization is fast (< 20ms target)
        // Note: This test verifies the merge logic is fast; full integration
        // with actual token computation will have different characteristics
        assert!(
            duration.as_millis() < 20,
            "Incremental tokenization should complete in <20ms, took {}ms",
            duration.as_millis()
        );

        println!(
            "Incremental tokenization for 1000-line file completed in {:?}",
            duration
        );
    }
}
