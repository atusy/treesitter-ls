// Layer-aware semantic tokens handler
use crate::config::CaptureMappings;
use crate::state::document::Document;
use tower_lsp::lsp_types::{SemanticTokens, SemanticTokensResult};
use tree_sitter::Query;

/// Handle semantic tokens with layer awareness
/// Merges tokens from all layers (root + injections)
pub fn handle_semantic_tokens_full_layered(
    document: &Document,
    queries: &std::collections::HashMap<String, Query>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensResult> {
    let mut all_tokens = Vec::new();
    
    // Process root layer if present
    if let Some(root_layer) = &document.root_layer
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
    for injection_layer in &document.injection_layers {
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
            if let Some(SemanticTokensResult::Tokens(_tokens)) = super::handle_semantic_tokens_full(
                &injection_text,
                &injection_layer.tree,
                query,
                Some(&injection_layer.language_id),
                capture_mappings,
            ) {
                // TODO: Adjust token positions based on injection ranges
                // For now, we skip injections to avoid position conflicts
                // This needs proper position mapping implementation
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
    
    // Sort is already handled by the original handler, but we may need to re-sort
    // after merging multiple layers
    all_tokens.sort_by_key(|t| (t.delta_line, t.delta_start));
    
    // Recalculate deltas after sorting
    let mut last_line = 0;
    let mut last_start = 0;
    
    for token in &mut all_tokens {
        let abs_line = last_line + token.delta_line;
        let abs_start = if token.delta_line == 0 {
            last_start + token.delta_start
        } else {
            token.delta_start
        };
        
        token.delta_line = abs_line - last_line;
        token.delta_start = if token.delta_line == 0 {
            abs_start - last_start
        } else {
            abs_start
        };
        
        last_line = abs_line;
        last_start = abs_start;
    }
    
    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data: all_tokens,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::language_layer::LanguageLayer;
    use std::collections::HashMap;
    use tree_sitter::Parser;
    
    fn create_test_document_with_layers() -> Document {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse("fn main() {}", None).unwrap();
        
        Document {
            text: "fn main() {}".to_string(),
            tree: Some(tree.clone()),
            old_tree: None,
            last_semantic_tokens: None,
            language_id: Some("rust".to_string()),
            root_layer: Some(LanguageLayer::root("rust".to_string(), tree)),
            injection_layers: vec![],
            parser_pool: None,
        }
    }
    
    #[test]
    fn test_semantic_tokens_with_single_layer() {
        let doc = create_test_document_with_layers();
        let mut queries = HashMap::new();
        
        // Add a simple query for testing
        let query = tree_sitter::Query::new(
            &tree_sitter_rust::LANGUAGE.into(),
            r#""fn" @keyword"#,
        ).unwrap();
        queries.insert("rust".to_string(), query);
        
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
}