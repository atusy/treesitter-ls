mod delta;
mod finalize;
mod injection;
mod legend;
mod parallel;
mod range;
mod token_collector;

use crate::config::CaptureMappings;
use tower_lsp_server::ls_types::{
    SemanticTokens, SemanticTokensFullDeltaResult, SemanticTokensResult,
};
use tree_sitter::{Query, Tree};

// Re-export public API from submodules
pub use delta::calculate_delta_or_full;
pub use legend::{LEGEND_MODIFIERS, LEGEND_TYPES};
pub use range::handle_semantic_tokens_range;
pub(crate) use range::handle_semantic_tokens_range_parallel_async;

// Re-export for parallel processing
use parallel::collect_injection_tokens_parallel;

// Internal re-exports for use within this module
use delta::calculate_semantic_tokens_delta;
use finalize::finalize_tokens;
use injection::{ParserProvider, collect_injection_tokens_recursive};
use token_collector::RawToken;

/// Handle semantic tokens full request
///
/// Analyzes the entire document including injected language regions and returns
/// semantic tokens for both the host document and all injected content.
/// Supports recursive/nested injections (e.g., Lua inside Markdown inside Markdown).
///
/// When coordinator or parser_pool is None, only host document tokens are returned
/// (no injection processing).
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
///
/// # Returns
/// Semantic tokens for the entire document including injected content (if coordinator/parser_pool provided)
///
/// # Note
/// This function defaults `supports_multiline` to `false` for backward compatibility.
/// Use `handle_semantic_tokens_full_with_multiline` for explicit control.
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
) -> Option<SemanticTokensResult> {
    handle_semantic_tokens_full_with_multiline(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        parser_pool,
        false, // Default to split multiline tokens for backward compatibility
    )
}

/// Handle semantic tokens full request with explicit multiline token support.
///
/// This variant allows explicit control over multiline token handling based on
/// client capabilities.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Semantic tokens for the entire document including injected content
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full_with_multiline(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
    supports_multiline: bool,
) -> Option<SemanticTokensResult> {
    // Collect all absolute tokens (line, col, length, mapped_name, depth)
    let mut all_tokens: Vec<RawToken> = Vec::with_capacity(1000);

    let lines: Vec<&str> = text.lines().collect();

    // Wrap parser_pool in ParserProvider for unified interface
    let mut provider = parser_pool.map(ParserProvider::new);

    // Recursively collect tokens from the document and all injections
    collect_injection_tokens_recursive(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        provider.as_mut(),
        text,   // host_text = text (we're at the root)
        &lines, // host_lines
        0,      // content_start_byte = 0 (we're at the root)
        0,      // depth = 0 (starting depth)
        supports_multiline,
        &mut all_tokens,
    );

    finalize_tokens(all_tokens)
}

/// Handle semantic tokens full request with Rayon parallel injection processing.
///
/// This variant uses Rayon's work-stealing parallelism for processing multiple
/// injections concurrently. Thread-local parser caching eliminates the need
/// for cross-thread synchronization during parsing.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for injection queries and language loading
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Semantic tokens for the entire document including injected content
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full_parallel(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
    coordinator: &crate::language::LanguageCoordinator,
    supports_multiline: bool,
) -> Option<SemanticTokensResult> {
    let mut all_tokens: Vec<RawToken> = Vec::with_capacity(1000);
    let lines: Vec<&str> = text.lines().collect();

    // Collect host document tokens first (not parallelized - typically fast)
    injection::collect_injection_tokens_recursive(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        None, // No coordinator for host tokens - handled separately
        None, // No parser provider - host only
        text,
        &lines,
        0,
        0,
        supports_multiline,
        &mut all_tokens,
    );

    // Collect injection tokens in parallel using Rayon
    let injection_tokens = collect_injection_tokens_parallel(
        text,
        tree,
        filetype,
        coordinator,
        capture_mappings,
        supports_multiline,
    );

    // Merge injection tokens with host tokens
    all_tokens.extend(injection_tokens);

    finalize_tokens(all_tokens)
}

