pub mod hierarchy_chain;
pub mod injection_aware;
pub mod range_builder;

pub use hierarchy_chain::{
    chain_injected_to_host, is_range_strictly_larger, range_contains, ranges_equal,
    skip_to_distinct_host,
};
pub use injection_aware::{
    adjust_range_to_host, calculate_effective_lsp_range, is_cursor_within_effective_range,
    is_node_in_selection_chain,
};
pub use range_builder::{build_selection_range, find_distinct_parent, node_to_range};

use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range_with_text};
use crate::document::DocumentHandle;
use crate::language::injection::{self, parse_offset_directive_for_pattern};
use crate::language::{DocumentParserPool, LanguageCoordinator};
use crate::text::PositionMapper;
use tower_lsp::lsp_types::{Position, Range, SelectionRange};
use tree_sitter::{Node, Query};

/// Maximum depth for nested injection recursion (prevents stack overflow)
const MAX_INJECTION_DEPTH: usize = 10;

/// Build selection range with parsed injection content (Sprint 3 + Sprint 5 nested support)
///
/// This function parses the injected content using the appropriate language parser
/// and builds a selection hierarchy that includes nodes from the injected language's AST.
/// It recursively handles nested injections up to MAX_INJECTION_DEPTH levels.
///
/// # Arguments
/// * `node` - The node at cursor position in the host document
/// * `root` - The root node of the host document tree
/// * `text` - The full document text
/// * `mapper` - PositionMapper for UTF-16 column conversion
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
/// * `coordinator` - Language coordinator for getting parsers
/// * `parser_pool` - Parser pool for acquiring/releasing parsers
/// * `cursor_byte` - The byte offset of cursor position for offset checking
///
/// # Returns
/// SelectionRange that includes nodes from both injected and host language ASTs
#[allow(clippy::too_many_arguments)]
fn build_selection_range_with_parsed_injection(
    node: Node,
    root: &Node,
    text: &str,
    mapper: &PositionMapper,
    injection_query: Option<&Query>,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
) -> SelectionRange {
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    let Some((hierarchy, content_node, pattern_index)) = injection_info else {
        return build_selection_range(node, mapper);
    };

    if hierarchy.len() < 2 {
        return build_selection_range(node, mapper);
    }

    let offset_from_query =
        injection_query.and_then(|q| parse_offset_directive_for_pattern(q, pattern_index));

    if let Some(offset) = offset_from_query
        && !is_cursor_within_effective_range(text, &content_node, cursor_byte, offset)
    {
        return build_selection_range(node, mapper);
    }

    let injected_lang = &hierarchy[hierarchy.len() - 1];

    let build_fallback = || {
        let effective_range = offset_from_query
            .map(|offset| calculate_effective_lsp_range(text, mapper, &content_node, offset));
        build_unparsed_injection_selection(node, content_node, effective_range, mapper)
    };

    let load_result = coordinator.ensure_language_loaded(injected_lang);
    if !load_result.success {
        return build_fallback();
    }

    let Some(mut parser) = parser_pool.acquire(injected_lang) else {
        return build_fallback();
    };

    let (content_text, effective_start_byte) = if let Some(offset) = offset_from_query {
        let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
        let effective = calculate_effective_range_with_text(text, byte_range, offset);
        (&text[effective.start..effective.end], effective.start)
    } else {
        (&text[content_node.byte_range()], content_node.start_byte())
    };

    let Some(injected_tree) = parser.parse(content_text, None) else {
        parser_pool.release(injected_lang.to_string(), parser);
        return build_fallback();
    };

    let relative_byte = cursor_byte.saturating_sub(effective_start_byte);
    let injected_root = injected_tree.root_node();

    let Some(injected_node) = injected_root.descendant_for_byte_range(relative_byte, relative_byte)
    else {
        parser_pool.release(injected_lang.to_string(), parser);
        return build_fallback();
    };

    let nested_injection_query = coordinator.get_injection_query(injected_lang);

    let injected_selection = if let Some(nested_inj_query) = nested_injection_query.as_ref() {
        let nested_injection_info = injection::detect_injection_with_content(
            &injected_node,
            &injected_root,
            content_text,
            Some(nested_inj_query.as_ref()),
            injected_lang,
        );

        if let Some((nested_hierarchy, nested_content_node, nested_pattern_index)) =
            nested_injection_info
        {
            let nested_offset =
                parse_offset_directive_for_pattern(nested_inj_query.as_ref(), nested_pattern_index);

            let cursor_in_nested = match nested_offset {
                Some(offset) => is_cursor_within_effective_range(
                    content_text,
                    &nested_content_node,
                    relative_byte,
                    offset,
                ),
                None => true,
            };

            if cursor_in_nested && nested_hierarchy.len() >= 2 {
                build_recursive_injection_selection(
                    &injected_node,
                    &injected_root,
                    content_text,
                    nested_inj_query.as_ref(),
                    injected_lang,
                    coordinator,
                    parser_pool,
                    relative_byte,
                    effective_start_byte,
                    mapper,
                    1, // depth: first level of nested injection
                )
            } else {
                build_injected_selection_range(
                    injected_node,
                    &injected_root,
                    effective_start_byte,
                    mapper,
                )
            }
        } else {
            build_injected_selection_range(
                injected_node,
                &injected_root,
                effective_start_byte,
                mapper,
            )
        }
    } else {
        build_injected_selection_range(injected_node, &injected_root, effective_start_byte, mapper)
    };

    let host_selection = Some(build_selection_range(content_node, mapper));
    let result = chain_injected_to_host(injected_selection, host_selection);
    parser_pool.release(injected_lang.to_string(), parser);
    result
}

