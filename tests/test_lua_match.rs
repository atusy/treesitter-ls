use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

#[test]
fn test_lua_match_predicates() {
    // Initialize parser with a simple language (using Rust for this test)
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&language).unwrap();

    // Test code with various identifiers
    let source_code = r#"
        fn main() {
            let number = 123;
            let CONSTANT = 456;
            let mixed_Case = 789;
            let snake_case = 0;
        }
    "#;

    let tree = parser.parse(source_code, None).unwrap();
    let root_node = tree.root_node();

    // Test lua-match with digit pattern
    let query_str = r#"((integer_literal) @number (#lua-match? @number "^%d+$"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found_numbers = Vec::new();
    while let Some(match_) = matches.next() {
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found_numbers.push(text.to_string());
            }
        }
    }

    assert!(found_numbers.contains(&"123".to_string()));
    assert!(found_numbers.contains(&"456".to_string()));
    assert!(found_numbers.contains(&"789".to_string()));

    // Test lua-match with uppercase pattern
    // Note: Multi-line queries with predicates on separate lines create multiple patterns
    // We should write them on the same line for proper association
    let query_str = r#"((identifier) @constant (#lua-match? @constant "^[A-Z][A-Z_0-9]*$"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found_constants = Vec::new();
    while let Some(match_) = matches.next() {
        // Only process matches that have captures
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found_constants.push(text.to_string());
            }
        }
    }

    assert!(found_constants.contains(&"CONSTANT".to_string()));
    assert!(!found_constants.contains(&"mixed_Case".to_string()));
    assert!(!found_constants.contains(&"snake_case".to_string()));

    // Test lua-match with pattern classes
    // Note: %w in lua-pattern crate converts to [a-zA-Z0-9] without underscore
    // So we'll use a pattern that explicitly includes underscore
    let query_str = r#"((identifier) @word (#lua-match? @word "^[%w_]+$"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found_words = Vec::new();
    while let Some(match_) = matches.next() {
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found_words.push(text.to_string());
            }
        }
    }

    // All identifiers should match %w+ pattern
    assert!(found_words.contains(&"main".to_string()));
    assert!(found_words.contains(&"number".to_string()));
    assert!(found_words.contains(&"CONSTANT".to_string()));
    assert!(found_words.contains(&"mixed_Case".to_string()));
    assert!(found_words.contains(&"snake_case".to_string()));
}

#[test]
fn test_lua_match_with_anchors() {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&language).unwrap();

    let source_code = r#"
        fn test_function() {
            let x = 1;
        }
    "#;

    let tree = parser.parse(source_code, None).unwrap();
    let root_node = tree.root_node();

    // Test pattern with start anchor
    let query_str = r#"((identifier) @func (#lua-match? @func "^test"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found_funcs = Vec::new();
    while let Some(match_) = matches.next() {
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found_funcs.push(text.to_string());
            }
        }
    }

    assert!(found_funcs.contains(&"test_function".to_string()));
    assert!(!found_funcs.contains(&"x".to_string()));

    // Test pattern with end anchor
    let query_str = r#"((identifier) @func_suffix (#lua-match? @func_suffix "function$"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found_suffix = Vec::new();
    while let Some(match_) = matches.next() {
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found_suffix.push(text.to_string());
            }
        }
    }

    assert!(found_suffix.contains(&"test_function".to_string()));
}

#[test]
fn test_lua_match_quantifiers() {
    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&language).unwrap();

    let source_code = r#"
        fn main() {
            let a = 1;
            let ab = 2;
            let abc = 3;
            let abcd = 4;
        }
    "#;

    let tree = parser.parse(source_code, None).unwrap();
    let root_node = tree.root_node();

    // Test with + quantifier (one or more)
    let query_str = r#"((identifier) @multi (#lua-match? @multi "^a%l+$"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found = Vec::new();
    while let Some(match_) = matches.next() {
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found.push(text.to_string());
            }
        }
    }

    assert!(!found.contains(&"a".to_string())); // 'a' alone doesn't match a%l+ 
    assert!(found.contains(&"ab".to_string()));
    assert!(found.contains(&"abc".to_string()));
    assert!(found.contains(&"abcd".to_string()));

    // Test with * quantifier (zero or more)
    let query_str = r#"((identifier) @any (#lua-match? @any "^a%l*$"))"#;

    let query = Query::new(&language, query_str).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source_code.as_bytes());

    let mut found_any = Vec::new();
    while let Some(match_) = matches.next() {
        if !match_.captures.is_empty() {
            let filtered = treesitter_ls::treesitter::filter_captures(&query, match_, source_code);
            for capture in filtered {
                let text = &source_code[capture.node.start_byte()..capture.node.end_byte()];
                found_any.push(text.to_string());
            }
        }
    }

    assert!(found_any.contains(&"a".to_string())); // Now 'a' matches
    assert!(found_any.contains(&"ab".to_string()));
    assert!(found_any.contains(&"abc".to_string()));
    assert!(found_any.contains(&"abcd".to_string()));
}
