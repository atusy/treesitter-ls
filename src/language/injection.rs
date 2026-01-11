use crate::language::predicate_accessor::{UnifiedPredicate, get_all_predicates};
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
}

/// Default offset with no adjustments
pub const DEFAULT_OFFSET: InjectionOffset = InjectionOffset {
    start_row: 0,
    start_column: 0,
    end_row: 0,
    end_column: 0,
};

/// Parses offset directive for a specific pattern in the query.
/// Returns None if the specified pattern has no #offset! directive for @injection.content.
pub fn parse_offset_directive_for_pattern(
    query: &Query,
    pattern_index: usize,
) -> Option<InjectionOffset> {
    // Use unified accessor for predicates
    for predicate in get_all_predicates(query, pattern_index) {
        // Check if this is an offset! directive
        if predicate.operator() == "offset!"
            && let UnifiedPredicate::General(pred) = predicate
        {
            // Check if it applies to @injection.content capture
            if let Some(tree_sitter::QueryPredicateArg::Capture(capture_id)) = pred.args.first() {
                // Find the capture name
                if let Some(capture_name) = query.capture_names().get(*capture_id as usize)
                    && *capture_name == "injection.content"
                {
                    // Parse the 4 numeric arguments after the capture
                    // Format: (#offset! @injection.content start_row start_col end_row end_col)
                    let arg_count = pred.args.len();

                    // Validate argument count (should be 5: capture + 4 offsets)
                    if arg_count < 5 {
                        log::info!(
                            target: "treesitter_ls::query",
                            "Malformed #offset! directive for pattern {}: expected 4 offset values, got {}. \
                            Using default offset (0, 0, 0, 0). \
                            Correct format: (#offset! @injection.content start_row start_col end_row end_col)",
                            pattern_index,
                            arg_count - 1 // Subtract 1 for the capture argument
                        );
                        return Some(DEFAULT_OFFSET);
                    }

                    // Try to parse each argument as i32
                    let parse_arg = |idx: usize| -> Result<i32, String> {
                        if let Some(tree_sitter::QueryPredicateArg::String(s)) = pred.args.get(idx)
                        {
                            s.parse().map_err(|_| s.to_string())
                        } else {
                            Err(String::from("missing"))
                        }
                    };

                    // Parse all 4 offset values
                    let parse_results = vec![
                        (1, "start_row", parse_arg(1)),
                        (2, "start_col", parse_arg(2)),
                        (3, "end_row", parse_arg(3)),
                        (4, "end_col", parse_arg(4)),
                    ];

                    // Check if all values parsed successfully
                    let all_valid = parse_results.iter().all(|(_, _, r)| r.is_ok());

                    if all_valid {
                        // Extract the successfully parsed values
                        let values: Vec<i32> = parse_results
                            .into_iter()
                            .map(|(_, _, r)| r.unwrap())
                            .collect();

                        return Some(InjectionOffset::new(
                            values[0], values[1], values[2], values[3],
                        ));
                    } else {
                        // Log which values failed to parse
                        let error_details: Vec<String> = parse_results
                            .into_iter()
                            .filter_map(|(_, name, result)| {
                                result.err().map(|val| format!("{} = '{}'", name, val))
                            })
                            .collect();

                        log::info!(
                            target: "treesitter_ls::query",
                            "Failed to parse #offset! directive for pattern {}: invalid values [{}]. \
                            Using default offset (0, 0, 0, 0). \
                            All offset values must be integers.",
                            pattern_index,
                            error_details.join(", ")
                        );

                        return Some(DEFAULT_OFFSET);
                    }
                }
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
/// Handles three patterns:
/// 1. Static: `#set! injection.language "language_name"`
/// 2. Dynamic capture: `(language) @injection.language`
/// 3. nvim-treesitter custom: `#set-lang-from-info-string! @capture` (uses capture text as language)
fn extract_injection_language(query: &Query, match_: &QueryMatch, text: &str) -> Option<String> {
    // First check for static language via #set! property
    if let Some(language) = extract_static_language(query, match_) {
        return Some(language);
    }

    // Then check for nvim-treesitter's #set-lang-from-info-string! predicate
    if let Some(language) = extract_language_from_info_string(query, match_, text) {
        return Some(language);
    }

    // Finally check for dynamic language via @injection.language capture
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

/// Extracts language from nvim-treesitter's #set-lang-from-info-string! predicate
///
/// This is a custom nvim-treesitter predicate that uses the text of a capture
/// as the injection language. It's commonly used for markdown fenced code blocks:
///
/// ```scheme
/// (fenced_code_block
///   (info_string (language) @_lang)
///   (code_fence_content) @injection.content
///   (#set-lang-from-info-string! @_lang))
/// ```
fn extract_language_from_info_string(
    query: &Query,
    match_: &QueryMatch,
    text: &str,
) -> Option<String> {
    // Look for #set-lang-from-info-string! predicate
    for predicate in get_all_predicates(query, match_.pattern_index) {
        if predicate.operator() == "set-lang-from-info-string!"
            && let UnifiedPredicate::General(pred) = predicate
        {
            // The predicate takes a capture reference as argument
            if let Some(tree_sitter::QueryPredicateArg::Capture(capture_id)) = pred.args.first() {
                // Find the capture in the match
                for capture in match_.captures {
                    if capture.index == *capture_id {
                        // Extract the text from the captured node as the language
                        let lang_text = &text[capture.node.byte_range()];
                        // Normalize the language name (lowercase, trim)
                        let normalized = lang_text.trim().to_lowercase();
                        if !normalized.is_empty() {
                            return Some(normalized);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Represents an injection region found in the document
#[derive(Debug, Clone)]
pub struct InjectionRegionInfo<'a> {
    /// The injection language (e.g., "lua", "yaml")
    pub language: String,
    /// The content node from the injection query
    pub content_node: Node<'a>,
    /// The pattern index (for offset directive lookups)
    pub pattern_index: usize,
}

use std::ops::Range;

/// Owned injection region for caching (no lifetime dependency on parse tree)
///
/// Unlike `InjectionRegionInfo<'a>`, this struct owns all its data and can be
/// stored in caches that outlive the parse tree. Created via `from_region_info()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheableInjectionRegion {
    /// The injection language (e.g., "lua", "yaml")
    pub language: String,
    /// Byte range of the injection content in the source
    pub byte_range: Range<usize>,
    /// Line range (0-indexed, inclusive start, exclusive end)
    pub line_range: Range<u32>,
    /// Unique identifier for associating with cached tokens
    pub result_id: String,
    /// Hash of the content for stable region matching across parses.
    /// When document structure changes (e.g., header edit), byte_range shifts
    /// but content_hash stays the same for unchanged injections.
    pub content_hash: u64,
}

impl CacheableInjectionRegion {
    /// Create from an InjectionRegionInfo, extracting position data from the node
    pub fn from_region_info(info: &InjectionRegionInfo<'_>, result_id: &str, text: &str) -> Self {
        let node = &info.content_node;
        let content = &text[node.start_byte()..node.end_byte()];
        Self {
            language: info.language.clone(),
            byte_range: node.start_byte()..node.end_byte(),
            line_range: (node.start_position().row as u32)..(node.end_position().row as u32),
            result_id: result_id.to_string(),
            content_hash: Self::hash_content(content),
        }
    }

    /// Compute a simple hash of content bytes for stable matching.
    /// Uses FNV-1a for speed and good distribution.
    fn hash_content(content: &str) -> u64 {
        // FNV-1a hash
        const FNV_OFFSET: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET;
        for byte in content.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash
    }

    /// Check if a byte offset falls within this injection region's byte range.
    ///
    /// Used for determining which injection regions overlap with an edit.
    pub fn contains_byte(&self, byte: usize) -> bool {
        self.byte_range.contains(&byte)
    }

    /// Extract the injection content from the host document text.
    ///
    /// Returns the substring of `host_text` corresponding to this region's byte range.
    /// This is the "virtual document" content that would be sent to a language server.
    pub fn extract_content<'a>(&self, host_text: &'a str) -> &'a str {
        &host_text[self.byte_range.clone()]
    }

    /// Translate a host document position to a virtual document position.
    ///
    /// Subtracts the injection region's start line from the host position's line.
    /// Character (column) remains unchanged.
    pub fn translate_host_to_virtual(
        &self,
        host_pos: tower_lsp::lsp_types::Position,
    ) -> tower_lsp::lsp_types::Position {
        tower_lsp::lsp_types::Position {
            line: host_pos.line - self.line_range.start,
            character: host_pos.character,
        }
    }

    /// Translate a virtual document position back to host document position.
    ///
    /// Adds the injection region's start line to the virtual position's line.
    /// Character (column) remains unchanged.
    pub fn translate_virtual_to_host(
        &self,
        virtual_pos: tower_lsp::lsp_types::Position,
    ) -> tower_lsp::lsp_types::Position {
        tower_lsp::lsp_types::Position {
            line: virtual_pos.line + self.line_range.start,
            character: virtual_pos.character,
        }
    }
}

/// Collects all injection regions in the document
///
/// Unlike `detect_injection_with_content` which requires a specific node,
/// this function finds ALL injection regions in the entire document.
/// Used for semantic tokens to highlight all injected content.
///
/// # Arguments
/// * `root` - Root node of the document AST
/// * `text` - The document text
/// * `injection_query` - The injection query for detecting injections
///
/// # Returns
/// Vector of injection region information, or None if no query
pub fn collect_all_injections<'a>(
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
) -> Option<Vec<InjectionRegionInfo<'a>>> {
    let query = injection_query?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    // Use a map to deduplicate by content node range
    let mut injections_map = std::collections::HashMap::new();

    while let Some(match_) = matches.next() {
        // Find @injection.content capture in this match
        for capture in match_.captures {
            if let Some(capture_name) = query.capture_names().get(capture.index as usize)
                && *capture_name == "injection.content"
            {
                // Extract the injection language
                if let Some(language) = extract_injection_language(query, match_, text) {
                    let key = (capture.node.start_byte(), capture.node.end_byte());
                    injections_map.entry(key).or_insert(InjectionRegionInfo {
                        language,
                        content_node: capture.node,
                        pattern_index: match_.pattern_index,
                    });
                }
            }
        }
    }

    Some(injections_map.into_values().collect())
}

/// Detects injection and returns both the language and the content node
/// Also returns the pattern index of the innermost injection for offset lookups
pub fn detect_injection_with_content<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> Option<(Vec<String>, Node<'a>, usize)> {
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
    for (_, _, lang, _, _) in &sorted_injections {
        hierarchy.push(lang.clone());
    }

    // Return the innermost content node and its pattern index
    let (_, _, _, innermost_node, pattern_index) = sorted_injections.last().cloned()?;

    Some((hierarchy, innermost_node, pattern_index))
}

/// Represents an injection region with its metadata
type InjectionRegion<'a> = (usize, usize, String, Node<'a>, usize);

/// Collects all injection regions that contain the given node
/// Returns tuples of (start_byte, end_byte, language, content_node, pattern_index)
fn collect_injection_regions<'a>(
    node: &Node<'a>,
    root: &Node<'a>,
    text: &str,
    injection_query: Option<&Query>,
) -> Option<Vec<InjectionRegion<'a>>> {
    let query = injection_query?;

    // Run the query on the entire tree
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, *root, text.as_bytes());

    // Collect all injection regions that contain our node
    // Use a map to deduplicate by node range (start, end)
    let mut injections_map = std::collections::HashMap::new();

    while let Some(match_) = matches.next() {
        if let Some((content_node, language, pattern_index)) =
            extract_content_and_language(node, match_, query, text)
        {
            let key = (content_node.start_byte(), content_node.end_byte());

            // Only keep the first injection for each unique range
            // This handles cases where multiple patterns match the same node
            injections_map.entry(key).or_insert((
                content_node.start_byte(),
                content_node.end_byte(),
                language,
                content_node,
                pattern_index,
            ));
        }
    }

    // Convert to vector
    let injections: Vec<_> = injections_map.into_values().collect();

    Some(injections)
}

/// Extracts the injection content node and language if the given node is within it
/// Also returns the pattern index for offset lookups
fn extract_content_and_language<'a>(
    node: &Node<'a>,
    match_: &QueryMatch<'_, 'a>,
    query: &Query,
    text: &str,
) -> Option<(Node<'a>, String, usize)> {
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
                    // Return pattern index along with content node and language
                    return Some((content_node, language, match_.pattern_index));
                }
            }
        }
    }

    None
}