/// Handle semantic tokens full request with Rayon parallel injection processing (async).
///
/// This is an async wrapper around `handle_semantic_tokens_full_parallel` that uses
/// `tokio::task::spawn_blocking` to run the CPU-bound Rayon work on a dedicated
/// thread pool, avoiding blocking the tokio runtime.
///
/// # Arguments
/// * `text` - The source text (owned for moving into spawn_blocking)
/// * `tree` - The parsed syntax tree (owned for moving into spawn_blocking)
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for injection queries and language loading
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Semantic tokens for the entire document including injected content,
/// or None if the task was cancelled or failed.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_semantic_tokens_full_parallel_async(
    text: String,
    tree: Tree,
    query: std::sync::Arc<Query>,
    filetype: Option<String>,
    capture_mappings: Option<CaptureMappings>,
    coordinator: std::sync::Arc<crate::language::LanguageCoordinator>,
    supports_multiline: bool,
) -> Option<SemanticTokensResult> {
    tokio::task::spawn_blocking(move || {
        handle_semantic_tokens_full_parallel(
            &text,
            &tree,
            &query,
            filetype.as_deref(),
            capture_mappings.as_ref(),
            &coordinator,
            supports_multiline,
        )
    })
    .await
    .ok()
    .flatten()
}

/// Handle semantic tokens full delta request with Rayon parallel injection processing (async).
///
/// This is an async version of `handle_semantic_tokens_full_delta` that uses
/// `tokio::task::spawn_blocking` to run the CPU-bound Rayon work.
///
/// # Arguments
/// * `text` - The source text (owned for moving into spawn_blocking)
/// * `tree` - The parsed syntax tree (owned for moving into spawn_blocking)
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `previous_result_id` - The result ID from the previous semantic tokens response
/// * `previous_tokens` - The previous semantic tokens to calculate delta from
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for injection queries and language loading
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Either a delta or full semantic tokens for the document,
/// or None if the task was cancelled or failed.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_semantic_tokens_full_delta_parallel_async(
    text: String,
    tree: Tree,
    query: std::sync::Arc<Query>,
    previous_result_id: String,
    previous_tokens: Option<SemanticTokens>,
    filetype: Option<String>,
    capture_mappings: Option<CaptureMappings>,
    coordinator: std::sync::Arc<crate::language::LanguageCoordinator>,
    supports_multiline: bool,
) -> Option<SemanticTokensFullDeltaResult> {
    // Get current tokens using parallel processing
    let current_result = handle_semantic_tokens_full_parallel_async(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        supports_multiline,
    )
    .await?;

    let current_tokens = match current_result {
        SemanticTokensResult::Tokens(tokens) => tokens,
        SemanticTokensResult::Partial(_) => return None,
    };

    // Check if we can calculate a delta
    if let Some(prev) = previous_tokens.as_ref()
        && prev.result_id.as_deref() == Some(&previous_result_id)
        && let Some(delta) = calculate_semantic_tokens_delta(prev, &current_tokens)
    {
        return Some(SemanticTokensFullDeltaResult::TokensDelta(delta));
    }

    // Fall back to full tokens
    Some(SemanticTokensFullDeltaResult::Tokens(current_tokens))
}

