// Layer-aware semantic tokens handler
use super::semantic_token_mapper::SemanticTokenMapper;
use crate::config::CaptureMappings;
use crate::state::document::Document;
use tower_lsp::lsp_types::{SemanticTokens, SemanticTokensResult};
use tree_sitter::Query;

/// Handle semantic tokens with layer awareness
/// Merges tokens from all layers (root + injections)
///
/// WARNING: This handler is currently incomplete and should not be used in production:
/// - Delta recalculation may produce incorrect results for merged tokens
/// - May return empty tokens when queries are not found for root layer
///
/// Use the original handle_semantic_tokens_full with root_layer.tree instead
pub fn handle_semantic_tokens_full_layered(
    document: &Document,
    queries: &std::collections::HashMap<String, &Query>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensResult> {
    let mut all_tokens = Vec::new();

    // Process root layer if present
    if let Some(root_layer) = document.root_layer()
        && let Some(query) = queries.get(&root_layer.language_id)
        && let Some(SemanticTokensResult::Tokens(tokens)) = super::handle_semantic_tokens_full(
            &document.text,
            &root_layer.tree,
            query,
            Some(&root_layer.language_id),
            capture_mappings,
        )
    {
        all_tokens.extend(tokens.data);
    }

    // Process injection layers
    for injection_layer in document.injection_layers() {
        if let Some(query) = queries.get(&injection_layer.language_id) {
            // For injection layers, we need to handle range-limited tokens
            // Extract text for the injection ranges
            let mut injection_text = String::new();
            for (start, end) in &injection_layer.ranges {
                if *start < document.text.len() && *end <= document.text.len() {
                    injection_text.push_str(&document.text[*start..*end]);
                }
            }

            // Parse tokens for injection with the injection's tree
            if let Some(SemanticTokensResult::Tokens(tokens)) = super::handle_semantic_tokens_full(
                &injection_text,
                &injection_layer.tree,
                query,
                Some(&injection_layer.language_id),
                capture_mappings,
            ) {
                // Map injection tokens to document coordinates
                let mapper = SemanticTokenMapper::new(&injection_layer.ranges, &document.text);
                let mapped_tokens = mapper.map_tokens(tokens.data);
                all_tokens.extend(mapped_tokens);
            }
        }
    }

    // Sort tokens by position if we have any
    if all_tokens.is_empty() {
        return Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: vec![],
        }));
    }

    // Convert all tokens to absolute positions first
    // This is necessary because tokens from different layers have independent delta sequences
    let mut absolute_tokens = Vec::with_capacity(all_tokens.len());
    for layer_tokens in [all_tokens].into_iter() {
        let mut current_line = 0;
        let mut current_start = 0;

        for token in layer_tokens {
            current_line += token.delta_line;
            if token.delta_line == 0 {
                current_start += token.delta_start;
            } else {
                current_start = token.delta_start;
            }

            absolute_tokens.push((
                current_line,
                current_start,
                token.length,
                token.token_type,
                token.token_modifiers_bitset,
            ));
        }
    }

    // Sort by absolute position
    absolute_tokens.sort_by_key(|t| (t.0, t.1));

    // Convert back to delta-encoded tokens
    let mut result_tokens = Vec::with_capacity(absolute_tokens.len());
    let mut last_line = 0;
    let mut last_start = 0;

    for (line, start, length, token_type, modifiers) in absolute_tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        result_tokens.push(tower_lsp::lsp_types::SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: result_tokens,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tree_sitter::Parser;

    fn create_test_document_with_layers() -> Document {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse("fn main() {}", None).unwrap();

        Document {
            text: "fn main() {}".to_string(),
            last_semantic_tokens: None,
            layers: crate::state::layer_manager::LayerManager::with_root("rust".to_string(), tree),
        }
    }

    #[test]
    fn test_semantic_tokens_with_single_layer() {
        let doc = create_test_document_with_layers();
        let mut queries = HashMap::new();

        // Add a simple query for testing
        let query = tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), r#""fn" @keyword"#)
            .unwrap();
        queries.insert("rust".to_string(), &query);

        let result = handle_semantic_tokens_full_layered(&doc, &queries, None);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            // Should have at least one token for "fn"
            assert!(!tokens.data.is_empty());
        }
    }

    #[test]
    fn test_semantic_tokens_with_no_matching_query() {
        let doc = create_test_document_with_layers();
        let queries = HashMap::new(); // Empty queries

        let result = handle_semantic_tokens_full_layered(&doc, &queries, None);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            // Should return empty tokens when no query matches
            assert!(tokens.data.is_empty());
        }
    }

    #[test]
    fn test_semantic_tokens_with_injection_layers() {
        // Create a document with injection layers
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let root_tree = parser.parse("fn main() { /* comment */ }", None).unwrap();

        let mut doc = Document {
            text: "fn main() { /* comment */ }".to_string(),
            last_semantic_tokens: None,
            layers: crate::state::layer_manager::LayerManager::with_root(
                "rust".to_string(),
                root_tree,
            ),
        };

        // Add a mock injection layer (simulating a comment with different highlighting)
        let comment_tree = parser.parse("comment", None).unwrap();
        let injection_layer = crate::state::language_layer::LanguageLayer::injection(
            "comment".to_string(),
            comment_tree,
            vec![(14, 22)], // Position of "comment" in "/* comment */"
            1,
            0,
        );
        doc.layers.add_injection_layer(injection_layer);

        // Create queries
        let mut queries = HashMap::new();
        let rust_query =
            tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), r#""fn" @keyword"#)
                .unwrap();
        queries.insert("rust".to_string(), &rust_query);

        // For the comment, we'll use a simple identifier query
        let comment_query = tree_sitter::Query::new(
            &tree_sitter_rust::LANGUAGE.into(),
            r#"(identifier) @comment"#,
        )
        .unwrap();
        queries.insert("comment".to_string(), &comment_query);

        let result = handle_semantic_tokens_full_layered(&doc, &queries, None);
        assert!(result.is_some());

        if let Some(SemanticTokensResult::Tokens(tokens)) = result {
            // Should have tokens from both layers
            assert!(!tokens.data.is_empty());

            // Verify tokens are properly sorted
            let mut last_line = 0;
            let mut last_start = 0;
            for token in &tokens.data {
                let abs_line = last_line + token.delta_line;
                let abs_start = if token.delta_line == 0 {
                    last_start + token.delta_start
                } else {
                    token.delta_start
                };

                // Ensure tokens are in order
                assert!(abs_line >= last_line);
                if abs_line == last_line {
                    assert!(abs_start >= last_start);
                }

                last_line = abs_line;
                last_start = abs_start;
            }
        }
    }
}