/// Calculate a stable region_id for an injection based on its language and position.
/// Format: `{language}-{ordinal}` where ordinal is the 0-indexed count of same-language injections.
///
/// # Arguments
/// * `injections` - All injection regions in document order
/// * `target_index` - Index of the target injection in the list
///
/// # Returns
/// A stable region_id string like "lua-0", "python-1", etc.
pub fn calculate_region_id(injections: &[InjectionRegionInfo], target_index: usize) -> String {
    let target = &injections[target_index];
    let ordinal = injections[..=target_index]
        .iter()
        .filter(|inj| inj.language == target.language)
        .count()
        - 1;
    format!("{}-{}", target.language, ordinal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn test_parse_offset_directive_for_pattern() {
        // Test that the pattern-aware function correctly returns
        // offsets only for the specific pattern

        // Create a query similar to markdown's injection.scm with multiple patterns
        let query_str = r#"
            ; Pattern 0: Raw string literals - NO OFFSET
            ((raw_string_literal) @injection.content
              (#set! injection.language "regex"))

            ; Pattern 1: Comments - HAS OFFSET
            ((line_comment) @injection.content
              (#set! injection.language "markdown")
              (#offset! @injection.content 1 0 -1 0))
        "#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        // Pattern 0 (raw_string_literal) has NO offset
        let offset_pattern_0 = parse_offset_directive_for_pattern(&query, 0);
        assert_eq!(offset_pattern_0, None, "Pattern 0 should have no offset");

        // Pattern 1 (line_comment) HAS offset
        let offset_pattern_1 = parse_offset_directive_for_pattern(&query, 1);
        assert_eq!(
            offset_pattern_1,
            Some(InjectionOffset::new(1, 0, -1, 0)),
            "Pattern 1 should have offset (1, 0, -1, 0)"
        );
    }

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
        let (hierarchy, _content_node, _pattern_index) = result.unwrap();

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

        let result =
            detect_injection_with_content(&node.unwrap(), &root, text, Some(&query), "rust");
        assert_eq!(
            result.map(|(h, _, _)| h),
            Some(vec!["rust".to_string(), "regex".to_string()])
        );
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

        let result =
            detect_injection_with_content(&node.unwrap(), &root, text, Some(&query), "rust");
        assert_eq!(result.map(|(h, _, _)| h), None);
    }

    #[test]
    fn test_detect_injection_without_query() {
        let mut parser = create_rust_parser();
        let text = r#"fn main() { }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        let node = root.child(0).unwrap();
        let result = detect_injection_with_content(&node, &root, text, None, "rust");
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
        let (hierarchy, _, _) = result.unwrap();
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
        let (hierarchy, _, _) = result.unwrap();
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

    #[test]
    fn test_offset_directive_with_non_numeric_values() {
        // Non-numeric values (foo bar baz qux) should return DEFAULT_OFFSET
        let language = tree_sitter_rust::LANGUAGE.into();
        let query_str = r#"
            ((line_comment) @injection.content
              (#set! injection.language "test")
              (#offset! @injection.content foo bar baz qux))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");
        let offset = parse_offset_directive_for_pattern(&query, 0);

        assert_eq!(
            offset,
            Some(super::DEFAULT_OFFSET),
            "Non-numeric values should return DEFAULT_OFFSET"
        );
    }

    #[test]
    fn test_offset_directive_with_missing_arguments() {
        // Missing arguments (only 2 instead of 4) should return DEFAULT_OFFSET
        let language = tree_sitter_rust::LANGUAGE.into();
        let query_str = r#"
            ((line_comment) @injection.content
              (#set! injection.language "test")
              (#offset! @injection.content 1 0))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");
        let offset = parse_offset_directive_for_pattern(&query, 0);

        assert_eq!(
            offset,
            Some(super::DEFAULT_OFFSET),
            "Missing arguments should return DEFAULT_OFFSET"
        );
    }

    #[test]
    fn test_offset_directive_with_extra_arguments() {
        // Too many arguments (5 instead of 4) should parse first 4, ignore extra
        let language = tree_sitter_rust::LANGUAGE.into();
        let query_str = r#"
            ((line_comment) @injection.content
              (#set! injection.language "test")
              (#offset! @injection.content 1 0 -1 0 5))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");
        let offset = parse_offset_directive_for_pattern(&query, 0);

        assert_eq!(
            offset,
            Some(InjectionOffset::new(1, 0, -1, 0)),
            "Extra arguments should be ignored, first 4 should be parsed"
        );
    }

    #[test]
    fn test_offset_directive_with_mixed_valid_invalid() {
        // Mixed valid/invalid values (1 invalid -1 0) should return DEFAULT_OFFSET
        let language = tree_sitter_rust::LANGUAGE.into();
        let query_str = r#"
            ((line_comment) @injection.content
              (#set! injection.language "test")
              (#offset! @injection.content 1 invalid -1 0))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");
        let offset = parse_offset_directive_for_pattern(&query, 0);

        assert_eq!(
            offset,
            Some(super::DEFAULT_OFFSET),
            "Mixed valid/invalid values should return DEFAULT_OFFSET"
        );
    }

    #[test]
    fn test_offset_directive_with_empty_args() {
        // Empty offset directive (no arguments after capture) should return DEFAULT_OFFSET
        let language = tree_sitter_rust::LANGUAGE.into();
        let query_str = r#"
            ((line_comment) @injection.content
              (#set! injection.language "test")
              (#offset! @injection.content))
        "#;

        let query = Query::new(&language, query_str).expect("valid query");
        let offset = parse_offset_directive_for_pattern(&query, 0);

        assert_eq!(
            offset,
            Some(super::DEFAULT_OFFSET),
            "Empty offset directive should return DEFAULT_OFFSET"
        );
    }

    #[test]
    fn test_cacheable_injection_region_from_region_info() {
        // Create a parser and parse some code to get a real Node
        let mut parser = create_rust_parser();
        let text = r#"fn main() { let s = "hello"; }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Create an injection query that matches the string
        let query_str = r#"
            ((string_literal) @injection.content
              (#set! injection.language "markdown"))
        "#;
        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        // Get injection regions
        let regions = collect_all_injections(&root, text, Some(&query));
        let regions = regions.expect("Should find injections");
        assert!(!regions.is_empty(), "Should find at least one injection");

        let region_info = &regions[0];

        // Convert to CacheableInjectionRegion (owned, no lifetime)
        let cacheable =
            CacheableInjectionRegion::from_region_info(region_info, "test-result-id", text);

        // Verify all fields are captured correctly
        assert_eq!(cacheable.language, "markdown");
        assert_eq!(
            cacheable.byte_range.start,
            region_info.content_node.start_byte()
        );
        assert_eq!(
            cacheable.byte_range.end,
            region_info.content_node.end_byte()
        );
        assert_eq!(
            cacheable.line_range.start,
            region_info.content_node.start_position().row as u32
        );
        assert_eq!(
            cacheable.line_range.end,
            region_info.content_node.end_position().row as u32
        );
        assert_eq!(cacheable.result_id, "test-result-id");
    }

    #[test]
    fn test_cacheable_injection_region_contains_byte() {
        let region = CacheableInjectionRegion {
            language: "lua".to_string(),
            byte_range: 100..200,
            line_range: 5..10,
            result_id: "test-region".to_string(),
            content_hash: 12345,
        };

        // Byte within range
        assert!(
            region.contains_byte(100),
            "Start of range should be included"
        );
        assert!(
            region.contains_byte(150),
            "Middle of range should be included"
        );
        assert!(
            region.contains_byte(199),
            "End-1 of range should be included"
        );

        // Byte outside range
        assert!(
            !region.contains_byte(99),
            "Before range should not be included"
        );
        assert!(
            !region.contains_byte(200),
            "End of range (exclusive) should not be included"
        );
        assert!(
            !region.contains_byte(300),
            "Far after range should not be included"
        );
    }

    #[test]
    fn test_cacheable_injection_region_extract_content() {
        // Simulate a Markdown document with a Rust code block
        let host_text =
            "# Title\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\nMore text";
        //                0123456789...
        // The code block content starts at byte 17 (after "```rust\n") and ends at byte 54

        let region = CacheableInjectionRegion {
            language: "rust".to_string(),
            byte_range: 17..54, // "fn main() {\n    println!(\"hello\");\n}"
            line_range: 3..6,
            result_id: "test-region".to_string(),
            content_hash: 12345,
        };

        let content = region.extract_content(host_text);
        assert_eq!(content, "fn main() {\n    println!(\"hello\");\n}\n");
    }

    #[test]
    fn test_cacheable_injection_region_translate_host_to_virtual() {
        use tower_lsp::lsp_types::Position;

        // Markdown document structure:
        // Line 0: "# Title"
        // Line 1: ""
        // Line 2: "```rust"
        // Line 3: "fn example() {"      <- injection starts here (line_range.start = 3)
        // Line 4: "    println!(...);"
        // Line 5: "}"
        // Line 6: ""
        // Line 7: "fn main() {"
        // Line 8: "    example();"       <- cursor here, host line 8
        // Line 9: "}"
        // Line 10: "```"

        let region = CacheableInjectionRegion {
            language: "rust".to_string(),
            byte_range: 17..100,
            line_range: 3..10, // Code block content spans lines 3-9
            result_id: "test-region".to_string(),
            content_hash: 12345,
        };

        // Host position: line 8, character 4 (cursor on "example" call)
        let host_pos = Position {
            line: 8,
            character: 4,
        };

        // Virtual position should be: line 5 (8 - 3 = 5), character 4
        let virtual_pos = region.translate_host_to_virtual(host_pos);

        assert_eq!(
            virtual_pos.line, 5,
            "Line should be host_line - region_start_line"
        );
        assert_eq!(
            virtual_pos.character, 4,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn test_cacheable_injection_region_translate_virtual_to_host() {
        use tower_lsp::lsp_types::Position;

        // Same scenario as translate_host_to_virtual test, but reverse direction
        // Injection region starts at line 3 in host document

        let region = CacheableInjectionRegion {
            language: "rust".to_string(),
            byte_range: 17..100,
            line_range: 3..10,
            result_id: "test-region".to_string(),
            content_hash: 12345,
        };

        // Virtual position: line 0, character 3 (fn definition at start of virtual doc)
        // This corresponds to host line 3 (0 + 3 = 3)
        let virtual_pos = Position {
            line: 0,
            character: 3,
        };

        let host_pos = region.translate_virtual_to_host(virtual_pos);

        assert_eq!(
            host_pos.line, 3,
            "Line should be virtual_line + region_start_line"
        );
        assert_eq!(host_pos.character, 3, "Character should remain unchanged");
    }

    // ============================================================
    // Tests for calculate_region_id
    // ============================================================

    #[test]
    fn test_calculate_region_id_returns_language_ordinal_format() {
        // Test that calculate_region_id returns {language}-{ordinal} format
        // For a single Lua injection, it should return "lua-0"
        let mut parser = create_rust_parser();
        let text = r#"fn main() { let s = "hello"; }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Create an injection query that matches the string as "lua"
        let query_str = r#"
            ((string_literal) @injection.content
              (#set! injection.language "lua"))
        "#;
        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        // Get injection regions
        let regions = collect_all_injections(&root, text, Some(&query));
        let regions = regions.expect("Should find injections");
        assert_eq!(regions.len(), 1, "Should find exactly one injection");

        // Calculate region_id for the first (and only) injection
        let region_id = calculate_region_id(&regions, 0);
        assert_eq!(region_id, "lua-0", "First lua injection should be lua-0");
    }

    #[test]
    fn test_calculate_region_id_lua_python_lua_produces_correct_ordinals() {
        // Test that Lua-Python-Lua blocks produce lua-0, python-0, lua-1
        // This verifies ordinal is per-language, not global
        //
        // We simulate multiple injections by creating InjectionRegionInfo structs directly
        // since parsing multiple distinct language injections is complex with Rust grammar.

        let mut parser = create_rust_parser();
        let text = r#"fn main() { let a = "lua1"; let b = "python"; let c = "lua2"; }"#;
        let tree = parse_rust_code(&mut parser, text);
        let root = tree.root_node();

        // Find all string_literal nodes manually using StreamingIterator
        let mut cursor = tree_sitter::QueryCursor::new();
        let query_str = r#"(string_literal) @str"#;
        let language = tree_sitter_rust::LANGUAGE.into();
        let query = Query::new(&language, query_str).expect("valid query");

        let mut matches_iter = cursor.matches(&query, root, text.as_bytes());
        let mut nodes = Vec::new();
        while let Some(m) = matches_iter.next() {
            nodes.push(m.captures[0].node);
        }

        assert_eq!(nodes.len(), 3, "Should find 3 strings");

        // Create injection regions manually: lua, python, lua
        let injections = vec![
            InjectionRegionInfo {
                language: "lua".to_string(),
                content_node: nodes[0],
                pattern_index: 0,
            },
            InjectionRegionInfo {
                language: "python".to_string(),
                content_node: nodes[1],
                pattern_index: 0,
            },
            InjectionRegionInfo {
                language: "lua".to_string(),
                content_node: nodes[2],
                pattern_index: 0,
            },
        ];

        // Verify region_ids
        assert_eq!(
            calculate_region_id(&injections, 0),
            "lua-0",
            "First lua should be lua-0"
        );
        assert_eq!(
            calculate_region_id(&injections, 1),
            "python-0",
            "Python should be python-0"
        );
        assert_eq!(
            calculate_region_id(&injections, 2),
            "lua-1",
            "Second lua should be lua-1"
        );
    }
}
