mod delta;
mod finalize;
mod injection;
mod legend;
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

// Internal re-exports for use within this module
use delta::calculate_semantic_tokens_delta;
use finalize::finalize_tokens;

/// Handle semantic tokens full request with parallel injection processing.
///
/// This variant uses `ConcurrentParserPool` and `JoinSet` to parse multiple
/// injection blocks concurrently, improving performance for documents with
/// many injections (e.g., Markdown with many code blocks).
///
/// Supports nested injections up to MAX_INJECTION_DEPTH (e.g., Lua inside
/// Markdown inside Markdown).
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for injection queries (Arc-wrapped for sharing across tasks)
/// * `concurrent_pool` - Concurrent parser pool for parallel parsing
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Semantic tokens for the entire document including injected content
#[allow(clippy::too_many_arguments)]
pub async fn handle_semantic_tokens_full(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: std::sync::Arc<crate::language::LanguageCoordinator>,
    concurrent_pool: &std::sync::Arc<crate::language::ConcurrentParserPool>,
    supports_multiline: bool,
) -> Option<SemanticTokensResult> {
    let all_tokens = injection::collect_injection_tokens(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        concurrent_pool,
        supports_multiline,
    )
    .await;

    finalize_tokens(all_tokens)
}

/// Handle semantic tokens full delta request with parallel injection processing.
///
/// This is the async variant that uses `ConcurrentParserPool` for parallel parsing
/// of injection blocks, including nested injections.
///
/// # Arguments
/// * `text` - The current source text
/// * `tree` - The parsed syntax tree for the current text
/// * `query` - The tree-sitter query for semantic highlighting
/// * `previous_result_id` - The result ID from the previous semantic tokens response
/// * `previous_tokens` - The previous semantic tokens to calculate delta from
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for injection queries (Arc-wrapped for sharing across tasks)
/// * `concurrent_pool` - Concurrent parser pool for parallel parsing
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Either a delta or full semantic tokens for the document
#[allow(clippy::too_many_arguments)]
pub async fn handle_semantic_tokens_full_delta(
    text: &str,
    tree: &Tree,
    query: &Query,
    previous_result_id: &str,
    previous_tokens: Option<&SemanticTokens>,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
    coordinator: std::sync::Arc<crate::language::LanguageCoordinator>,
    concurrent_pool: &std::sync::Arc<crate::language::ConcurrentParserPool>,
    supports_multiline: bool,
) -> Option<SemanticTokensFullDeltaResult> {
    // Get current tokens with parallel injection processing
    let current_result = handle_semantic_tokens_full(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        concurrent_pool,
        supports_multiline,
    )
    .await?;

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
    use tower_lsp_server::ls_types::SemanticToken;

    /// Returns the search path for tree-sitter grammars.
    /// Uses TREE_SITTER_GRAMMARS env var if set (Nix), otherwise falls back to deps/tree-sitter.
    fn test_search_path() -> String {
        std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
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

    #[tokio::test]
    async fn test_semantic_tokens_with_japanese() {
        use crate::language::LanguageCoordinator;
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

        // Use parallel handler with minimal coordinator (no injection processing needed for Rust)
        let coordinator = LanguageCoordinator::new();
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("rust"),
            None,
            coordinator,
            &concurrent_pool,
            false,
        )
        .await;

        assert!(result.is_some());

        // Verify tokens were generated
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

    #[tokio::test]
    async fn test_injection_semantic_tokens_basic() {
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

        // Use parallel handler
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    #[tokio::test]
    async fn test_nested_injection_semantic_tokens() {
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

        // Use parallel handler
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    #[tokio::test]
    async fn test_indented_injection_semantic_tokens() {
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

        // Use parallel handler
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    #[tokio::test]
    async fn test_semantic_tokens_full_minimal_coordinator() {
        // Test that handle_semantic_tokens_full works with a minimal coordinator
        // for languages without injection support (e.g., Rust).
        use crate::language::LanguageCoordinator;
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

        // Use parallel handler with minimal coordinator
        let coordinator = LanguageCoordinator::new();
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("rust"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    #[tokio::test]
    async fn test_semantic_tokens_range_minimal_coordinator() {
        // Test that handle_semantic_tokens_range works with a minimal coordinator
        // for languages without injection support.
        use crate::language::LanguageCoordinator;
        use tower_lsp_server::ls_types::{Position, Range};
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

        // Use parallel handler with minimal coordinator
        let coordinator = LanguageCoordinator::new();
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_range(
            text,
            &tree,
            &query,
            &range,
            Some("rust"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    #[tokio::test]
    async fn test_semantic_tokens_delta_with_injection() {
        // Test that handle_semantic_tokens_full_delta returns tokens from injected content.
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

        // Use parallel delta handler
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        // Use empty previous tokens to get full result back
        let result = handle_semantic_tokens_full_delta(
            text,
            &tree,
            &md_highlight_query,
            "no-match", // previous_result_id that won't match
            None,       // no previous tokens
            Some("markdown"),
            None, // capture_mappings
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    /// Test that parallel handler discovers languages only at depth > 1.
    ///
    /// Document structure:
    /// `````markdown
    /// ```rust
    /// fn main() {}
    /// ```
    /// `````
    ///
    /// "rust" is ONLY inside the nested markdown (depth 2), not at top level.
    /// The parallel handler must recursively discover and process it.
    #[tokio::test]
    async fn test_nested_only_language_parallel() {
        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Document with rust ONLY inside nested markdown (not at top level)
        let text = r#"`````markdown
```rust
fn main() {}
```
`````"#;

        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        coordinator.ensure_language_loaded("markdown");
        coordinator.ensure_language_loaded("rust");

        let mut parser_pool = coordinator.create_document_parser_pool();

        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Create ConcurrentParserPool via coordinator
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        // Use parallel handler
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None,
            coordinator,
            &concurrent_pool,
            false, // supports_multiline = false for backward compatibility in tests
        )
        .await;

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `fn` keyword token at line 2 (0-indexed), col 0
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_fn_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 2 (0-indexed), col 0, keyword type (1), length 2 ("fn")
            if abs_line == 2 && abs_col == 0 && token.token_type == 1 && token.length == 2 {
                found_fn_keyword = true;
                break;
            }
        }

        assert!(
            found_fn_keyword,
            "Should find `fn` keyword at line 2 from nested Rust injection (Markdown -> Markdown -> Rust). \
             Parallel handler must recursively discover nested languages."
        );
    }

    #[tokio::test]
    async fn test_rust_doc_comment_full_token() {
        // Rust doc comments (/// ...) include trailing newline in the tree-sitter node,
        // which causes end_pos.row > start_pos.row. This test verifies that we still
        // generate tokens for the full comment, not just the doc marker.
        use crate::language::LanguageCoordinator;
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

        // Use parallel handler
        let coordinator = LanguageCoordinator::new();
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("rust"),
            None,
            coordinator,
            &concurrent_pool,
            false, // supports_multiline
        )
        .await;

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

    #[tokio::test]
    async fn test_multiline_token_split_when_not_supported() {
        // Test that multiline tokens are split into per-line tokens when
        // supports_multiline is false (Option A fallback behavior).
        use crate::language::LanguageCoordinator;
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

        // Use parallel handler with supports_multiline = false (split into per-line tokens)
        let coordinator = LanguageCoordinator::new();
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("markdown"),
            None,
            coordinator,
            &concurrent_pool,
            false, // supports_multiline = false
        )
        .await;

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

    #[tokio::test]
    async fn test_multiline_token_single_when_supported() {
        // Test that multiline tokens are emitted as single tokens when
        // supports_multiline is true (Option B primary behavior).
        use crate::language::LanguageCoordinator;
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

        // Use parallel handler with supports_multiline = true (emit single token)
        let coordinator = LanguageCoordinator::new();
        let concurrent_pool = std::sync::Arc::new(coordinator.create_concurrent_parser_pool());
        let coordinator = std::sync::Arc::new(coordinator);

        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("markdown"),
            None,
            coordinator,
            &concurrent_pool,
            true, // supports_multiline = true
        )
        .await;

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
}
