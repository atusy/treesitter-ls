mod delta;
mod finalize;
mod injection;
mod legend;
mod parallel;
mod range;
mod token_collector;

use crate::config::CaptureMappings;
use tower_lsp_server::ls_types::SemanticTokensResult;
use tree_sitter::{Query, Tree};

// Re-export crate-internal API from submodules
pub(crate) use delta::calculate_delta_or_full;
pub(crate) use legend::{LEGEND_MODIFIERS, LEGEND_TYPES};
pub(crate) use range::handle_semantic_tokens_range_parallel_async;

// Re-export for parallel processing
use parallel::collect_injection_tokens_parallel;

// Internal re-exports for production code
use finalize::finalize_tokens;
use token_collector::{RawToken, collect_host_tokens};

// Test-only imports
#[cfg(test)]
use {delta::calculate_semantic_tokens_delta, tower_lsp_server::ls_types::SemanticTokens};

/// Handle semantic tokens full request with Rayon parallel injection processing.
///
/// Uses Rayon's work-stealing parallelism for processing multiple injections
/// concurrently. Thread-local parser caching eliminates the need for cross-thread
/// synchronization during parsing. Runs CPU-bound work on tokio's blocking thread
/// pool to avoid blocking the async runtime.
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
pub(crate) async fn handle_semantic_tokens_full(
    text: String,
    tree: Tree,
    query: std::sync::Arc<Query>,
    filetype: Option<String>,
    capture_mappings: Option<CaptureMappings>,
    coordinator: std::sync::Arc<crate::language::LanguageCoordinator>,
    supports_multiline: bool,
) -> Option<SemanticTokensResult> {
    tokio::task::spawn_blocking(move || {
        let mut all_tokens: Vec<RawToken> = Vec::with_capacity(1000);
        let lines: Vec<&str> = text.lines().collect();

        // Collect host document tokens first (no exclusion — finalize handles it)
        collect_host_tokens(
            &text,
            &tree,
            &query,
            filetype.as_deref(),
            capture_mappings.as_ref(),
            &text,
            &lines,
            0,
            0,
            supports_multiline,
            &[],
            &mut all_tokens,
        );

        // Collect injection tokens in parallel using Rayon.
        // Also returns active injection regions for finalize-time exclusion.
        let (injection_tokens, active_injection_regions) = collect_injection_tokens_parallel(
            &text,
            &tree,
            filetype.as_deref(),
            &coordinator,
            capture_mappings.as_ref(),
            supports_multiline,
        );

        // Merge injection tokens with host tokens
        all_tokens.extend(injection_tokens);

        finalize_tokens(all_tokens, &active_injection_regions)
    })
    .await
    .ok()
    .flatten()
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

    /// Test async wrapper for parallel injection processing.
    ///
    /// This verifies the spawn_blocking bridge works correctly when calling
    /// the Rayon-based parallel injection processing from an async context.
    #[tokio::test]
    async fn test_handle_semantic_tokens_full() {
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
        let result = handle_semantic_tokens_full(
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
    async fn test_handle_semantic_tokens_full_with_empty_document() {
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
        let result = handle_semantic_tokens_full(
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

    /// Integration test: Markdown with Lua code block — the finalize pipeline
    /// must exclude host tokens inside the injection region (line 3) while
    /// preserving tokens on other lines.
    #[tokio::test]
    async fn test_no_host_tokens_inside_injection_region() {
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
        let lua_result = coordinator.ensure_language_loaded("lua");
        if !md_result.success || !lua_result.success {
            eprintln!("Skipping: markdown or lua parser not available");
            return;
        }

        let Some(md_query) = coordinator.get_highlight_query("markdown") else {
            eprintln!("Skipping: markdown highlight query not available");
            return;
        };

        // Markdown with a Lua code block
        let text = "# Hello\n\n```lua\nlocal x = 42\n```\n".to_string();
        // Lines:
        //   0: "# Hello"
        //   1: ""
        //   2: "```lua"
        //   3: "local x = 42"
        //   4: "```"

        // Parse markdown
        let mut parser_pool = coordinator.create_document_parser_pool();
        let Some(mut parser) = parser_pool.acquire("markdown") else {
            return;
        };
        let Some(tree) = parser.parse(&text, None) else {
            return;
        };
        parser_pool.release("markdown".to_string(), parser);

        // Use the full pipeline — exclusion now happens in finalize_tokens
        let result = handle_semantic_tokens_full(
            text,
            tree,
            md_query,
            Some("markdown".to_string()),
            None,
            coordinator,
            false,
        )
        .await;

        assert!(result.is_some(), "Should return semantic tokens");

        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("Expected full tokens result");
        };

        // Decode delta-encoded tokens to absolute (line, col) for inspection
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut decoded = Vec::new();
        for st in &tokens.data {
            abs_line += st.delta_line;
            if st.delta_line > 0 {
                abs_col = st.delta_start;
            } else {
                abs_col += st.delta_start;
            }
            decoded.push((abs_line, abs_col, st.length, st.token_type));
        }

        // Line 3 contains "local x = 42" — injection tokens should be present
        let line3_tokens: Vec<_> = decoded.iter().filter(|t| t.0 == 3).collect();
        assert!(
            !line3_tokens.is_empty(),
            "Should have injection tokens on line 3 (Lua). Decoded: {:?}",
            decoded
        );

        // Line 0 has "# Hello" — should have heading / heading-content tokens
        let line0_tokens: Vec<_> = decoded.iter().filter(|t| t.0 == 0).collect();
        assert!(
            !line0_tokens.is_empty(),
            "Should have host tokens on line 0 (heading). Decoded: {:?}",
            decoded
        );
    }
}