/// Handle semantic tokens full delta request
///
/// Analyzes the document and returns either a delta from the previous version
/// or the full set of semantic tokens if delta cannot be calculated.
///
/// When coordinator and parser_pool are provided, tokens from injected language
/// regions are included in the result.
///
/// # Arguments
/// * `text` - The current source text
/// * `tree` - The parsed syntax tree for the current text
/// * `query` - The tree-sitter query for semantic highlighting
/// * `previous_result_id` - The result ID from the previous semantic tokens response
/// * `previous_tokens` - The previous semantic tokens to calculate delta from
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Either a delta or full semantic tokens for the document
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full_delta(
    text: &str,
    tree: &Tree,
    query: &Query,
    previous_result_id: &str,
    previous_tokens: Option<&SemanticTokens>,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
    supports_multiline: bool,
) -> Option<SemanticTokensFullDeltaResult> {
    // Get current tokens with injection support and multiline handling
    let current_result = handle_semantic_tokens_full_with_multiline(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        parser_pool,
        supports_multiline,
    )?;
    let current_tokens = match current_result {
        SemanticTokensResult::Tokens(tokens) => tokens,
        SemanticTokensResult::Partial(_) => return None,
    };

    // Check if we can calculate a delta
    if let Some(prev) = previous_tokens
        && prev.result_id.as_deref() == Some(previous_result_id)
        && let Some(delta) = calculate_semantic_tokens_delta(prev, &current_tokens)
    {
        return Some(SemanticTokensFullDeltaResult::TokensDelta(delta));
    }

    // Fall back to full tokens
    Some(SemanticTokensFullDeltaResult::Tokens(current_tokens))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::{Range, SemanticToken};

    /// Returns the search path for tree-sitter grammars.
    /// Uses TREE_SITTER_GRAMMARS env var if set (Nix), otherwise falls back to deps/tree-sitter.
    fn test_search_path() -> String {
        std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
    }

    #[test]
    fn test_semantic_tokens_range() {
        use tower_lsp_server::ls_types::Position;

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

    /// Alias for acceptance criteria naming
    #[test]
    fn test_diff_tokens_no_change() {
        // Same as test_semantic_tokens_delta_no_changes
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
        assert_eq!(delta.unwrap().edits.len(), 0);
    }

    /// Test that suffix matching reduces delta size when change is in the middle.
    ///
    /// Scenario: 5 tokens, only the 3rd token changes length
    /// Expected: Only 1 token in the edit (the changed one), not 3 tokens
    #[test]
    fn test_diff_tokens_suffix_matching() {
        // 5 tokens on the same line (delta_line=0 for all after first)
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // This one changes
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 4,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 10,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // Changed length
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 4,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 1);

        // With suffix matching: start=2 (skip 2 prefix tokens), delete_count=1, data=1 token
        // Without suffix matching: start=2, delete_count=3, data=3 tokens
        let edit = &delta.edits[0];
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(
            edit.start, 10,
            "Should skip 2 prefix tokens (2 * 5 integers)"
        );
        assert_eq!(
            edit.delete_count, 5,
            "Should only delete 1 token (with suffix matching) = 5 integers"
        );
        assert_eq!(
            edit.data.as_ref().unwrap().len(),
            1,
            "Should only include 1 changed token"
        );
    }

    /// Test that line insertion disables suffix optimization (PBI-077 safety).
    ///
    /// When lines are inserted, tokens at the end have the same delta encoding
    /// but are at different absolute positions. We must NOT match them as suffix.
    #[test]
    fn test_diff_tokens_line_insertion_no_suffix() {
        // Before: 3 tokens on lines 0, 1, 2
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                }, // line 0
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // line 1
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // line 2
            ],
        };

        // After: 4 tokens on lines 0, 1, 2, 3 (line inserted at position 1)
        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                }, // line 0 (same)
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 5,
                    token_modifiers_bitset: 0,
                }, // line 1 (NEW)
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // line 2 (was line 1)
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // line 3 (was line 2)
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 1);

        // The last two tokens in current have SAME delta encoding as last two in previous,
        // but they're at DIFFERENT absolute positions (line 2,3 vs line 1,2).
        // Suffix optimization MUST be disabled.
        let edit = &delta.edits[0];
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(
            edit.start, 5,
            "Should skip 1 prefix token (line 0) = 5 integers"
        );
        // Without suffix: delete_count=2 (tokens at line 1,2), data=3 tokens
        // With incorrect suffix: would wrongly match last token
        assert_eq!(
            edit.delete_count, 10,
            "Should delete 2 original tokens after prefix = 10 integers"
        );
        assert_eq!(
            edit.data.as_ref().unwrap().len(),
            3,
            "Should include 3 new tokens"
        );
    }

    /// Test that same-line edits preserve suffix optimization.
    ///
    /// When editing within a line (no line count change), suffix matching is safe.
    #[test]
    fn test_diff_tokens_same_line_edit_suffix() {
        // 4 tokens all on line 0
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 3,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // This changes
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 3,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 4,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        // Second token changes length
        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 3,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 8,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // Changed
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 3,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 4,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 1);

        // Same line count, so suffix matching should work
        let edit = &delta.edits[0];
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(edit.start, 5, "Should skip 1 prefix token = 5 integers");
        assert_eq!(
            edit.delete_count, 5,
            "Should only delete 1 token (suffix matched 2) = 5 integers"
        );
        assert_eq!(
            edit.data.as_ref().unwrap().len(),
            1,
            "Should only include 1 changed token"
        );
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
        let result =
            handle_semantic_tokens_full(text, &tree, &query, Some("rust"), None, None, None);

        assert!(result.is_some());

        // Verify tokens were generated (can't inspect internals due to private type)
        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("expected complete semantic tokens result");
        };

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

    #[test]
    fn test_injection_semantic_tokens_basic() {
        // Test semantic tokens for injected Lua code in Markdown
        // example.md has a lua fenced code block at line 6 (0-indexed):
        // ```lua
        // local xyz = 12345
        // ```
        //
        // The `local` keyword should produce a semantic token at line 6, col 0
        // with token_type = keyword (1)

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool with search paths
        let coordinator = LanguageCoordinator::new();

        // Configure with search paths
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Now try to load the languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Call the injection-aware function with Some(coordinator) and Some(parser_pool)
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
        );

        // Should return some tokens
        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `local` keyword token at line 6 (0-indexed), col 0
        // SemanticToken uses delta encoding, so we need to reconstruct absolute positions
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_local_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 6 (0-indexed), col 0, keyword type (1), length 5 ("local")
            if abs_line == 6 && abs_col == 0 && token.token_type == 1 && token.length == 5 {
                found_local_keyword = true;
                break;
            }
        }

        assert!(
            found_local_keyword,
            "Should find `local` keyword token at line 6, col 0 from injected Lua code"
        );
    }

    #[test]
    fn test_nested_injection_semantic_tokens() {
        // Test semantic tokens for nested injections: Lua inside Markdown inside Markdown
        // example.md has a nested structure at lines 12-16 (1-indexed):
        // `````markdown
        // ```lua
        // local injection = true
        // ```
        // `````
        //
        // The `local` keyword at line 14 (1-indexed) / line 13 (0-indexed) should produce
        // a semantic token with token_type = keyword (1)

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool with search paths
        let coordinator = LanguageCoordinator::new();

        // Configure with search paths
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Now try to load the languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Call the injection-aware function with Some(coordinator) and Some(parser_pool)
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
        );

        // Should return some tokens
        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `local` keyword token at line 13 (0-indexed), col 0
        // This is inside the nested markdown -> lua injection
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_nested_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 13 (0-indexed), col 0, keyword type (1), length 5 ("local")
            if abs_line == 13 && abs_col == 0 && token.token_type == 1 && token.length == 5 {
                found_nested_keyword = true;
                break;
            }
        }

        assert!(
            found_nested_keyword,
            "Should find `local` keyword token at line 13, col 0 from nested Lua injection (Lua inside Markdown inside Markdown)"
        );
    }

    #[test]
    fn test_indented_injection_semantic_tokens() {
        // Test semantic tokens for indented injections: Lua in a list item with 4-space indent
        // example.md has an indented code block at lines 22-24 (1-indexed):
        // * item
        //
        //     ```lua
        //     local indent = true
        //     ```
        //
        // The `local` keyword at line 23 (1-indexed) / line 22 (0-indexed) should produce
        // a semantic token with:
        // - token_type = keyword (1)
        // - column = 4 (indented by 4 spaces)

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool with search paths
        let coordinator = LanguageCoordinator::new();

        // Configure with search paths
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Now try to load the languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Call the injection-aware function with Some(coordinator) and Some(parser_pool)
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
        );

        // Should return some tokens
        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `local` keyword token at line 22 (0-indexed), col 4
        // This is inside the indented lua code block
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_indented_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 22 (0-indexed), col 4 (indented), keyword type (1), length 5 ("local")
            if abs_line == 22 && abs_col == 4 && token.token_type == 1 && token.length == 5 {
                found_indented_keyword = true;
                break;
            }
        }

        assert!(
            found_indented_keyword,
            "Should find `local` keyword token at line 22, col 4 from indented Lua injection in list item"
        );
    }

    #[test]
    fn test_semantic_tokens_full_with_injection_none_coordinator() {
        // Test that handle_semantic_tokens_full works when
        // coordinator and parser_pool are None - it should behave like
        // the non-injection handler, returning host-only tokens.
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

        // Call the injection handler with None coordinator and parser_pool
        // This should work and return the same tokens as handle_semantic_tokens_full
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("rust"),
            None, // capture_mappings
            None, // coordinator (None = no injection support)
            None, // parser_pool (None = no injection support)
        );

        assert!(result.is_some());

        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("expected complete semantic tokens result");
        };

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

    #[test]
    fn test_semantic_tokens_range_none_coordinator() {
        // Test that handle_semantic_tokens_range works when
        // coordinator and parser_pool are None - it should behave like
        // returning host-only tokens without injection processing.
        use tower_lsp_server::ls_types::Position;
        use tree_sitter::{Parser, Query};

        let text = r#"let x = "あいうえお"
let y = "hello"
let z = 42"#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        let query_text = r#"
            "let" @keyword
            (identifier) @variable
            (string_literal) @string
            (integer_literal) @number
        "#;

        let query = Query::new(&language, query_text).unwrap();

        // Request range that includes only line 1 (0-indexed)
        let range = Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 100,
            },
        };

        // Call the handler with None coordinator and parser_pool
        let result = handle_semantic_tokens_range(
            text,
            &tree,
            &query,
            &range,
            Some("rust"),
            None,  // capture_mappings
            None,  // coordinator (None = no injection support)
            None,  // parser_pool (None = no injection support)
            false, // supports_multiline
        );

        assert!(result.is_some());

        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("expected complete semantic tokens result");
        };

        // Should have tokens only from line 1: let, y, string "hello"
        // All tokens should be on line 0 in delta encoding since we're starting fresh
        assert!(
            tokens.data.len() >= 3,
            "Expected at least 3 tokens for line 1"
        );
    }

    #[test]
    fn test_semantic_tokens_delta_with_injection() {
        // Test that handle_semantic_tokens_full_delta returns tokens from injected content
        // when coordinator and parser_pool are provided.
        //
        // example.md has a lua fenced code block at line 6 (0-indexed):
        // ```lua
        // local xyz = 12345
        // ```
        //
        // The delta handler should return the `local` keyword token from the injected Lua.

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Call delta handler with coordinator and parser_pool
        // Use empty previous tokens to get full result back
        let result = handle_semantic_tokens_full_delta(
            text,
            &tree,
            &md_highlight_query,
            "no-match", // previous_result_id that won't match
            None,       // no previous tokens
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
            false, // supports_multiline
        );

        // Should return full tokens result
        let result = result.expect("Should return tokens");
        let SemanticTokensFullDeltaResult::Tokens(tokens) = result else {
            panic!("Expected full tokens result when no previous tokens match");
        };

        // Find the `local` keyword token at line 6 (0-indexed), col 0
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_local_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 6 (0-indexed), col 0, keyword type (1), length 5 ("local")
            if abs_line == 6 && abs_col == 0 && token.token_type == 1 && token.length == 5 {
                found_local_keyword = true;
                break;
            }
        }

        assert!(
            found_local_keyword,
            "Delta handler should return `local` keyword token at line 6, col 0 from injected Lua code"
        );
    }

    #[test]
    fn test_rust_doc_comment_full_token() {
        // Rust doc comments (/// ...) include trailing newline in the tree-sitter node,
        // which causes end_pos.row > start_pos.row. This test verifies that we still
        // generate tokens for the full comment, not just the doc marker.
        use tree_sitter::{Parser, Query};

        let text = "// foo\n/// bar\n";

        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        // Query similar to the installed Rust highlights.scm
        let query_text = r#"
            [(line_comment) (block_comment)] @comment
            [(outer_doc_comment_marker) (inner_doc_comment_marker)] @comment.documentation
            (line_comment (doc_comment)) @comment.documentation
        "#;

        let query = Query::new(&language, query_text).unwrap();

        let result =
            handle_semantic_tokens_full(text, &tree, &query, Some("rust"), None, None, None);

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Should have tokens for both comments
        // Token 1: "// foo" at (0,0) len=6
        // Token 2: "/// bar" at (1,0) len=7 (without trailing newline)
        // Token 3: "/" marker at (1,2) len=1 (may be deduped with token 2)

        // Find the doc comment token at line 1, column 0
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_doc_comment_full = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 1, col 0, should have length 7 ("/// bar" without newline)
            if abs_line == 1 && abs_col == 0 && token.length == 7 {
                found_doc_comment_full = true;
                break;
            }
        }

        assert!(
            found_doc_comment_full,
            "Should find full doc comment token at line 1, col 0 with length 7. \
             Got tokens: {:?}",
            tokens.data
        );
    }

    #[test]
    fn test_multiline_token_split_when_not_supported() {
        // Test that multiline tokens are split into per-line tokens when
        // supports_multiline is false (Option A fallback behavior).
        use tree_sitter::{Parser, Query};

        // A simple markdown document with a multiline block quote
        let text = "> line1\n> line2\n> line3\n";

        let language = tree_sitter_md::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        // Query that captures block_quote as a single multiline node
        let query_text = r#"
            (block_quote) @comment
        "#;

        let query = Query::new(&language, query_text).unwrap();

        // Test with supports_multiline = false (split into per-line tokens)
        let result = handle_semantic_tokens_full_with_multiline(
            text,
            &tree,
            &query,
            Some("markdown"),
            None,
            None,
            None,
            false,
        );

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Should have 3 separate tokens, one for each line
        // Each line should be highlighted separately
        assert!(
            tokens.data.len() >= 3,
            "Expected at least 3 tokens for 3-line block quote, got {}. Tokens: {:?}",
            tokens.data.len(),
            tokens.data
        );

        // Verify tokens are on different lines
        let mut abs_line = 0u32;
        let mut lines_with_tokens = std::collections::HashSet::new();

        for token in &tokens.data {
            abs_line += token.delta_line;
            lines_with_tokens.insert(abs_line);
        }

        assert!(
            lines_with_tokens.len() >= 3,
            "Expected tokens on at least 3 different lines, got lines: {:?}",
            lines_with_tokens
        );
    }

    #[test]
    fn test_multiline_token_single_when_supported() {
        // Test that multiline tokens are emitted as single tokens when
        // supports_multiline is true (Option B primary behavior).
        use tree_sitter::{Parser, Query};

        // A simple markdown document with a multiline block quote
        let text = "> line1\n> line2\n> line3\n";

        let language = tree_sitter_md::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        // Query that captures block_quote as a single multiline node
        let query_text = r#"
            (block_quote) @comment
        "#;

        let query = Query::new(&language, query_text).unwrap();

        // Test with supports_multiline = true (emit single token)
        let result = handle_semantic_tokens_full_with_multiline(
            text,
            &tree,
            &query,
            Some("markdown"),
            None,
            None,
            None,
            true,
        );

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Should have a single token for the entire block quote
        // The token should start at line 0 and have a length that spans all lines
        assert!(
            !tokens.data.is_empty(),
            "Expected at least 1 token for multiline block quote"
        );

        // The first token should start at line 0, column 0
        let first_token = &tokens.data[0];
        assert_eq!(first_token.delta_line, 0, "First token should be on line 0");
        assert_eq!(
            first_token.delta_start, 0,
            "First token should start at column 0"
        );

        // The length should span multiple lines worth of content
        // "> line1" (7) + newline (1) + "> line2" (7) + newline (1) + "> line3" (7) = 23
        // But actual length depends on implementation details
        assert!(
            first_token.length > 7,
            "Multiline token length ({}) should be greater than single line (7)",
            first_token.length
        );
    }

    /// Test async wrapper for parallel injection processing.
    ///
    /// This verifies the spawn_blocking bridge works correctly when calling
    /// the Rayon-based parallel injection processing from an async context.
    #[tokio::test]
    async fn test_handle_semantic_tokens_full_parallel_async() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;
        use std::sync::Arc;

        // Set up coordinator with search paths
        let coordinator = Arc::new(LanguageCoordinator::new());

        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load markdown and lua languages
        let md_result = coordinator.ensure_language_loaded("markdown");
        let lua_result = coordinator.ensure_language_loaded("lua");
        if !md_result.success || !lua_result.success {
            eprintln!("Skipping: markdown or lua language parser not available");
            return;
        }

        let Some(query) = coordinator.get_highlight_query("markdown") else {
            eprintln!("Skipping: markdown highlight query not available");
            return;
        };

        // Markdown with a Lua code block
        let text = r#"# Hello

