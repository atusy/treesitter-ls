// POC: Layer-aware definition handler
use crate::handlers::definition::DefinitionResolver;
use crate::state::document::Document;
use tower_lsp::lsp_types::{GotoDefinitionResponse, Location, Position, Range, Url};
use tree_sitter::Query;

/// POC: Handle goto definition with layer awareness
/// This demonstrates how to refactor handlers to work with layers instead of single trees
pub fn handle_goto_definition_layered(
    resolver: &DefinitionResolver,
    document: &Document,
    position: Position,
    file_uri: Url,
) -> Option<GotoDefinitionResponse> {
    // Step 1: Convert LSP position to byte offset
    let mapper = document.position_mapper();
    let cursor_byte = mapper.position_to_byte(position)?;

    // Step 2: Find the appropriate layer at cursor position
    let layer = document.get_layer_at_position(cursor_byte)?;

    // Step 3: Get the tree and query for this layer
    let tree = &layer.tree;
    let text = &document.text;

    // Step 4: For now, assume we have a locals query
    // In real implementation, this would come from LanguageService
    // based on the layer's language_id
    let query_source = "(identifier) @local.reference"; // Placeholder
    let query = Query::new(&tree.language(), query_source).ok()?;

    // Step 5: Use existing resolver logic with the layer's tree
    let candidates = resolver.resolve_definition(text, tree, &query, cursor_byte);

    // Step 6: Convert candidates back to LSP locations
    if !candidates.is_empty() {
        let locations: Vec<Location> = candidates
            .into_iter()
            .filter_map(|candidate| {
                // Convert tree-sitter positions to LSP positions
                let start = mapper.byte_to_position(candidate.start_byte)?;
                let end = mapper.byte_to_position(candidate.end_byte)?;

                Some(Location {
                    uri: file_uri.clone(),
                    range: Range { start, end },
                })
            })
            .collect();

        if !locations.is_empty() {
            return Some(GotoDefinitionResponse::Array(locations));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::language_layer::LanguageLayer;
    use tree_sitter::Parser;

    fn create_test_document(text: &str, has_injection: bool) -> Document {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(text, None).unwrap();

        let mut doc = Document {
            text: text.to_string(),
            tree: Some(tree.clone()),
            old_tree: None,
            last_semantic_tokens: None,
            language_id: Some("rust".to_string()),
            root_layer: Some(LanguageLayer::root("rust".to_string(), tree.clone())),
            injection_layers: vec![],
            parser_pool: None,
        };

        if has_injection {
            // Simulate an injection layer (e.g., doc comment with code)
            let injection_tree = parser.parse("example", None).unwrap();
            doc.injection_layers.push(LanguageLayer {
                language_id: "rust".to_string(),
                tree: injection_tree,
                ranges: vec![(10, 20)], // Mock range
                depth: 1,
                parent_injection_node: None,
            });
        }

        doc
    }

    #[test]
    fn test_layer_aware_definition_without_injection() {
        let text = "fn main() { let x = 5; x }";
        let doc = create_test_document(text, false);

        // Verify we get the root layer
        let layer = doc.get_layer_at_position(15); // Position of 'x' in 'let x'
        assert!(layer.is_some());
        assert_eq!(layer.unwrap().language_id, "rust");
        assert_eq!(layer.unwrap().depth, 0);
    }

    #[test]
    fn test_layer_aware_definition_with_injection() {
        let text = "fn main() { /* code */ let x = 5; }";
        let doc = create_test_document(text, true);

        // Test position in injection range
        let injection_layer = doc.get_layer_at_position(15); // Within (10, 20)
        assert!(injection_layer.is_some());
        assert_eq!(injection_layer.unwrap().depth, 1);

        // Test position outside injection range
        let root_layer = doc.get_layer_at_position(25); // Outside (10, 20)
        assert!(root_layer.is_some());
        assert_eq!(root_layer.unwrap().depth, 0);
    }

    #[test]
    fn test_position_mapper_selection() {
        let text = "fn main() {}";

        // Document without injections uses SimplePositionMapper
        let doc_simple = create_test_document(text, false);
        let mapper_simple = doc_simple.position_mapper();
        let pos = Position {
            line: 0,
            character: 0,
        };
        assert!(mapper_simple.position_to_byte(pos).is_some());

        // Document with injections uses InjectionPositionMapper
        let doc_injection = create_test_document(text, true);
        let mapper_injection = doc_injection.position_mapper();
        assert!(mapper_injection.position_to_byte(pos).is_some());
    }
}
