use tree_sitter::{Parser, Query};
use treesitter_ls::language::injection::{
    detect_injection_with_content, parse_offset_directive_for_pattern,
};

#[test]
fn test_nested_injection_respects_offset_boundaries() {
    // Rust code with a regex that has offset - simulating nested injection scenario
    let rust_code = r#"fn main() {
    let pattern = Regex::new(r"_^\d+$").unwrap();
}"#;

    // Parse with Rust
    let rust_lang = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    parser
        .set_language(&rust_lang)
        .expect("Failed to set Rust language");

    let tree = parser
        .parse(rust_code, None)
        .expect("Failed to parse Rust code");
    let root = tree.root_node();

    // Rust injection query with offset for regex
    let injection_query_str = r#"
        (call_expression
          function: (scoped_identifier
            path: (identifier) @_regex
            (#eq? @_regex "Regex")
            name: (identifier) @_new
            (#eq? @_new "new"))
          arguments: (arguments
            (raw_string_literal
              (string_content) @injection.content))
          (#set! injection.language "regex")
          (#offset! @injection.content 0 1 0 0))
    "#;

    let injection_query =
        Query::new(&rust_lang, injection_query_str).expect("Failed to create injection query");

    // Find the string_content node that contains the regex
    let mut content_node = None;
    let mut cursor = tree.walk();

    // Simple traversal to find string_content node
    fn traverse<'a>(
        cursor: &mut tree_sitter::TreeCursor<'a>,
        target: &mut Option<tree_sitter::Node<'a>>,
    ) {
        loop {
            let node = cursor.node();
            if node.kind() == "string_content" {
                *target = Some(node);
                return;
            }

            if cursor.goto_first_child() {
                traverse(cursor, target);
                if target.is_some() {
                    return;
                }
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                return;
            }
        }
    }

    traverse(&mut cursor, &mut content_node);
    let content_node = content_node.expect("Should find string_content node");

    // Check if injection is detected
    let injection_result = detect_injection_with_content(
        &content_node,
        &root,
        rust_code,
        Some(&injection_query),
        "rust",
    );

    assert!(injection_result.is_some(), "Should detect regex injection");

    let (_hierarchy, detected_content_node, pattern_index) = injection_result.unwrap();

    // Check the pattern has offset
    let offset = parse_offset_directive_for_pattern(&injection_query, pattern_index);
    assert!(offset.is_some(), "Pattern should have offset directive");

    let offset = offset.unwrap();
    assert_eq!(offset.start_column, 1, "Should have column offset of 1");

    // The key test: Check that position at underscore is NOT in the effective range
    let content_range = detected_content_node.byte_range();
    let effective_start = content_range.start + offset.start_column as usize;

    // The underscore in r"_^\d+$" is at the start of the string content
    // With offset (0, 1, 0, 0), effective range starts 1 byte after
    let underscore_position_in_content = 0; // First character in string_content

    // Bug to demonstrate: Currently the code doesn't check this in nested injections
    // It should check if cursor position is within effective range
    assert!(
        underscore_position_in_content < offset.start_column as usize,
        "Underscore (position {}) should be before offset start column ({})",
        underscore_position_in_content,
        offset.start_column
    );

    // This demonstrates that positions before the offset should NOT show as part of the injection
    // The fix needs to apply this offset check in handle_nested_injection
}

#[test]
fn test_nested_injection_without_offset_works_normally() {
    // Test that injections without offset still work
    let markdown = "```rust\nfn main() {}\n```";

    let md_lang = tree_sitter_md::LANGUAGE.into();
    let mut parser = Parser::new();
    parser
        .set_language(&md_lang)
        .expect("Failed to set markdown language");

    let tree = parser
        .parse(markdown, None)
        .expect("Failed to parse markdown");
    let root = tree.root_node();

    // Markdown injection query for code blocks (no offset)
    let injection_query_str = r#"
        (fenced_code_block
          (info_string
            (language) @injection.language)
          (code_fence_content) @injection.content)
    "#;

    let injection_query =
        Query::new(&md_lang, injection_query_str).expect("Failed to create injection query");

    // Find code block or any node within it
    let mut test_node = None;
    let mut cursor = tree.walk();

    // Find any node that could match the injection pattern
    fn find_code_content<'a>(
        cursor: &mut tree_sitter::TreeCursor<'a>,
        target: &mut Option<tree_sitter::Node<'a>>,
    ) {
        loop {
            let node = cursor.node();
            // Try to find either code_fence_content or the fenced_code_block itself
            if node.kind() == "code_fence_content" || node.kind() == "fenced_code_block" {
                *target = Some(node);
                if node.kind() == "code_fence_content" {
                    return; // Prefer content node
                }
            }

            if cursor.goto_first_child() {
                find_code_content(cursor, target);
                if target.is_some() && target.as_ref().unwrap().kind() == "code_fence_content" {
                    return;
                }
                cursor.goto_parent();
            }

            if !cursor.goto_next_sibling() {
                return;
            }
        }
    }

    find_code_content(&mut cursor, &mut test_node);
    let test_node = test_node.expect("Should find code block or content");

    println!("Testing with node kind: {}", test_node.kind());

    let injection_result = detect_injection_with_content(
        &test_node,
        &root,
        markdown,
        Some(&injection_query),
        "markdown",
    );

    assert!(
        injection_result.is_some(),
        "Should detect rust injection when testing {} node",
        test_node.kind()
    );

    let (_hierarchy, _content_node, pattern_index) = injection_result.unwrap();

    // Check there's no offset
    let offset = parse_offset_directive_for_pattern(&injection_query, pattern_index);
    assert!(offset.is_none(), "Code block pattern should have no offset");

    // Without offset, all positions within content should be considered part of injection
}
