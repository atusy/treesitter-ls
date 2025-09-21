use crate::language::injection_capture::{InjectionCapture, InjectionOffset};
use tree_sitter::{Node, Query, QueryCursor, QueryMatch, StreamingIterator};

/// Detects if a node is inside an injected language region using Tree-sitter injection queries.
///
/// This function uses standard Tree-sitter injection queries to detect language boundaries
/// in a completely language-agnostic way. It supports both:
/// - Static injection: `#set! injection.language "language_name"`
/// - Dynamic injection: `@injection.language` captures
///
/// # Arguments
/// * `node` - The node to check for injection
/// * `root` - The root node of the syntax tree
/// * `text` - The source text
/// * `injection_query` - The injection query for the base language
/// * `base_language` - The name of the base language
///
/// # Returns
/// A vector of language names representing the hierarchy, or None if no injection is detected.
/// For example: `["rust", "regex"]` for a regex pattern in Rust code.
pub fn detect_injection(
    node: &Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<Vec<String>> {
    detect_injection_with_content(node, root, text, injection_query, base_language)
        .map(|capture| vec![base_language.to_string(), capture.language])
}

/// Checks if a node is within the bounds of another node
fn is_node_within(node: &Node, container: &Node) -> bool {
    node.start_byte() >= container.start_byte() && node.end_byte() <= container.end_byte()
}

/// Extracts the injection language from query properties or captures
///
/// Handles two patterns:
/// 1. Static: `#set! injection.language "language_name"`
/// 2. Dynamic: `(language) @injection.language`
fn extract_injection_language(query: &Query, match_: &QueryMatch, text: &str) -> Option<String> {
    // First check for static language via #set! property
    if let Some(language) = extract_static_language(query, match_) {
        return Some(language);
    }

    // Then check for dynamic language via @injection.language capture
    extract_dynamic_language(query, match_, text)
}

/// Extracts language from #set! injection.language property
fn extract_static_language(query: &Query, match_: &QueryMatch) -> Option<String> {
    for prop in query.property_settings(match_.pattern_index) {
        if prop.key.as_ref() == "injection.language"
            && let Some(value) = &prop.value
        {
            return Some(value.as_ref().to_string());
        }
    }
    None
}

/// Extracts language from @injection.language capture
fn extract_dynamic_language(query: &Query, match_: &QueryMatch, text: &str) -> Option<String> {
    for capture in match_.captures {
        if let Some(capture_name) = query.capture_names().get(capture.index as usize)
            && *capture_name == "injection.language"
        {
            let lang_text = &text[capture.node.byte_range()];
            return Some(lang_text.to_string());
        }
    }
    None
}

/// Detects injection and returns an InjectionCapture with language hierarchy and content node
pub fn detect_injection_with_content<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<InjectionCapture> {
    detect_injection_with_content_and_offset(node, root, text, injection_query, base_language)
}

/// Detects injection with offset calculation based on language rules
/// The node parameter is used to determine where the cursor is, but we need the actual
/// cursor position which might be within the node, not at its start
pub fn detect_injection_with_content_and_offset<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<InjectionCapture> {
    // This function is called with a node at the cursor position.
    // However, we need to be more precise about the cursor position.
    // For now, we'll use the node's start byte, but ideally we'd pass
    // the exact cursor byte position.
    let cursor_byte = node.start_byte();

    detect_injection_at_byte_position_impl(cursor_byte, root, text, injection_query, base_language)
}

/// Detects injection at a specific byte position with offset calculation
pub fn detect_injection_at_cursor_with_offset<'a>(
    cursor_byte: usize,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<InjectionCapture> {
    detect_injection_at_byte_position_impl(cursor_byte, root, text, injection_query, base_language)
}

/// Internal implementation that checks if a byte position is within an injection with offset
fn detect_injection_at_byte_position_impl<'a>(
    byte_position: usize,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<InjectionCapture> {
    use crate::text::PositionMapper;

    let query = injection_query?;
    let mapper = PositionMapper::new(text);

    // Run the query on the entire tree
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    let mut injections = Vec::new();

    // Check each injection to see if the byte position falls within it (with offset)
    while let Some(match_) = matches.next() {
        // Find @injection.content capture
        for capture in match_.captures {
            if let Some(capture_name) = query.capture_names().get(capture.index as usize)
                && *capture_name == "injection.content"
            {
                let content_node = capture.node;

                // Extract the injection language
                if let Some(language) = extract_injection_language(query, match_, text) {
                    // Get the offset for this language transition
                    let offset = get_injection_offset(base_language, &language);

                    // Create a capture with offset
                    let mut injection_capture = InjectionCapture::new(
                        language.clone(),
                        content_node.start_byte()..content_node.end_byte(),
                    );
                    injection_capture.offset = offset;
                    injection_capture.text = Some(text.to_string());

                    // Check if the byte position is within the adjusted boundaries
                    if injection_capture.contains_position_with_text(byte_position, &mapper) {
                        injections.push(injection_capture);
                    }
                }
            }
        }
    }

    // Return the innermost injection (if any)
    injections.into_iter().min_by_key(|inj| {
        // Prefer smaller (more specific) injections
        inj.content_range.end - inj.content_range.start
    })
}

/// Get offset rules for specific language transitions
fn get_injection_offset(base_language: &str, injected_language: &str) -> InjectionOffset {
    match (base_language, injected_language) {
        // lua->luadoc: skip first column (the hyphen) as per lua injections.scm
        // Comment pattern: "^[-][%s]*[@|]" with offset (0, 1, 0, 0)
        ("lua", "luadoc") => (0, 1, 0, 0),
        // Default: no offset
        _ => (0, 0, 0, 0),
    }
}

/// Collects all injection regions that contain the given node (with offsets applied)
#[allow(dead_code)]
fn collect_injection_regions_with_offset<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<Vec<(usize, usize, String, Node<'a>)>> {
    let query = injection_query?;

    // Run the query on the entire tree
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    // Collect all injection regions that contain our node
    // Use a map to deduplicate by node range (start, end)
    let mut injections_map = std::collections::HashMap::new();

    while let Some(match_) = matches.next() {
        if let Some((content_node, language)) = find_injection_content_and_language_with_offset(
            node,
            match_,
            query,
            text,
            base_language,
        ) {
            let key = (content_node.start_byte(), content_node.end_byte());

            // Only keep the first injection for each unique range
            // This handles cases where multiple patterns match the same node
            injections_map.entry(key).or_insert((
                content_node.start_byte(),
                content_node.end_byte(),
                language,
                content_node,
            ));
        }
    }

    // Convert to vector
    let injections: Vec<_> = injections_map.into_values().collect();

    Some(injections)
}

/// Finds the injection content node and language if the given node is within it (with offset)
#[allow(dead_code)]
fn find_injection_content_and_language_with_offset<'a>(
    node: &Node<'a>,
    match_: &QueryMatch<'_, 'a>,
    query: &Query,
    text: &str,
    base_language: &str,
) -> Option<(Node<'a>, String)> {
    use crate::text::PositionMapper;

    // Find @injection.content capture
    for capture in match_.captures {
        if let Some(capture_name) = query.capture_names().get(capture.index as usize)
            && *capture_name == "injection.content"
        {
            let content_node = capture.node;

            // Extract the injection language first
            if let Some(language) = extract_injection_language(query, match_, text) {
                // Get the offset for this language transition
                let offset = get_injection_offset(base_language, &language);

                // If there's no offset, use the regular check
                if offset == (0, 0, 0, 0) {
                    if is_node_within(node, &content_node) {
                        return Some((content_node, language));
                    }
                } else {
                    // Apply offset and check if node overlaps with adjusted boundaries
                    let mapper = PositionMapper::new(text);

                    // Create a temporary capture to use the offset checking logic
                    let mut temp_capture = InjectionCapture::new(
                        language.clone(),
                        content_node.start_byte()..content_node.end_byte(),
                    );
                    temp_capture.offset = offset;
                    temp_capture.text = Some(text.to_string());

                    // Get the adjusted range
                    let adjusted_range = temp_capture.adjusted_range_with_text(&mapper);

                    // Check if the node overlaps with the adjusted boundaries
                    // A node overlaps if its range intersects with the adjusted range
                    let node_start = node.start_byte();
                    let node_end = node.end_byte();

                    if node_start < adjusted_range.end && node_end > adjusted_range.start {
                        return Some((content_node, language));
                    }
                }
            }
        }
    }

    None
}

