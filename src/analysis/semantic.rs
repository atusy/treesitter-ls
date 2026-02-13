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
use parallel::{collect_injection_tokens_parallel, compute_host_exclusion_ranges};

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

        // Compute exclusion ranges from child injections so that parent captures
        // overlapping injection regions are suppressed. This prevents, e.g.,
        // Markdown's @markup.raw tokens leaking into Lua code blocks.
        let exclusion_ranges = compute_host_exclusion_ranges(
            &text,
            &tree,
            filetype.as_deref(),
            &coordinator,
        );

        // Collect host document tokens first (not parallelized - typically fast)
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
            &exclusion_ranges,
            &mut all_tokens,
        );

        // Collect injection tokens in parallel using Rayon
        let injection_tokens = collect_injection_tokens_parallel(
            &text,
            &tree,
            filetype.as_deref(),
            &coordinator,
            capture_mappings.as_ref(),
            supports_multiline,
        );

        // Merge injection tokens with host tokens
        all_tokens.extend(injection_tokens);

        finalize_tokens(all_tokens)
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

    /// Integration test: Markdown with Lua code block — host tokens must NOT
    /// appear inside the injection region. This is the core acceptance test for
    /// the exclusion ranges feature.
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
        let text = "# Hello\n\n```lua\nlocal x = 42\n```\n";
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
        let Some(tree) = parser.parse(text, None) else {
            return;
        };
        parser_pool.release("markdown".to_string(), parser);

        let lines: Vec<&str> = text.lines().collect();

        // Compute exclusion ranges from injections
        let exclusion_ranges = compute_host_exclusion_ranges(
            text,
            &tree,
            Some("markdown"),
            &coordinator,
        );

        // Collect host tokens WITH exclusion
        let mut host_tokens_with_exclusion = Vec::new();
        collect_host_tokens(
            text,
            &tree,
            &md_query,
            Some("markdown"),
            None,
            text,
            &lines,
            0,
            0,
            false,
            &exclusion_ranges,
            &mut host_tokens_with_exclusion,
        );

        // Also collect host tokens WITHOUT exclusion for comparison
        let mut host_tokens_without_exclusion = Vec::new();
        collect_host_tokens(
            text,
            &tree,
            &md_query,
            Some("markdown"),
            None,
            text,
            &lines,
            0,
            0,
            false,
            &[],
            &mut host_tokens_without_exclusion,
        );

        // The Lua code block is on line 3 ("local x = 42").
        // Without exclusion, Markdown typically produces tokens on line 3
        // (e.g., @markup.raw or similar captures for fenced code block content).
        // With exclusion, NO host tokens should appear on line 3.
        let host_tokens_on_injection_line_without_excl = host_tokens_without_exclusion
            .iter()
            .filter(|t| t.line == 3)
            .count();
        let host_tokens_on_injection_line_with_excl = host_tokens_with_exclusion
            .iter()
            .filter(|t| t.line == 3)
            .count();

        // If Markdown doesn't produce tokens on line 3 at all, the exclusion
        // is a no-op (still correct). But if it does, exclusion must suppress them.
        if host_tokens_on_injection_line_without_excl > 0 {
            assert_eq!(
                host_tokens_on_injection_line_with_excl, 0,
                "Host tokens on the Lua injection line should be suppressed by exclusion.\n\
                 Without exclusion: {} tokens on line 3\n\
                 With exclusion: {} tokens on line 3\n\
                 Exclusion ranges: {:?}\n\
                 Host tokens with exclusion: {:?}",
                host_tokens_on_injection_line_without_excl,
                host_tokens_on_injection_line_with_excl,
                exclusion_ranges,
                host_tokens_with_exclusion,
            );
        }

        // Tokens outside injection region (e.g., line 0 "# Hello") should be preserved
        let host_tokens_on_heading_line = host_tokens_with_exclusion
            .iter()
            .filter(|t| t.line == 0)
            .count();
        let host_tokens_on_heading_line_no_excl = host_tokens_without_exclusion
            .iter()
            .filter(|t| t.line == 0)
            .count();
        assert_eq!(
            host_tokens_on_heading_line, host_tokens_on_heading_line_no_excl,
            "Tokens outside injection region should be preserved"
        );
    }
}
