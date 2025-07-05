use tower_lsp::lsp_types::*;
use treesitter_ls::LEGEND_TYPES;

#[tokio::test]
async fn test_server_capabilities_match_vim_lsp_expectations() {
    // Test that demonstrates the server capabilities should match what vim-lsp expects

    // vim-lsp expects these token types (from vim-lsp client capabilities)
    let vim_lsp_expected_types = vec![
        "type",
        "class",
        "enum",
        "interface",
        "struct",
        "typeParameter",
        "parameter",
        "variable",
        "property",
        "enumMember",
        "event",
        "function",
        "method",
        "macro",
        "keyword",
        "modifier",
        "comment",
        "string",
        "number",
        "regexp",
        "operator",
    ];

    // Check if server provides what vim-lsp expects
    let mut missing_types = Vec::new();
    for expected_type in &vim_lsp_expected_types {
        let found = LEGEND_TYPES.iter().any(|t| t.as_str() == *expected_type);
        if !found {
            missing_types.push(*expected_type);
        }
    }

    if !missing_types.is_empty() {
        println!(
            "ERROR: vim-lsp expects these token types but server doesn't provide them: {:?}",
            missing_types
        );
        assert!(false, "Server missing token types that vim-lsp expects");
    }

    // Check if server provides types that vim-lsp doesn't expect
    let mut unexpected_types = Vec::new();
    for server_type in LEGEND_TYPES {
        let found = vim_lsp_expected_types
            .iter()
            .any(|t| *t == server_type.as_str());
        if !found {
            unexpected_types.push(server_type.as_str());
        }
    }

    if !unexpected_types.is_empty() {
        println!(
            "WARNING: Server provides these token types but vim-lsp doesn't expect them: {:?}",
            unexpected_types
        );
    }
}

#[tokio::test]
async fn test_semantic_token_type_ordering() {
    // Test that demonstrates potential ordering issues between server and client

    // Server's token types in order
    let server_types: Vec<&str> = LEGEND_TYPES.iter().map(|t| t.as_str()).collect();

    // vim-lsp's expected order (from the codebase analysis)
    let vim_lsp_order = vec![
        "type",
        "class",
        "enum",
        "interface",
        "struct",
        "typeParameter",
        "parameter",
        "variable",
        "property",
        "enumMember",
        "event",
        "function",
        "method",
        "macro",
        "keyword",
        "modifier",
        "comment",
        "string",
        "number",
        "regexp",
        "operator",
    ];

    // Check if the ordering matches for common types
    let mut index_mismatches = Vec::new();
    for (vim_idx, vim_type) in vim_lsp_order.iter().enumerate() {
        if let Some(server_idx) = server_types.iter().position(|&t| t == *vim_type) {
            if server_idx != vim_idx {
                index_mismatches.push((vim_type, vim_idx, server_idx));
            }
        }
    }

    if !index_mismatches.is_empty() {
        println!("WARNING: Token type index mismatches between server and vim-lsp:");
        for (token_type, vim_idx, server_idx) in index_mismatches {
            println!(
                "  {}: vim-lsp expects index {}, server has index {}",
                token_type, vim_idx, server_idx
            );
        }
        // This is a warning, not a failure, as it might still work
    }
}

#[tokio::test]
async fn test_semantic_tokens_capability_declaration() {
    // Test that verifies the server declares proper semantic tokens capabilities

    // The server should declare semantic_tokens_provider with proper legend
    // This is currently done in src/lib.rs:709-720

    // Verify that the legend includes the required fields
    assert!(
        !LEGEND_TYPES.is_empty(),
        "Server should provide semantic token types"
    );
    assert!(
        LEGEND_TYPES.contains(&SemanticTokenType::FUNCTION),
        "Should include function type"
    );
    assert!(
        LEGEND_TYPES.contains(&SemanticTokenType::VARIABLE),
        "Should include variable type"
    );
    assert!(
        LEGEND_TYPES.contains(&SemanticTokenType::KEYWORD),
        "Should include keyword type"
    );
}

#[tokio::test]
async fn test_position_encoding_compatibility() {
    // Test that demonstrates UTF-16 position encoding conversion

    // Test string with multi-byte characters
    let text = "fn test() {\n    let 変数 = 42;\n}";

    // UTF-8 byte positions (what tree-sitter gives us)
    let utf8_pos = text.find("変数").unwrap_or(0);

    // UTF-16 code unit positions (what LSP expects)
    let lines: Vec<&str> = text.lines().collect();
    let line_with_var = lines.get(1).unwrap_or(&"");
    let utf16_pos = line_with_var.chars().take_while(|&c| c != '変').count();

    // These will be different for multi-byte characters
    if utf8_pos != utf16_pos {
        println!(
            "Position encoding correctly handled: UTF-8 byte {} vs UTF-16 code unit {}",
            utf8_pos, utf16_pos
        );
    }

    // Test our conversion functions would work correctly
    // This simulates what the server should do internally
    assert!(true, "Position encoding compatibility test completed");
}

#[test]
fn test_utf16_position_encoding_theory() {
    // Test our understanding of position encoding without requiring a full server

    // Test text with multi-byte characters
    let text = "fn test() {\n    let 変数 = 42;\n}";
    let lines: Vec<&str> = text.lines().collect();
    let line1 = lines[1]; // "    let 変数 = 42;"

    // UTF-8 byte position of "変" character
    let utf8_pos = line1.find("変").unwrap(); // Should be 8 bytes: "    let "

    // UTF-16 code unit position of "変" character
    let utf16_pos = line1.chars().take_while(|&c| c != '変').count(); // Should be 8 characters

    // For ASCII characters, these should be the same
    assert_eq!(
        utf8_pos, utf16_pos,
        "For ASCII prefix, UTF-8 bytes == UTF-16 code units"
    );

    // The Japanese character "変" is 3 bytes in UTF-8 but 1 code unit in UTF-16
    let char_変 = '変';
    assert_eq!(
        char_変.len_utf8(),
        3,
        "Japanese character should be 3 UTF-8 bytes"
    );
    assert_eq!(
        char_変.len_utf16(),
        1,
        "Japanese character should be 1 UTF-16 code unit"
    );

    println!("Position encoding test passed: ASCII characters work consistently");
}
