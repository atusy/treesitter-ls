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
    let query = injection_query?;

    // Run the query on the entire tree
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    // Look for matches where our node is captured as @injection.content
    while let Some(match_) = matches.next() {
        if let Some(language) = check_injection_match(node, match_, query, text) {
            return Some(vec![base_language.to_string(), language]);
        }
    }

    None
}

/// Checks if a query match represents an injection containing the given node
fn check_injection_match(
    node: &Node,
    match_: &QueryMatch,
    query: &Query,
    text: &str,
) -> Option<String> {
    // Find @injection.content capture
    for capture in match_.captures {
        let capture_name = query.capture_names().get(capture.index as usize)?;

        if *capture_name == "injection.content" {
            let captured_node = capture.node;

            // Check if our node is within this injection region
            if is_node_within(node, &captured_node) {
                // Extract the injection language
                return extract_injection_language(query, match_, text);
            }
        }
    }

    None
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

/// Detects injection and returns both the language and the content node
pub fn detect_injection_with_content<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<(Vec<String>, Node<'a>)> {
    let query = injection_query?;

    // Run the query on the entire tree
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    // Look for matches where our node is captured as @injection.content
    while let Some(match_) = matches.next() {
        // Find @injection.content capture
        for capture in match_.captures {
            if let Some(capture_name) = query.capture_names().get(capture.index as usize) {
                if *capture_name == "injection.content" {
                    let content_node = capture.node;

                    // Check if our node is within this injection region
                    if is_node_within(node, &content_node) {
                        // Extract the injection language
                        if let Some(language) = extract_injection_language(query, match_, text) {
                            return Some((vec![base_language.to_string(), language], content_node));
                        }
                    }
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

    // Helper function to find a node at a specific byte position
    fn find_node_at_byte<'a>(root: &Node<'a>, byte: usize) -> Option<Node<'a>> {
        root.descendant_for_byte_range(byte, byte)
    }
}