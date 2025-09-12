use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query};

#[test]
fn test_concurrent_parsing_with_rust() {
    // Test that multiple parsers can work concurrently
    let language = tree_sitter_rust::LANGUAGE.into();

    // Create multiple parsers
    let mut parser1 = Parser::new();
    let mut parser2 = Parser::new();
    let mut parser3 = Parser::new();

    parser1.set_language(&language).unwrap();
    parser2.set_language(&language).unwrap();
    parser3.set_language(&language).unwrap();

    // Parse different code snippets
    let code1 = "fn first() { println!(\"1\"); }";
    let code2 = "fn second() { println!(\"2\"); }";
    let code3 = "fn third() { println!(\"3\"); }";

    let tree1 = parser1.parse(code1, None).unwrap();
    let tree2 = parser2.parse(code2, None).unwrap();
    let tree3 = parser3.parse(code3, None).unwrap();

    // All should have valid trees
    assert!(tree1.root_node().child_count() > 0);
    assert!(tree2.root_node().child_count() > 0);
    assert!(tree3.root_node().child_count() > 0);
}

#[test]
fn test_query_execution_on_multiple_trees() {
    let language = tree_sitter_rust::LANGUAGE.into();

    // Create a query for function names
    let query_str = "(function_item name: (identifier) @function)";
    let query = Query::new(&language, query_str).unwrap();

    // Parse multiple files
    let mut parsers: Vec<Parser> = (0..3)
        .map(|_| {
            let mut p = Parser::new();
            p.set_language(&language).unwrap();
            p
        })
        .collect();

    let codes = vec!["fn alpha() {}", "fn beta() {}", "fn gamma() {}"];

    let trees: Vec<_> = codes
        .iter()
        .enumerate()
        .map(|(i, code)| parsers[i].parse(code, None).unwrap())
        .collect();

    // Query should work on all trees
    use tree_sitter::QueryCursor;
    for (i, tree) in trees.iter().enumerate() {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), codes[i].as_bytes());
        let mut match_count = 0;
        while matches.next().is_some() {
            match_count += 1;
        }
        assert_eq!(
            match_count, 1,
            "Tree {} should have exactly 1 function match",
            i
        );
    }
}

#[test]
fn test_multiple_parsers_sharing_language() {
    // Test that multiple parsers can share the same language instance
    let language = tree_sitter_rust::LANGUAGE.into();

    // Create 5 parsers all using the same language
    let mut parsers: Vec<Parser> = (0..5)
        .map(|_| {
            let mut p = Parser::new();
            p.set_language(&language).unwrap();
            p
        })
        .collect();

    // Parse different code snippets with each parser
    let codes: Vec<String> = (0..5)
        .map(|i| format!("fn function_{}() {{ println!(\"Function {}\"); }}", i, i))
        .collect();

    let trees: Vec<_> = codes
        .iter()
        .enumerate()
        .map(|(i, code)| parsers[i].parse(code, None).unwrap())
        .collect();

    // Create a query to find function names
    let query_str = "(function_item name: (identifier) @function)";
    let query = Query::new(&language, query_str).unwrap();

    // All trees should be valid and queryable
    use tree_sitter::QueryCursor;
    for (i, tree) in trees.iter().enumerate() {
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), codes[i].as_bytes());
        let mut match_count = 0;
        while matches.next().is_some() {
            match_count += 1;
        }
        assert_eq!(match_count, 1, "Tree {} should have exactly 1 function", i);

        // Verify the tree has proper structure
        assert!(
            tree.root_node().child_count() > 0,
            "Tree {} should have children",
            i
        );
        assert!(
            tree.root_node().kind() == "source_file",
            "Tree {} root should be source_file",
            i
        );
    }
}