/// Helper function for testing: detects injection at a specific byte position
#[cfg(test)]
fn detect_injection_at_byte_position<'a>(
    byte_position: usize,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<InjectionCapture> {
    detect_injection_at_byte_position_impl(
        byte_position,
        root,
        text,
        injection_query,
        base_language,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn create_rust_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");
        parser
    }

    fn parse_rust_code(parser: &mut Parser, code: &str) -> tree_sitter::Tree {
        parser.parse(code, None).expect("parse rust")
    }

    #[test]
    fn test_injection_with_lua_luadoc_offset() {
        use tree_sitter::Parser;

        // Test that lua->luadoc injection gets proper offset
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Using rust string to simulate lua comment "---@param"
        let text = r#"let comment = "luadoc content here";"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create a query that simulates lua->luadoc injection
        let query_str = r#"
        (string_literal
          (string_content) @injection.content
          (#set! injection.language "luadoc"))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");

        // Get a node well within the string content
        // The string content spans 15..35 ("luadoc content here")
        // With offset (0, 1, 0, 0), adjusted range is 16..35
        // Test that byte 20 (within adjusted range) detects the injection
        let result = detect_injection_at_cursor_with_offset(20, &root, text, Some(&query), "lua");

        assert!(result.is_some(), "Expected to find injection at byte 20");

        let capture = result.unwrap();

        // Verify lua->luadoc gets offset (0, 1, 0, 0) as per lua injections.scm
        assert_eq!(
            capture.offset,
            (0, 1, 0, 0),
            "lua->luadoc should have offset (0, 1, 0, 0) as per injections.scm"
        );
    }

    #[test]
    fn test_hyphen_not_in_luadoc_injection_old() {
        use tree_sitter::Parser;

        // Simulate lua comment "-@alias" where hyphen should NOT be in injection
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Using rust string to simulate the lua comment scenario
        // The content "-@alias" starts at byte 15
        let text = r#"let comment = "-@alias Table table";"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create a query that simulates lua->luadoc injection
        let query_str = r#"
        (string_literal
          (string_content) @injection.content
          (#set! injection.language "luadoc"))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");

        // With offset (0, 1, 0, 0) to skip first column:
        // Byte 15: hyphen <- NOT in injection (excluded by offset)
        // Byte 16: @ symbol <- IN injection (start of actual luadoc content)

        // Hyphen should NOT be in injection
        let cursor_byte_hyphen = 15;
        let result =
            detect_injection_at_byte_position(cursor_byte_hyphen, &root, text, Some(&query), "lua");

        assert_eq!(
            result, None,
            "Hyphen at byte 15 should NOT be detected as being in luadoc injection"
        );

        // @ symbol should be in the injection (start of actual luadoc)
        let cursor_byte_at_symbol = 16;
        let result2 = detect_injection_at_byte_position(
            cursor_byte_at_symbol,
            &root,
            text,
            Some(&query),
            "lua",
        );

        assert!(
            result2.is_some(),
            "@ symbol at byte 16 should be detected as being in luadoc injection"
        );
    }

    #[test]
    fn test_detect_injection_returns_injection_capture() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = r#"let x = "test string";"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        let query_str = r#"
        (string_literal
          (string_content) @injection.content
          (#set! injection.language "test_lang"))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");
        let node_in_string = find_node_at_byte(&root, 10).expect("node at position");

        let result =
            detect_injection_with_content(&node_in_string, &root, text, Some(&query), "rust");

        assert!(result.is_some());
        let capture = result.unwrap();

        // Check that we get an InjectionCapture with offset
        assert_eq!(capture.language, "test_lang");
        assert_eq!(capture.offset, (0, 0, 0, 0));
    }

    #[test]
    fn test_detect_nested_injections() {
        use crate::language::injection_capture::DEFAULT_OFFSET;
        use tree_sitter::Parser;

        // Simulate a markdown file with a code block
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = r#"let x = "markdown with ```lua code```";"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create a mock injection query that simulates nested injections
        let query_str = r#"
        (string_literal
          (string_content) @injection.content
          (#set! injection.language "markdown"))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");

        // Find a node within the string content
        let node_in_string = find_node_at_byte(&root, 20).expect("node at position");

        // Detect injection with content
        let result =
            detect_injection_with_content(&node_in_string, &root, text, Some(&query), "rust");

        assert!(result.is_some());
        let capture = result.unwrap();

        // Should detect markdown injection
        assert_eq!(capture.language, "markdown");
        assert_eq!(capture.offset, DEFAULT_OFFSET);
    }

    #[test]
    fn test_detect_injection_with_static_language() {
        let mut parser = create_rust_parser();
        let text = r#"fn main() { let re = Regex::new(r"^\d+$").unwrap(); }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Create a query that matches Regex::new with static language
        let query_str = r#"
            (call_expression
              function: (scoped_identifier
                path: (identifier) @_regex
                (#eq? @_regex "Regex")
                name: (identifier) @_new
                (#eq? @_new "new"))
              arguments: (arguments
                (raw_string_literal
                  (string_content) @injection.content))
              (#set! injection.language "regex"))
        "#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        // Find a node inside the regex string
        let node = find_node_at_byte(&root, 35); // Position in regex string
        assert!(node.is_some());

        let result = detect_injection(&node.unwrap(), &root, text, Some(&query), "rust");
        assert_eq!(result, Some(vec!["rust".to_string(), "regex".to_string()]));
    }

    #[test]
    fn test_detect_injection_with_no_injection() {
        let mut parser = create_rust_parser();
        let text = r#"fn main() { println!("hello"); }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Query that won't match
        let query_str = r#"
            (call_expression
              function: (identifier) @_fn
              (#eq? @_fn "nonexistent")
              (arguments) @injection.content
              (#set! injection.language "test"))
        "#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        let node = find_node_at_byte(&root, 20); // Position in string
        assert!(node.is_some());

        let result = detect_injection(&node.unwrap(), &root, text, Some(&query), "rust");
        assert_eq!(result, None);
    }

    #[test]
    fn test_detect_injection_without_query() {
        let mut parser = create_rust_parser();
        let text = r#"fn main() { }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        let node = root.child(0).unwrap();
        let result = detect_injection(&node, &root, text, None, "rust");
        assert_eq!(result, None);
    }

    #[test]
    fn test_is_node_within() {
        let mut parser = create_rust_parser();
        let text = r#"fn main() { let x = 42; }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        let outer = root.child(0).unwrap(); // function_item
        let inner = find_node_at_byte(&root, 20).unwrap(); // Some node inside

        assert!(is_node_within(&inner, &outer));
        assert!(!is_node_within(&outer, &inner));
    }

    #[test]
    fn test_recursive_injection_depth_limit() {
        // Test that we can handle multiple levels of injection
        // This is a simple test - real recursive injection happens in refactor.rs

        let mut parser = create_rust_parser();
        let text = r#"fn main() { let x = "nested"; }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Create a query that would inject strings as another language
        let query_str = r#"
        ((string_literal
          (string_content) @injection.content)
         (#set! injection.language "nested_lang"))
        "#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        let node = find_node_at_byte(&root, 22).expect("node in string");
        let result = detect_injection_with_content(&node, &root, text, Some(&query), "rust");

        assert!(result.is_some());
        let capture = result.unwrap();
        assert_eq!(capture.language, "nested_lang");

        // The actual deep recursion is tested through integration with refactor.rs
        // where handle_nested_injection recursively processes injections
    }

    #[test]
    fn test_duplicate_injections_same_node() {
        // Test that multiple injection patterns matching the same node
        // should only result in one injection (not nested)
        let mut parser = create_rust_parser();
        let text = r#"fn main() { /* comment */ }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Create a mock query that would inject the same node twice
        // This simulates what happens with luadoc -> comment
        let query_str = r#"
        ((block_comment) @injection.content
         (#set! injection.language "doc"))

        ((block_comment) @injection.content
         (#set! injection.language "comment"))
        "#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        // Find a node inside the comment
        // The injection query matches on block_comment nodes, so we need to be inside one
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&query, root, text.as_bytes());

        let mut injection_count = 0;
        while let Some(_match) = matches.next() {
            injection_count += 1;
        }

        // This should find 2 matches (both patterns match the same comment)
        assert_eq!(injection_count, 2, "Expected 2 injection patterns to match");

        // Now test our detection from inside the comment
        let node_in_comment = find_node_at_byte(&root, 14).expect("node in comment");
        let result =
            detect_injection_with_content(&node_in_comment, &root, text, Some(&query), "rust");

        // Should detect only one injection (first pattern takes precedence)
        assert!(result.is_some(), "Should find injection");
        let capture = result.unwrap();
        // Should only use the first matching pattern, not both
        assert_eq!(capture.language, "doc", "Should only show first injection");
    }

    // Helper function to find a node at a specific byte position
    fn find_node_at_byte<'a>(root: &Node<'a>, byte: usize) -> Option<Node<'a>> {
        root.descendant_for_byte_range(byte, byte)
    }
}
