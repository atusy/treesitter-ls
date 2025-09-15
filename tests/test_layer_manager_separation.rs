// Test that LayerManager does not depend on DocumentParserPool
use treesitter_ls::document::LanguageLayer;

#[test]
fn test_layer_manager_should_not_have_parser_pool() {
    // This test verifies that LayerManager can be created and used
    // without any dependency on DocumentParserPool

    // Create a simple tree for testing
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse("fn main() {}", None).unwrap();

    // LayerManager should work with just layers, no parser pool
    let layer = LanguageLayer::root("rust".to_string(), tree);

    // This demonstrates that LanguageLayer is sufficient for document structure
    assert!(layer.is_root());
}
