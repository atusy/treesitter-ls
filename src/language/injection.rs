use crate::language::predicate_accessor::{get_all_predicates, UnifiedPredicate};
use tree_sitter::{Node, Query, QueryCursor, QueryMatch, StreamingIterator};

/// Represents offset adjustments for injection content boundaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InjectionOffset {
    pub start_row: i32,
    pub start_column: i32,
    pub end_row: i32,
    pub end_column: i32,
}

impl InjectionOffset {
    /// Create a new InjectionOffset
    pub fn new(start_row: i32, start_column: i32, end_row: i32, end_column: i32) -> Self {
        Self {
            start_row,
            start_column,
            end_row,
            end_column,
        }
    }

    /// Check if this offset has any non-zero values
    pub fn has_offset(&self) -> bool {
        self.start_row != 0 || self.start_column != 0 || self.end_row != 0 || self.end_column != 0
    }

    /// Convert to tuple for backwards compatibility
    pub fn as_tuple(&self) -> (i32, i32, i32, i32) {
        (
            self.start_row,
            self.start_column,
            self.end_row,
            self.end_column,
        )
    }
}

/// Default offset with no adjustments
pub const DEFAULT_OFFSET: InjectionOffset = InjectionOffset {
    start_row: 0,
    start_column: 0,
    end_row: 0,
    end_column: 0,
};

/// Parses offset directive from query and returns the offset values if found
/// Returns None if no #offset! directive exists for @injection.content
pub fn parse_offset_directive(query: &Query) -> Option<InjectionOffset> {
    // Check all patterns in the query
    for pattern_index in 0..query.pattern_count() {
        // Use unified accessor for predicates
        for predicate in get_all_predicates(query, pattern_index) {
            // Check if this is an offset! directive
            if predicate.operator() == "offset!"
                && let UnifiedPredicate::General(pred) = predicate
            {
                // Check if it applies to @injection.content capture
                if let Some(tree_sitter::QueryPredicateArg::Capture(capture_id)) = pred.args.first()
                {
                    // Find the capture name
                    if let Some(capture_name) = query.capture_names().get(*capture_id as usize)
                        && *capture_name == "injection.content"
                    {
                        // Parse the 4 numeric arguments after the capture
                        // Format: (#offset! @injection.content start_row start_col end_row end_col)
                        if pred.args.len() >= 5 {
                            // Try to parse each argument as i32
                            let parse_arg = |idx: usize| -> Option<i32> {
                                if let Some(tree_sitter::QueryPredicateArg::String(s)) =
                                    pred.args.get(idx)
                                {
                                    s.parse().ok()
                                } else {
                                    None
                                }
                            };

                            // Parse all 4 offset values
                            if let (
                                Some(start_row),
                                Some(start_col),
                                Some(end_row),
                                Some(end_col),
                            ) = (parse_arg(1), parse_arg(2), parse_arg(3), parse_arg(4))
                            {
                                return Some(InjectionOffset::new(
                                    start_row, start_col, end_row, end_col,
                                ));
                            }
                        }
                        // If parsing fails, return default offset
                        return Some(DEFAULT_OFFSET);
                    }
                }
            }
        }
    }
    None
}


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
        .map(|(hierarchy, _)| hierarchy)
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
    // Use unified accessor to check property settings
    for predicate in get_all_predicates(query, match_.pattern_index) {
        if let UnifiedPredicate::Property(prop) = predicate
            && prop.key.as_ref() == "injection.language"
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

/// Detects injection and returns both the language and the content node
pub fn detect_injection_with_content<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<(Vec<String>, Node<'a>)> {
    let injections = collect_injection_regions(node, root, text, injection_query)?;

    if injections.is_empty() {
        return None;
    }

    // Sort injections by their range (outer to inner)
    let mut sorted_injections = injections;
    sorted_injections.sort_by(|a, b| {
        // Sort by start byte (ascending), then by end byte (descending)
        // This ensures outer injections come before inner ones
        a.0.cmp(&b.0).then(b.1.cmp(&a.1))
    });

    // Build the language hierarchy from outermost to innermost
    let mut hierarchy = vec![base_language.to_string()];
    for (_, _, lang, _) in &sorted_injections {
        hierarchy.push(lang.clone());
    }

    // Return the innermost content node
    let innermost_node = sorted_injections.last().map(|(_, _, _, node)| *node)?;

    Some((hierarchy, innermost_node))
}

/// Collects all injection regions that contain the given node
fn collect_injection_regions<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
) -> Option<Vec<(usize, usize, String, Node<'a>)>> {
    let query = injection_query?;

    // Run the query on the entire tree
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    // Collect all injection regions that contain our node
    // Use a map to deduplicate by node range (start, end)
    let mut injections_map = std::collections::HashMap::new();

    while let Some(match_) = matches.next() {
        if let Some((content_node, language)) =
            find_injection_content_and_language(node, match_, query, text)
        {
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

/// Finds the injection content node and language if the given node is within it
fn find_injection_content_and_language<'a>(
    node: &Node<'a>,
    match_: &QueryMatch<'_, 'a>,
    query: &Query,
    text: &str,
) -> Option<(Node<'a>, String)> {
    // Find @injection.content capture
    for capture in match_.captures {
        if let Some(capture_name) = query.capture_names().get(capture.index as usize)
            && *capture_name == "injection.content"
        {
            let content_node = capture.node;

            // Check if our node is within this injection region
            if is_node_within(node, &content_node) {
                // Extract the injection language
                if let Some(language) = extract_injection_language(query, match_, text) {
                    return Some((content_node, language));
                }
            }
        }
    }

    None
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
    fn test_detect_nested_injections() {
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
        let (hierarchy, _content_node) = result.unwrap();

        // Should detect rust -> markdown hierarchy
        assert_eq!(hierarchy, vec!["rust", "markdown"]);
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
        let (hierarchy, _) = result.unwrap();
        assert_eq!(hierarchy, vec!["rust", "nested_lang"]);

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
        let (hierarchy, _) = result.unwrap();
        // Should only use the first matching pattern, not both
        assert_eq!(
            hierarchy,
            vec!["rust", "doc"],
            "Should only show first injection"
        );
    }

    // Helper function to find a node at a specific byte position
    fn find_node_at_byte<'a>(root: &Node<'a>, byte: usize) -> Option<Node<'a>> {
        root.descendant_for_byte_range(byte, byte)
    }
}
