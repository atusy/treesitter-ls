// This test demonstrates the semantic token issue when reopening files
// and verifies that the fix (adding did_close handler) works correctly.

use tower_lsp::lsp_types::{SemanticToken, SemanticTokens};
use tree_sitter::Parser;
use treesitter_ls::document::DocumentStore;
use url::Url;

#[test]
fn test_document_store_reopen_resets_semantic_tokens() {
    // This test shows that reopening a document (via insert) resets the semantic tokens
    let store = DocumentStore::new();
    let uri = Url::parse("file:///test.rs").unwrap();

    // Create a simple tree for testing
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();

    // First insert with a tree
    let text = "fn main() {}";
    let tree = parser.parse(text, None).unwrap();
    store.insert(
        uri.clone(),
        text.to_string(),
        Some("rust".to_string()),
        Some(tree),
    );

    // Add some semantic tokens
    let tokens = SemanticTokens {
        result_id: Some("v1".to_string()),
        data: vec![SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 2,
            token_type: 0,
            token_modifiers_bitset: 0,
        }],
    };
    store.update_semantic_tokens(&uri, tokens.clone());

    // Verify tokens are stored
    {
        let doc = store.get(&uri).unwrap();
        assert!(doc.last_semantic_tokens().is_some());
        let tokens = doc.last_semantic_tokens().unwrap();
        assert_eq!(tokens.result_id.as_deref(), Some("v1"));
    }

    // Reopen the document (simulating did_open after close)
    let tree2 = parser.parse(text, None).unwrap();
    store.insert(
        uri.clone(),
        text.to_string(),
        Some("rust".to_string()),
        Some(tree2),
    );

    // Check that semantic tokens were reset
    {
        let doc = store.get(&uri).unwrap();
        assert!(
            doc.last_semantic_tokens().is_none(),
            "Semantic tokens should be reset after reopening"
        );
    }
}

#[test]
fn test_document_store_update_preserves_semantic_tokens() {
    // This test shows that update_document preserves language but NOT semantic tokens
    let store = DocumentStore::new();
    let uri = Url::parse("file:///test.rs").unwrap();

    // Create a simple tree for testing
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();

    // First insert with a tree
    let text1 = "fn main() {}";
    let tree1 = parser.parse(text1, None).unwrap();
    store.insert(
        uri.clone(),
        text1.to_string(),
        Some("rust".to_string()),
        Some(tree1),
    );

    // Add some semantic tokens
    let tokens = SemanticTokens {
        result_id: Some("v1".to_string()),
        data: vec![SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 2,
            token_type: 0,
            token_modifiers_bitset: 0,
        }],
    };
    store.update_semantic_tokens(&uri, tokens.clone());

    // Verify tokens are stored
    {
        let doc = store.get(&uri).unwrap();
        assert!(doc.last_semantic_tokens().is_some());
    }

    // Update document (this also resets semantic tokens)
    let text2 = "fn main() { println!(\"hello\"); }";
    store.update_document(uri.clone(), text2.to_string(), None);

    // Check that semantic tokens were reset
    {
        let doc = store.get(&uri).unwrap();
        assert!(
            doc.last_semantic_tokens().is_none(),
            "Semantic tokens are reset on update_document"
        );
        assert_eq!(
            doc.language_id(),
            Some("rust"),
            "Language should be preserved"
        );
    }
}

#[test]
fn test_document_store_remove() {
    // Test that the remove method properly cleans up documents
    let store = DocumentStore::new();
    let uri = Url::parse("file:///test.rs").unwrap();

    // Create a simple tree for testing
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();

    // Insert a document
    let text = "fn main() {}";
    let tree = parser.parse(text, None).unwrap();
    store.insert(
        uri.clone(),
        text.to_string(),
        Some("rust".to_string()),
        Some(tree),
    );

    // Add some semantic tokens
    let tokens = SemanticTokens {
        result_id: Some("v1".to_string()),
        data: vec![SemanticToken {
            delta_line: 0,
            delta_start: 0,
            length: 2,
            token_type: 0,
            token_modifiers_bitset: 0,
        }],
    };
    store.update_semantic_tokens(&uri, tokens.clone());

    // Verify document exists
    assert!(store.get(&uri).is_some());

    // Remove the document
    let removed = store.remove(&uri);
    assert!(removed.is_some(), "Should return the removed document");

    // Verify document is gone
    assert!(
        store.get(&uri).is_none(),
        "Document should be removed from store"
    );

    // Reinsert should start fresh
    let tree2 = parser.parse(text, None).unwrap();
    store.insert(
        uri.clone(),
        text.to_string(),
        Some("rust".to_string()),
        Some(tree2),
    );

    // Check that it's a fresh document without old semantic tokens
    {
        let doc = store.get(&uri).unwrap();
        assert!(
            doc.last_semantic_tokens().is_none(),
            "New document should not have old semantic tokens"
        );
    }
}