/// Recursively build selection for deeply nested injections.
///
/// Parses injection content and recurses if further injections are detected.
/// The `parent_start_byte` is the byte offset where the parent injection content
/// starts in the host document. The `mapper` is for the host document.
#[allow(clippy::too_many_arguments)]
fn build_recursive_injection_selection(
    node: &Node,
    root: &Node,
    text: &str,
    injection_query: &Query,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
    parent_start_byte: usize,
    mapper: &PositionMapper,
    depth: usize,
) -> SelectionRange {
    if depth >= MAX_INJECTION_DEPTH {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    }

    let injection_info = injection::detect_injection_with_content(
        node,
        root,
        text,
        Some(injection_query),
        base_language,
    );

    let Some((hierarchy, content_node, pattern_index)) = injection_info else {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    if hierarchy.len() < 2 {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    }
    let nested_lang = hierarchy.last().unwrap().clone();

    let offset = parse_offset_directive_for_pattern(injection_query, pattern_index);

    let load_result = coordinator.ensure_language_loaded(&nested_lang);
    if !load_result.success {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    }

    let Some(mut nested_parser) = parser_pool.acquire(&nested_lang) else {
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    // effective.start is relative to `text`, so host byte = parent_start_byte + effective.start
    let (nested_text, nested_effective_start_byte) = if let Some(off) = offset {
        let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
        let effective = calculate_effective_range_with_text(text, byte_range, off);
        (
            &text[effective.start..effective.end],
            parent_start_byte + effective.start,
        )
    } else {
        (
            &text[content_node.byte_range()],
            parent_start_byte + content_node.start_byte(),
        )
    };

    let Some(nested_tree) = nested_parser.parse(nested_text, None) else {
        parser_pool.release(nested_lang.to_string(), nested_parser);
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    let nested_relative_byte = if let Some(off) = offset {
        let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
        let effective = calculate_effective_range_with_text(text, byte_range, off);
        cursor_byte.saturating_sub(effective.start)
    } else {
        cursor_byte.saturating_sub(content_node.start_byte())
    };

    let nested_root = nested_tree.root_node();

    let Some(nested_node) =
        nested_root.descendant_for_byte_range(nested_relative_byte, nested_relative_byte)
    else {
        parser_pool.release(nested_lang.to_string(), nested_parser);
        return build_injected_selection_range(*node, root, parent_start_byte, mapper);
    };

    let deeply_nested_injection_query = coordinator.get_injection_query(&nested_lang);

    let nested_selection = if let Some(deep_inj_query) = deeply_nested_injection_query.as_ref() {
        let deep_injection_info = injection::detect_injection_with_content(
            &nested_node,
            &nested_root,
            nested_text,
            Some(deep_inj_query.as_ref()),
            &nested_lang,
        );

        if deep_injection_info.is_some() {
            build_recursive_injection_selection(
                &nested_node,
                &nested_root,
                nested_text,
                deep_inj_query.as_ref(),
                &nested_lang,
                coordinator,
                parser_pool,
                nested_relative_byte,
                nested_effective_start_byte,
                mapper,
                depth + 1,
            )
        } else {
            build_injected_selection_range(
                nested_node,
                &nested_root,
                nested_effective_start_byte,
                mapper,
            )
        }
    } else {
        build_injected_selection_range(
            nested_node,
            &nested_root,
            nested_effective_start_byte,
            mapper,
        )
    };

    let content_node_selection = Some(build_injected_selection_range(
        content_node,
        root,
        parent_start_byte,
        mapper,
    ));

    let result = chain_injected_to_host(nested_selection, content_node_selection);
    parser_pool.release(nested_lang.to_string(), nested_parser);
    result
}

/// Calculate the start position for nested injection relative to host document
///
/// This function handles signed offsets from injection directives like
/// `(#offset! @injection.content -1 0 0 0)` used in markdown frontmatter.
/// Negative offsets are handled with saturating arithmetic to prevent underflow.
///
/// Column calculation logic:
/// - If the *effective* row (content_start.row + offset_rows) is 0, we're on the
///   same row as the parent, so we add parent's column offset.
/// - If the effective row is > 0, we've moved to a later row (e.g., after skipping
///   a fence line), so the column is absolute within the content.
///
/// Note: This function was used for Point-based calculation before Sprint 9.
/// It's now kept for test coverage but production code uses byte-based offsets.
#[cfg(test)]
fn calculate_nested_start_position(
    parent_start: tree_sitter::Point,
    content_start: tree_sitter::Point,
    offset_rows: i32,
    offset_cols: i32,
) -> tree_sitter::Point {
    let col_parent = if (content_start.row as i32 + offset_rows).max(0) == 0 {
        parent_start.column as i64
    } else {
        0 as i64
    };
    tree_sitter::Point::new(
        ((parent_start.row + content_start.row) as i64 + offset_rows as i64).max(0) as usize,
        (col_parent + content_start.column as i64 + offset_cols as i64).max(0) as usize,
    )
}

/// Build selection range for nodes in injected content
///
/// This builds SelectionRange from injected AST nodes, adjusting positions
/// to be relative to the host document (not the injection slice).
/// Nodes with identical ranges are deduplicated (LSP spec requires strictly expanding ranges).
///
/// The `content_start_byte` is the byte offset where the injection content starts
/// in the host document. The `mapper` is used for proper UTF-16 column conversion.
fn build_injected_selection_range(
    node: Node,
    injected_root: &Node,
    content_start_byte: usize,
    mapper: &PositionMapper,
) -> SelectionRange {
    let parent = find_distinct_parent(node, &node.byte_range()).map(|parent_node| {
        if parent_node.id() == injected_root.id() {
            Box::new(SelectionRange {
                range: adjust_range_to_host(parent_node, content_start_byte, mapper),
                parent: None, // Connected to host in chain_injected_to_host
            })
        } else {
            Box::new(build_injected_selection_range(
                parent_node,
                injected_root,
                content_start_byte,
                mapper,
            ))
        }
    });

    SelectionRange {
        range: adjust_range_to_host(node, content_start_byte, mapper),
        parent,
    }
}

/// Build selection when injection content cannot be parsed.
///
/// Used as fallback when injection language is unavailable or parser fails.
/// Splices the effective_range into the host document's selection hierarchy
/// without parsing the injection content.
fn build_unparsed_injection_selection(
    node: Node,
    content_node: Node,
    effective_range: Option<Range>,
    mapper: &PositionMapper,
) -> SelectionRange {
    let content_node_range = node_to_range(content_node, mapper);
    let inner_selection = build_selection_range(node, mapper);

    if let Some(eff_range) = effective_range {
        if ranges_equal(&inner_selection.range, &content_node_range) {
            return SelectionRange {
                range: eff_range,
                parent: inner_selection
                    .parent
                    .map(|p| Box::new(replace_range_in_chain(*p, content_node_range, eff_range))),
            };
        }

        if is_node_in_selection_chain(&inner_selection, &content_node, mapper) {
            return replace_range_in_chain(inner_selection, content_node_range, eff_range);
        }
    } else if is_node_in_selection_chain(&inner_selection, &content_node, mapper) {
        return inner_selection;
    }

    splice_effective_range_into_hierarchy(
        inner_selection,
        effective_range.unwrap_or(content_node_range),
        &content_node,
        mapper,
    )
}

/// Replace a specific range in the selection chain with the effective range
fn replace_range_in_chain(
    selection: SelectionRange,
    target_range: Range,
    effective_range: Range,
) -> SelectionRange {
    SelectionRange {
        range: if ranges_equal(&selection.range, &target_range) {
            effective_range
        } else {
            selection.range
        },
        parent: selection
            .parent
            .map(|p| Box::new(replace_range_in_chain(*p, target_range, effective_range))),
    }
}

fn splice_effective_range_into_hierarchy(
    selection: SelectionRange,
    effective_range: Range,
    content_node: &Node,
    mapper: &PositionMapper,
) -> SelectionRange {
    if !range_contains(&effective_range, &selection.range) {
        return selection;
    }

    let parent = match selection.parent {
        Some(parent) => {
            let parent = *parent;
            let parent_range = parent.range;
            let spliced = Some(Box::new(splice_effective_range_into_hierarchy(
                parent,
                effective_range,
                content_node,
                mapper,
            )));
            if range_contains(&parent_range, &effective_range)
                && !ranges_equal(&parent_range, &effective_range)
            {
                Some(Box::new(SelectionRange {
                    range: effective_range,
                    parent: spliced,
                }))
            } else {
                spliced
            }
        }
        None => Some(Box::new(SelectionRange {
            range: effective_range,
            parent: content_node
                .parent()
                .map(|p| Box::new(build_selection_range(p, mapper))),
        })),
    };

    SelectionRange {
        range: selection.range,
        parent,
    }
}

/// Handle textDocument/selectionRange request with full injection parsing support.
///
/// Parses injected content and builds selection hierarchies from the injected
/// language's AST. Returns one SelectionRange per position (LSP Spec 3.17 alignment).
pub fn handle_selection_range(
    document: &DocumentHandle,
    positions: &[Position],
    injection_query: Option<&Query>,
    base_language: Option<&str>,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
) -> Vec<SelectionRange> {
    let text = document.text();
    let mapper = document.position_mapper();
    let root = document.tree().map(|t| t.root_node());
    positions
        .iter()
        .map(|pos| {
            if let Some(root) = root
                && let Some(cursor_byte_offset) = mapper.position_to_byte(*pos)
                && let Some(node) =
                    root.descendant_for_byte_range(cursor_byte_offset, cursor_byte_offset)
            {
                if let Some(lang) = base_language {
                    build_selection_range_with_parsed_injection(
                        node,
                        &root,
                        text,
                        &mapper,
                        injection_query,
                        lang,
                        coordinator,
                        parser_pool,
                        cursor_byte_offset,
                    )
                } else {
                    build_selection_range(node, &mapper)
                }
            } else {
                SelectionRange {
                    range: Range::new(*pos, *pos),
                    parent: None,
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    /// Verifies offset directive parsing and effective range boundary checking.
    #[test]
    fn test_selection_range_respects_offset_directive() {
        use crate::text::PositionMapper;
        use tree_sitter::{Parser, Query};

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // string_content "_^\d+$" is at bytes 43-49
        let text = r#"fn main() {
    let pattern = Regex::new(r"_^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // offset (0, 2, 0, 0) means effective range starts 2 bytes after capture → bytes 45-49
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
  (#offset! @injection.content 0 2 0 0))
        "#;

        let injection_query = Query::new(&language, injection_query_str).expect("valid query");
        let mapper = PositionMapper::new(text);

        let underscore_pos = Position::new(1, 31);
        let underscore_point = Point::new(
            underscore_pos.line as usize,
            underscore_pos.character as usize,
        );
        let underscore_byte = mapper.position_to_byte(underscore_pos).unwrap();

        let string_content_node = root
            .descendant_for_point_range(underscore_point, underscore_point)
            .expect("should find node");

        assert_eq!(string_content_node.kind(), "string_content");
        assert_eq!(string_content_node.start_byte(), 43);
        assert_eq!(string_content_node.end_byte(), 49);

        let offset = parse_offset_directive_for_pattern(&injection_query, 0);
        assert!(offset.is_some(), "Offset directive should be found");
        let offset = offset.unwrap();
        assert_eq!(offset.start_row, 0);
        assert_eq!(offset.start_column, 2);
        assert_eq!(offset.end_row, 0);
        assert_eq!(offset.end_column, 0);

        // byte 43 (underscore) is OUTSIDE effective range 45-49
        assert!(
            !is_cursor_within_effective_range(text, &string_content_node, underscore_byte, offset),
            "Cursor at byte 43 (underscore) should be OUTSIDE effective range 45-49"
        );

        // byte 45 (caret) is INSIDE effective range
        let caret_pos = Position::new(1, 33);
        let caret_byte = mapper.position_to_byte(caret_pos).unwrap();
        assert_eq!(caret_byte, 45, "Caret should be at byte 45");

        assert!(
            is_cursor_within_effective_range(text, &string_content_node, caret_byte, offset),
            "Cursor at byte 45 (caret ^) should be INSIDE effective range 45-49"
        );

        // byte 44 is OUTSIDE effective range
        assert!(
            !is_cursor_within_effective_range(text, &string_content_node, 44, offset),
            "Cursor at byte 44 should be OUTSIDE effective range 45-49"
        );
    }

    /// Nested injection (Rust → YAML → Rust) includes content node boundary in hierarchy.
    #[test]
    fn test_nested_injection_includes_content_node_boundary() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());
        coordinator.register_language_for_test("rust", tree_sitter_rust::LANGUAGE.into());

        let yaml_injection_query_str = r#"
((double_quote_scalar) @injection.content
 (#set! injection.language "rust"))
        "#;
        let yaml_lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
        let yaml_injection_query =
            Query::new(&yaml_lang, yaml_injection_query_str).expect("valid yaml injection query");
        coordinator.register_injection_query_for_test("yaml", yaml_injection_query);

        let mut parser_pool = coordinator.create_document_parser_pool();

        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        // Rust → YAML → Rust: double_quote_scalar contains "fn nested() {}"
        let text = r##"fn main() {
    let yaml = r#"title: "fn nested() {}""#;
}"##;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query =
            Query::new(&rust_language, injection_query_str).expect("valid injection query");

        let cursor_pos = Position::new(1, 33);
        let mapper = crate::text::PositionMapper::new(text);
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();
        let point = Point::new(cursor_pos.line as usize, cursor_pos.character as usize);
        let node = root
            .descendant_for_point_range(point, point)
            .expect("find node");

        let selection = build_selection_range_with_parsed_injection(
            node,
            &root,
            text,
            &mapper,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            cursor_byte,
        );

        let mut ranges: Vec<Range> = Vec::new();
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            ranges.push(sel.range);
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        assert!(
            ranges.len() >= 7,
            "Expected at least 7 selection levels with nested injection, got {}",
            ranges.len()
        );

        // double_quote_scalar boundary should be around (1:25-26 to 1:40-42)
        let nested_content_found = ranges.iter().any(|r| {
            r.start.line == 1
                && r.start.character >= 25
                && r.start.character <= 26
                && r.end.line == 1
                && r.end.character >= 40
                && r.end.character <= 42
        });

        assert!(
            nested_content_found,
            "Selection hierarchy should include nested injection content node boundary.\n\
             Ranges in hierarchy: {:?}\n\
             Expected a range around (1:25-26 to 1:40-42) for the nested content node.",
            ranges
        );
    }

    /// Injected content (YAML in Rust string) produces selection hierarchy from injected AST.
    #[test]
    fn test_selection_range_parses_injected_content() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());

        let mut parser_pool = coordinator.create_document_parser_pool();

        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        let text = r##"fn main() {
    let yaml = r#"title: "awesome"
array: ["xxxx"]"#;
}"##;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query =
            Query::new(&rust_language, injection_query_str).expect("valid injection query");

        let cursor_pos = Position::new(1, 32); // 'a' in "awesome"
        let point = Point::new(cursor_pos.line as usize, cursor_pos.character as usize);

        let node = root
            .descendant_for_point_range(point, point)
            .expect("should find node");

        let mapper = crate::text::PositionMapper::new(text);
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();

        let selection = build_selection_range_with_parsed_injection(
            node,
            &root,
            text,
            &mapper,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            cursor_byte,
        );

        let mut level_count = 0;
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            level_count += 1;
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // With injection: YAML AST nodes + host nodes = at least 7 levels
        assert!(
            level_count >= 7,
            "Expected at least 7 selection levels with injected YAML AST, got {}. \
             This indicates the injected content was not parsed.",
            level_count
        );
    }

    /// Nested start position handles negative offsets and column alignment.
    #[test]
    fn test_calculate_nested_start_position() {
        // Negative row offset saturates to 0
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(2, 0),
            tree_sitter::Point::new(1, 0),
            -5,
            0,
        );
        assert_eq!(result.row, 0);

        // Negative column offset saturates to 0
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(0, 10),
            tree_sitter::Point::new(0, 5),
            0,
            -20,
        );
        assert_eq!(result.column, 0);

        // Effective row 0: add parent's column
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(5, 4),
            tree_sitter::Point::new(0, 0),
            0,
            0,
        );
        assert_eq!(result.row, 5);
        assert_eq!(result.column, 4);

        // Row offset > 0: column is absolute
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(5, 4),
            tree_sitter::Point::new(0, 0),
            1,
            0,
        );
        assert_eq!(result.row, 6);
        assert_eq!(result.column, 0);

        // Positive offset: column is absolute
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(10, 5),
            tree_sitter::Point::new(0, 3),
            2,
            1,
        );
        assert_eq!(result.row, 12);
        assert_eq!(result.column, 4);

        // No row offset: add parent column
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(10, 5),
            tree_sitter::Point::new(0, 3),
            0,
            1,
        );
        assert_eq!(result.row, 10);
        assert_eq!(result.column, 9);

        // Negative offset brings effective row to 0: add parent column
        let result = calculate_nested_start_position(
            tree_sitter::Point::new(5, 4),
            tree_sitter::Point::new(1, 2),
            -1,
            0,
        );
        assert_eq!(result.row, 5);
        assert_eq!(result.column, 6);
    }

    /// Deduplicates nodes with identical ranges (LSP requires strictly expanding ranges).
    #[test]
    fn test_selection_range_deduplicates_same_range_nodes() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = "fn f() { x }";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        let cursor_byte = 9; // position of "x"
        let node = root
            .descendant_for_byte_range(cursor_byte, cursor_byte)
            .expect("should find node");

        assert_eq!(node.kind(), "identifier");

        let mapper = PositionMapper::new(text);
        let selection = build_selection_range(node, &mapper);

        let mut ranges: Vec<(u32, u32, u32, u32)> = Vec::new();
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            ranges.push((
                sel.range.start.line,
                sel.range.start.character,
                sel.range.end.line,
                sel.range.end.character,
            ));
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // No consecutive duplicates allowed
        for i in 1..ranges.len() {
            assert_ne!(
                ranges[i - 1],
                ranges[i],
                "Found duplicate ranges at positions {} and {}: {:?}",
                i - 1,
                i,
                ranges[i]
            );
        }

        assert!(
            ranges.len() <= 8,
            "Expected at most 8 levels (with deduplication), got {}. Ranges: {:?}",
            ranges.len(),
            ranges
        );
    }

    /// Selection ranges use UTF-16 columns, not byte offsets.
    /// "あ" = 3 bytes UTF-8, 1 UTF-16 code unit. 'x' at byte 17 → UTF-16 column 15.
    #[test]
    fn test_selection_range_output_uses_utf16_columns() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = "let あ = 1; let x = 2;";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        let mapper = PositionMapper::new(text);

        let byte_offset = 17; // 'x' at byte 17
        let node = root
            .descendant_for_byte_range(byte_offset, byte_offset)
            .expect("should find node");

        assert_eq!(node.kind(), "identifier");
        assert_eq!(&text[node.byte_range()], "x");

        let selection = build_selection_range(node, &mapper);

        // UTF-16 column 15, not byte 17
        assert_eq!(selection.range.start.character, 15);
        assert_eq!(selection.range.end.character, 16);
    }

    /// Injected content uses UTF-16 columns. "0" in `r#"あ: 0"#` is at UTF-16 col 17, not byte 19.
    #[test]
    fn test_injected_selection_range_uses_utf16_columns() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());

        let yaml_injection_query_str = r#"
