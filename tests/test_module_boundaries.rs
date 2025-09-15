// Test that LanguageLayer is accessible from document module
use treesitter_ls::document::LanguageLayer;

#[test]
fn test_language_layer_should_be_in_document_module() {
    // Red: LanguageLayer should be in document module
    // LanguageLayer is now correctly in document module

    // Create a simple tree for testing
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).unwrap();
    let tree = parser.parse("fn main() {}", None).unwrap();

    // LanguageLayer should be constructible from document module
    let _layer = LanguageLayer::root("rust".to_string(), tree);
}