```lua
local x = 42
```
"#
        .to_string();

        // Parse the markdown document
        let mut parser_pool = coordinator.create_document_parser_pool();
        let Some(mut parser) = parser_pool.acquire("markdown") else {
            eprintln!("Skipping: could not acquire markdown parser");
            return;
        };
        let Some(tree) = parser.parse(&text, None) else {
            eprintln!("Skipping: could not parse markdown");
            return;
        };
        parser_pool.release("markdown".to_string(), parser);

        // Call the async handler
        let result = handle_semantic_tokens_full_parallel_async(
            text,
            tree,
            query,
            Some("markdown".to_string()),
            None,
            coordinator,
            false,
        )
        .await;

        // Should return tokens including injection tokens
        assert!(result.is_some(), "Should return semantic tokens");

        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("Expected full tokens result");
        };

        // Should have tokens from the Lua injection
        // Look for a keyword token (the 'local' keyword in Lua)
        let has_keyword_token = tokens.data.iter().any(|t| t.token_type == 1); // keyword = 1
        assert!(
            has_keyword_token,
            "Should have keyword tokens from Lua injection. Got {} tokens",
            tokens.data.len()
        );
    }

    /// Test that async handler returns None for empty document (consistent with sync behavior).
    #[tokio::test]
    async fn test_handle_semantic_tokens_full_parallel_async_with_empty_document() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;
        use std::sync::Arc;

        let coordinator = Arc::new(LanguageCoordinator::new());

        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        let md_result = coordinator.ensure_language_loaded("markdown");
        if !md_result.success {
            eprintln!("Skipping: markdown language parser not available");
            return;
        }

        let Some(query) = coordinator.get_highlight_query("markdown") else {
            eprintln!("Skipping: markdown highlight query not available");
            return;
        };

        // Empty document
        let text = "".to_string();

        let mut parser_pool = coordinator.create_document_parser_pool();
        let Some(mut parser) = parser_pool.acquire("markdown") else {
            return;
        };
        let Some(tree) = parser.parse(&text, None) else {
            return;
        };
        parser_pool.release("markdown".to_string(), parser);

        // Call the async handler with empty document
        let result = handle_semantic_tokens_full_parallel_async(
            text,
            tree,
            query,
            Some("markdown".to_string()),
            None,
            coordinator,
            false,
        )
        .await;

        // Empty document returns None (consistent with finalize_tokens behavior)
        assert!(
            result.is_none(),
            "Empty document should return None (no tokens to return)"
        );
    }
}