((double_quote_scalar) @injection.content
 (#set! injection.language "yaml"))
        "#;
        let yaml_lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
        let yaml_injection_query =
            Query::new(&yaml_lang, yaml_injection_query_str).expect("valid yaml injection query");
        coordinator.register_injection_query_for_test("yaml", yaml_injection_query);

        let mut parser_pool = coordinator.create_document_parser_pool();

        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        // "あ" = 3 bytes UTF-8, 1 UTF-16 code unit
        // "0" at UTF-16 col 17, byte 19
        let text = "let yaml = r#\"あ: 0\"#;";
        let tree = parser.parse(text, None).expect("parse");
        let root = tree.root_node();

        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query = Query::new(&rust_language, injection_query_str).expect("valid query");

        let mapper = crate::text::PositionMapper::new(text);

        let mut cursor = root.walk();
        let mut content_node = None;
        loop {
            let node = cursor.node();
            if node.kind() == "string_content" {
                content_node = Some(node);
                break;
            }
            if cursor.goto_first_child() {
                continue;
            }
            while !cursor.goto_next_sibling() {
                if !cursor.goto_parent() {
                    break;
                }
            }
            if cursor.node().id() == root.id() {
                break;
            }
        }
        let content_node = content_node.expect("Should find string_content node");

        let content_text = &text[content_node.byte_range()];
        assert_eq!(content_text, "あ: 0");

        let zero_byte_in_host = content_node.start_byte() + 5; // "あ"=3 + ": "=2

        let selection = build_selection_range_with_parsed_injection(
            content_node,
            &root,
            text,
            &mapper,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            zero_byte_in_host,
        );

        // Find small range in injected content, verify UTF-16 column < 19 (byte offset)
        let mut found_small_range = false;
        let mut current = &selection;
        loop {
            if current.range.start.line == 0
                && current.range.end.line == 0
                && current.range.end.character - current.range.start.character <= 5
                && current.range.start.character >= 17
                && current.range.start.character <= 20
            {
                found_small_range = true;
                assert!(
                    current.range.start.character < 19,
                    "Expected UTF-16 column (17 or 18), got byte-based column {}",
                    current.range.start.character
                );
                break;
            }
            if let Some(parent) = &current.parent {
                current = parent.as_ref();
            } else {
                break;
            }
        }

        assert!(
            found_small_range,
            "Should find a small range in the injected content. Selection ranges: {:?}",
            collect_ranges(&selection)
        );
    }

    /// Invalid positions get fallback empty ranges (LSP Spec 3.17: 1:1 position-result alignment).
    #[test]
    fn test_selection_range_maintains_position_alignment() {
        use crate::document::store::DocumentStore;
        use crate::language::LanguageCoordinator;
        use tower_lsp::lsp_types::Url;
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = "let x = 1;\nlet y = 2;";
        let tree = parser.parse(text, None).expect("parse rust");

        let url = Url::parse("file:///test.rs").unwrap();
        let store = DocumentStore::new();
        store.insert(
            url.clone(),
            text.to_string(),
            Some("rust".to_string()),
            Some(tree),
        );

        let positions = vec![
            Position::new(0, 4),   // valid: 'x'
            Position::new(100, 0), // invalid: line 100 doesn't exist
            Position::new(1, 4),   // valid: 'y'
        ];

        let coordinator = LanguageCoordinator::new();
        let mut parser_pool = coordinator.create_document_parser_pool();
        let document = store.get(&url).expect("document should exist");
        let ranges = handle_selection_range(
            &document,
            &positions,
            None,
            None,
            &coordinator,
            &mut parser_pool,
        );

        assert_eq!(ranges.len(), positions.len());
        assert!(ranges[0].range.start.line == 0);
        assert_eq!(ranges[1].range.start, ranges[1].range.end); // invalid → empty
        assert!(ranges[1].parent.is_none());
        assert!(ranges[2].range.start.line == 1);
    }

    /// Helper to collect all ranges in a selection hierarchy for debugging
    fn collect_ranges(selection: &SelectionRange) -> Vec<Range> {
        let mut ranges = vec![selection.range];
        let mut current = &selection.parent;
        while let Some(parent) = current {
            ranges.push(parent.range);
            current = &parent.parent;
        }
        ranges
    }

    /// Empty documents return valid fallback range at (0, 0).
    #[test]
    fn test_selection_range_handles_empty_document() {
        use crate::document::store::DocumentStore;
        use crate::language::LanguageCoordinator;
        use tower_lsp::lsp_types::Url;
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = "";
        let tree = parser.parse(text, None).expect("parse empty document");

        let url = Url::parse("file:///empty.rs").unwrap();
        let store = DocumentStore::new();
        store.insert(
            url.clone(),
            text.to_string(),
            Some("rust".to_string()),
            Some(tree),
        );

        let positions = vec![Position::new(0, 0)];

        let coordinator = LanguageCoordinator::new();
        let mut parser_pool = coordinator.create_document_parser_pool();
        let document = store.get(&url).expect("document should exist");
        let ranges = handle_selection_range(
            &document,
            &positions,
            None,
            None,
            &coordinator,
            &mut parser_pool,
        );

        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].range.start, Position::new(0, 0));
        assert_eq!(ranges[0].range.end, Position::new(0, 0));
    }
}
