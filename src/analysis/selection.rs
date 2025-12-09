use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range_with_text};
use crate::document::DocumentHandle;
use crate::language::injection::{self, parse_offset_directive_for_pattern};
use crate::language::{DocumentParserPool, LanguageCoordinator};
use tower_lsp::lsp_types::{Position, Range, SelectionRange};
use tree_sitter::{Node, Point, Query};

/// Convert LSP Position to tree-sitter Point
pub fn position_to_point(pos: &Position) -> Point {
    Point::new(pos.line as usize, pos.character as usize)
}

/// Convert tree-sitter Point to LSP Position
pub fn point_to_position(point: Point) -> Position {
    Position::new(point.row as u32, point.column as u32)
}

/// Convert tree-sitter Node to LSP Range
fn node_to_range(node: Node) -> Range {
    Range::new(
        point_to_position(node.start_position()),
        point_to_position(node.end_position()),
    )
}

/// Build selection range hierarchy for a node
fn build_selection_range(node: Node) -> SelectionRange {
    let range = node_to_range(node);
    let node_byte_range = node.byte_range();

    // Build parent chain, skipping nodes with same range (LSP spec requires strictly expanding)
    let parent = find_distinct_parent(node, &node_byte_range)
        .map(|parent_node| Box::new(build_selection_range(parent_node)));

    SelectionRange { range, parent }
}

/// Find the next parent node that has a different (larger) range than the current node.
/// This ensures the LSP selection range hierarchy is strictly expanding.
/// Unlike `find_next_distinct_parent`, this version doesn't have a root check
/// and simply walks up until it finds a parent with a different range.
fn find_distinct_parent<'a>(
    node: Node<'a>,
    current_range: &std::ops::Range<usize>,
) -> Option<Node<'a>> {
    let mut current = node.parent();
    while let Some(parent) = current {
        let parent_range = parent.byte_range();
        // If parent has a different range, use it
        if parent_range != *current_range {
            return Some(parent);
        }
        current = parent.parent();
    }
    None
}

/// Build selection range hierarchy with injection awareness
///
/// When the cursor is inside an injection region, this function ensures
/// the injection content node is included in the selection hierarchy.
///
/// # Arguments
/// * `node` - The node at cursor position
/// * `root` - The root node of the document tree
/// * `text` - The document text
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
///
/// # Returns
/// SelectionRange that includes injection boundaries when applicable
pub fn build_selection_range_with_injection(
    node: Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
) -> SelectionRange {
    // Try to detect if we're inside an injection region
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    match injection_info {
        Some((_hierarchy, content_node, _pattern_index)) => {
            build_injection_aware_selection(node, content_node)
        }
        None => build_selection_range(node),
    }
}

/// Build selection range hierarchy with injection awareness and offset support
///
/// This version takes a cursor_byte parameter to check if the cursor is within
/// the effective range of the injection after applying offset directives.
/// When an offset directive exists, the selection hierarchy uses the effective
/// range (after applying offset) instead of the full content node range.
///
/// # Arguments
/// * `node` - The node at cursor position
/// * `root` - The root node of the document tree
/// * `text` - The document text
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
/// * `cursor_byte` - The byte position of the cursor for offset checking
///
/// # Returns
/// SelectionRange that includes injection boundaries when applicable and cursor
/// is within the effective range (after applying offset)
pub fn build_selection_range_with_injection_and_offset(
    node: Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
    cursor_byte: usize,
) -> SelectionRange {
    // Try to detect if we're inside an injection region
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    match injection_info {
        Some((_hierarchy, content_node, pattern_index)) => {
            // Check for offset directive on this specific pattern
            let offset_from_query =
                injection_query.and_then(|q| parse_offset_directive_for_pattern(q, pattern_index));

            // Only apply offset-based filtering when there's an actual offset directive
            // (Lesson from Sprint 13 in development record 0002)
            if let Some(offset) = offset_from_query {
                // Check if cursor is within the effective range (after applying offset)
                if !is_cursor_within_effective_range(text, &content_node, cursor_byte, offset) {
                    // Cursor is outside effective range - return base language selection
                    return build_selection_range(node);
                }

                // Use effective range in selection hierarchy instead of full content node range
                let effective_range = calculate_effective_lsp_range(text, &content_node, offset);
                return build_injection_aware_selection_with_effective_range(
                    node,
                    content_node,
                    effective_range,
                );
            }

            // No offset directive - use full content node range
            build_injection_aware_selection(node, content_node)
        }
        None => build_selection_range(node),
    }
}

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
/// * `injection_query` - Optional injection query for detecting injections
/// * `base_language` - The base language of the document
/// * `coordinator` - Language coordinator for getting parsers
/// * `parser_pool` - Parser pool for acquiring/releasing parsers
/// * `cursor_byte` - The byte offset of cursor position for offset checking
///
/// # Returns
/// SelectionRange that includes nodes from both injected and host language ASTs
#[allow(clippy::too_many_arguments)]
pub fn build_selection_range_with_parsed_injection(
    node: Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
) -> SelectionRange {
    // Delegate to the recursive implementation with depth 0
    build_selection_range_with_parsed_injection_recursive(
        node,
        root,
        text,
        injection_query,
        base_language,
        coordinator,
        parser_pool,
        cursor_byte,
        0, // Initial depth
    )
}

/// Internal recursive implementation for nested injection support (Sprint 5)
#[allow(clippy::too_many_arguments)]
fn build_selection_range_with_parsed_injection_recursive(
    node: Node,
    root: &Node,
    text: &str,
    injection_query: Option<&Query>,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
    depth: usize,
) -> SelectionRange {
    // Safety: limit recursion depth to prevent stack overflow
    if depth >= MAX_INJECTION_DEPTH {
        return build_selection_range(node);
    }

    // First, detect if we're inside an injection region
    let injection_info =
        injection::detect_injection_with_content(&node, root, text, injection_query, base_language);

    let Some((hierarchy, content_node, pattern_index)) = injection_info else {
        // Not in injection - fall back to normal selection
        return build_selection_range(node);
    };

    // Need at least 2 entries in hierarchy: base language + injected language
    if hierarchy.len() < 2 {
        return build_selection_range(node);
    }

    // Check for offset directive on this specific pattern
    let offset_from_query =
        injection_query.and_then(|q| parse_offset_directive_for_pattern(q, pattern_index));

    // If offset exists, check if cursor is within effective range
    if let Some(offset) = offset_from_query
        && !is_cursor_within_effective_range(text, &content_node, cursor_byte, offset)
    {
        // Cursor is outside effective range - return base language selection
        return build_selection_range(node);
    }

    // Get the injected language name (last in hierarchy)
    let injected_lang = &hierarchy[hierarchy.len() - 1];

    // Helper closure to build fallback selection with or without effective range
    let build_fallback = || {
        if let Some(offset) = offset_from_query {
            // Use effective range in fallback when offset exists
            let effective_range = calculate_effective_lsp_range(text, &content_node, offset);
            build_injection_aware_selection_with_effective_range(
                node,
                content_node,
                effective_range,
            )
        } else {
            build_injection_aware_selection(node, content_node)
        }
    };

    // Ensure the injected language is loaded before trying to acquire a parser
    // This dynamically loads the language from search paths if not already registered
    let load_result = coordinator.ensure_language_loaded(injected_lang);
    if !load_result.success {
        return build_fallback();
    }

    // Try to acquire a parser for the injected language
    let Some(mut parser) = parser_pool.acquire(injected_lang) else {
        return build_fallback();
    };

    // Extract the injected content text - use effective range if offset exists
    let (content_text, effective_start_position, effective_start_byte) =
        if let Some(offset) = offset_from_query {
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range_with_text(text, byte_range, offset);
            let effective_text = &text[effective.start..effective.end];

            // Calculate effective start position for coordinate adjustment
            let mapper = crate::text::PositionMapper::new(text);
            let effective_start_pos = mapper
                .byte_to_position(effective.start)
                .map(|p| tree_sitter::Point::new(p.line as usize, p.character as usize))
                .unwrap_or(content_node.start_position());

            (effective_text, effective_start_pos, effective.start)
        } else {
            (
                &text[content_node.byte_range()],
                content_node.start_position(),
                content_node.start_byte(),
            )
        };

    // Parse the injected content
    let Some(injected_tree) = parser.parse(content_text, None) else {
        parser_pool.release(injected_lang.to_string(), parser);
        return build_fallback();
    };

    // Calculate cursor position relative to effective injection content
    let relative_byte = cursor_byte.saturating_sub(effective_start_byte);

    // Find the node at cursor position in the injected AST
    let injected_root = injected_tree.root_node();
    let Some(injected_node) = injected_root.descendant_for_byte_range(relative_byte, relative_byte)
    else {
        parser_pool.release(injected_lang.to_string(), parser);
        return build_fallback();
    };

    // Sprint 5: Check for nested injection within the injected content
    // Get the injection query for the injected language
    let nested_injection_query = coordinator.get_injection_query(injected_lang);

    let injected_selection = if let Some(nested_inj_query) = nested_injection_query.as_ref() {
        // Check if cursor is inside a nested injection
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
            // Check offset for nested injection
            let nested_offset =
                parse_offset_directive_for_pattern(nested_inj_query.as_ref(), nested_pattern_index);

            let cursor_in_nested = match nested_offset {
                Some(offset) => is_cursor_within_effective_range(
                    content_text,
                    &nested_content_node,
                    relative_byte,
                    offset,
                ),
                None => true, // No offset means cursor is always "inside" if detected
            };

            if cursor_in_nested && nested_hierarchy.len() >= 2 {
                // We have a nested injection! Recursively build selection for it
                build_nested_injection_selection(
                    &injected_node,
                    &injected_root,
                    content_text,
                    nested_inj_query.as_ref(),
                    injected_lang,
                    coordinator,
                    parser_pool,
                    relative_byte,
                    effective_start_position,
                    depth + 1,
                )
            } else {
                // No valid nested injection - build selection from current injected node
                build_injected_selection_range(
                    injected_node,
                    &injected_root,
                    effective_start_position,
                )
            }
        } else {
            // No nested injection detected - build selection from current injected node
            build_injected_selection_range(injected_node, &injected_root, effective_start_position)
        }
    } else {
        // No injection query for the injected language - build selection from current injected node
        build_injected_selection_range(injected_node, &injected_root, effective_start_position)
    };

    // Now chain the injected selection to the host document's selection
    // Skip the content_node itself (its range is replaced by the injected hierarchy)
    // Start from content_node's GRANDPARENT to avoid including content_node's full range
    let host_selection = content_node
        .parent()
        .and_then(|parent| parent.parent())
        .map(|grandparent| build_selection_range(grandparent));

    // Connect injected hierarchy to host hierarchy
    let result = chain_injected_to_host(injected_selection, host_selection);

    // Release the parser back to the pool
    parser_pool.release(injected_lang.to_string(), parser);

    result
}

/// Build selection for a nested injection (Sprint 5)
///
/// This handles the recursive case where we're inside an injection that itself
/// contains another injection.
#[allow(clippy::too_many_arguments)]
fn build_nested_injection_selection(
    node: &Node,
    root: &Node,
    text: &str,
    injection_query: &Query,
    base_language: &str,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
    cursor_byte: usize,
    parent_start_position: tree_sitter::Point,
    depth: usize,
) -> SelectionRange {
    // Safety check
    if depth >= MAX_INJECTION_DEPTH {
        return build_injected_selection_range(*node, root, parent_start_position);
    }

    // Detect the nested injection
    let injection_info = injection::detect_injection_with_content(
        node,
        root,
        text,
        Some(injection_query),
        base_language,
    );

    let Some((hierarchy, content_node, pattern_index)) = injection_info else {
        return build_injected_selection_range(*node, root, parent_start_position);
    };

    // Get nested language name from hierarchy (last element)
    if hierarchy.len() < 2 {
        return build_injected_selection_range(*node, root, parent_start_position);
    }
    let nested_lang = hierarchy.last().unwrap().clone();

    // Check offset
    let offset = parse_offset_directive_for_pattern(injection_query, pattern_index);

    // Ensure nested language is loaded
    let load_result = coordinator.ensure_language_loaded(&nested_lang);
    if !load_result.success {
        return build_injected_selection_range(*node, root, parent_start_position);
    }

    // Acquire parser for nested language
    let Some(mut nested_parser) = parser_pool.acquire(&nested_lang) else {
        return build_injected_selection_range(*node, root, parent_start_position);
    };

    // Extract nested content text
    let (nested_text, nested_effective_start, nested_effective_start_byte) =
        if let Some(off) = offset {
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range_with_text(text, byte_range, off);
            let effective_text = &text[effective.start..effective.end];

            // Calculate effective start position (relative to parent injection)
            let effective_start_pos = calculate_nested_start_position(
                parent_start_position,
                content_node.start_position(),
                off.start_row as usize,
                off.start_column as usize,
            );

            (effective_text, effective_start_pos, effective.start)
        } else {
            let nested_start_pos = calculate_nested_start_position(
                parent_start_position,
                content_node.start_position(),
                0,
                0,
            );
            (
                &text[content_node.byte_range()],
                nested_start_pos,
                content_node.start_byte(),
            )
        };

    // Parse nested content
    let Some(nested_tree) = nested_parser.parse(nested_text, None) else {
        parser_pool.release(nested_lang.to_string(), nested_parser);
        return build_injected_selection_range(*node, root, parent_start_position);
    };

    let nested_relative_byte = cursor_byte.saturating_sub(nested_effective_start_byte);
    let nested_root = nested_tree.root_node();

    let Some(nested_node) =
        nested_root.descendant_for_byte_range(nested_relative_byte, nested_relative_byte)
    else {
        parser_pool.release(nested_lang.to_string(), nested_parser);
        return build_injected_selection_range(*node, root, parent_start_position);
    };

    // Check for even deeper nesting (recursive)
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
            // Even deeper nesting - recurse
            build_nested_injection_selection(
                &nested_node,
                &nested_root,
                nested_text,
                deep_inj_query.as_ref(),
                &nested_lang,
                coordinator,
                parser_pool,
                nested_relative_byte,
                nested_effective_start,
                depth + 1,
            )
        } else {
            build_injected_selection_range(nested_node, &nested_root, nested_effective_start)
        }
    } else {
        build_injected_selection_range(nested_node, &nested_root, nested_effective_start)
    };

    // Chain nested selection to parent injected content
    // Get the parent's selection starting from content_node's parent
    let parent_selection = content_node
        .parent()
        .map(|parent| build_injected_selection_range(parent, root, parent_start_position));

    let result = chain_injected_to_host(nested_selection, parent_selection);

    parser_pool.release(nested_lang.to_string(), nested_parser);
    result
}

/// Calculate the start position for nested injection relative to host document
fn calculate_nested_start_position(
    parent_start: tree_sitter::Point,
    content_start: tree_sitter::Point,
    offset_rows: usize,
    offset_cols: usize,
) -> tree_sitter::Point {
    // The content_start is relative to the parent injection
    // We need to add the parent's start position and apply any offset
    let row = parent_start.row + content_start.row + offset_rows;
    let col = if content_start.row == 0 {
        // First row of content - add parent's column
        parent_start.column + content_start.column + offset_cols
    } else {
        // Later rows - column is absolute within the parent
        content_start.column + offset_cols
    };
    tree_sitter::Point::new(row, col)
}

/// Build selection range for nodes in injected content
///
/// This builds SelectionRange from injected AST nodes, adjusting positions
/// to be relative to the host document (not the injection slice).
/// Nodes with identical ranges are deduplicated (LSP spec requires strictly expanding ranges).
fn build_injected_selection_range(
    node: Node,
    injected_root: &Node,
    content_start_position: tree_sitter::Point,
) -> SelectionRange {
    // Adjust the node's range to be relative to the host document
    let adjusted_range = adjust_range_to_host(node, content_start_position);
    let node_byte_range = node.byte_range();

    // Build parent chain within injected content, skipping nodes with same range
    let parent =
        find_next_distinct_parent(node, &node_byte_range, injected_root).map(|parent_node| {
            // Stop at the root of the injected content
            if parent_node.id() == injected_root.id() {
                // The root of injected content - adjust its range too
                Box::new(SelectionRange {
                    range: adjust_range_to_host(parent_node, content_start_position),
                    parent: None, // Will be connected to host in chain_injected_to_host
                })
            } else {
                Box::new(build_injected_selection_range(
                    parent_node,
                    injected_root,
                    content_start_position,
                ))
            }
        });

    SelectionRange {
        range: adjusted_range,
        parent,
    }
}

/// Find the next parent node that has a different (larger) range than the current node.
/// This ensures the LSP selection range hierarchy is strictly expanding.
fn find_next_distinct_parent<'a>(
    node: Node<'a>,
    current_range: &std::ops::Range<usize>,
    root: &Node,
) -> Option<Node<'a>> {
    let mut current = node.parent();
    while let Some(parent) = current {
        let parent_range = parent.byte_range();
        // If parent has a different range, use it
        if parent_range != *current_range {
            return Some(parent);
        }
        // If we've reached root, return it even if same range
        if parent.id() == root.id() {
            return Some(parent);
        }
        current = parent.parent();
    }
    None
}

/// Adjust a node's range from injection-relative to host-document-relative coordinates
fn adjust_range_to_host(node: Node, content_start_position: tree_sitter::Point) -> Range {
    let node_start = node.start_position();
    let node_end = node.end_position();

    // Add the content node's starting position to the relative positions
    let adjusted_start = if node_start.row == 0 {
        // First row - add column offset
        Position::new(
            (content_start_position.row + node_start.row) as u32,
            (content_start_position.column + node_start.column) as u32,
        )
    } else {
        // Subsequent rows - only add row offset, column is absolute within injection
        Position::new(
            (content_start_position.row + node_start.row) as u32,
            node_start.column as u32,
        )
    };

    let adjusted_end = if node_end.row == 0 {
        Position::new(
            (content_start_position.row + node_end.row) as u32,
            (content_start_position.column + node_end.column) as u32,
        )
    } else {
        Position::new(
            (content_start_position.row + node_end.row) as u32,
            node_end.column as u32,
        )
    };

    Range::new(adjusted_start, adjusted_end)
}

/// Chain the injected selection hierarchy to the host document hierarchy
fn chain_injected_to_host(
    mut injected: SelectionRange,
    host: Option<SelectionRange>,
) -> SelectionRange {
    // Find the end of the injected chain (the injected root)
    fn find_and_connect_tail(selection: &mut SelectionRange, host: Option<SelectionRange>) {
        if selection.parent.is_none() {
            // This is the tail - connect to host
            selection.parent = host.map(Box::new);
        } else if let Some(ref mut parent) = selection.parent {
            find_and_connect_tail(parent, host);
        }
    }

    find_and_connect_tail(&mut injected, host);
    injected
}

/// Calculate the effective LSP Range after applying offset to content node
fn calculate_effective_lsp_range(
    text: &str,
    content_node: &Node,
    offset: injection::InjectionOffset,
) -> Range {
    let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
    let effective = calculate_effective_range_with_text(text, byte_range, offset);

    // Convert byte positions to LSP positions
    let mapper = crate::text::PositionMapper::new(text);
    let start_pos = mapper
        .byte_to_position(effective.start)
        .unwrap_or_else(|| Position::new(0, 0));
    let end_pos = mapper
        .byte_to_position(effective.end)
        .unwrap_or_else(|| Position::new(0, 0));

    Range::new(start_pos, end_pos)
}

/// Build selection hierarchy with injection content node included
///
/// Shared logic for injection-aware selection range building.
fn build_injection_aware_selection(node: Node, content_node: Node) -> SelectionRange {
    let inner_selection = build_selection_range(node);

    // Check if content_node is already in the parent chain
    if is_node_in_selection_chain(&inner_selection, &content_node) {
        // content_node is already in the chain, just return as-is
        inner_selection
    } else {
        // Need to splice content_node into the hierarchy
        splice_injection_content_into_hierarchy(inner_selection, content_node)
    }
}

/// Build selection hierarchy with effective range instead of full content node range
///
/// When an offset directive adjusts the injection boundaries, we use the effective
/// range in the selection hierarchy. This ensures that excluded regions (like `---`
/// boundaries in YAML frontmatter) are not included in the selection.
fn build_injection_aware_selection_with_effective_range(
    node: Node,
    content_node: Node,
    effective_range: Range,
) -> SelectionRange {
    let content_node_range = node_to_range(content_node);

    // Build base selection from the starting node
    let inner_selection = build_selection_range(node);

    // If the starting node IS the content node, replace its range with effective range
    if ranges_equal(&inner_selection.range, &content_node_range) {
        return SelectionRange {
            range: effective_range,
            parent: inner_selection.parent.map(|p| {
                Box::new(replace_range_in_chain(
                    *p,
                    content_node_range,
                    effective_range,
                ))
            }),
        };
    }

    // Check if content_node is already in the parent chain
    if is_node_in_selection_chain(&inner_selection, &content_node) {
        // content_node is in the chain - replace its range with effective range
        replace_range_in_chain(inner_selection, content_node_range, effective_range)
    } else {
        // Need to splice effective range into the hierarchy
        splice_effective_range_into_hierarchy(inner_selection, effective_range, &content_node)
    }
}

/// Replace a specific range in the selection chain with the effective range
fn replace_range_in_chain(
    selection: SelectionRange,
    target_range: Range,
    effective_range: Range,
) -> SelectionRange {
    if ranges_equal(&selection.range, &target_range) {
        // Found the target - replace with effective range
        SelectionRange {
            range: effective_range,
            parent: selection
                .parent
                .map(|p| Box::new(replace_range_in_chain(*p, target_range, effective_range))),
        }
    } else {
        // Continue up the chain
        SelectionRange {
            range: selection.range,
            parent: selection
                .parent
                .map(|p| Box::new(replace_range_in_chain(*p, target_range, effective_range))),
        }
    }
}

/// Splice effective range into hierarchy at the appropriate level
fn splice_effective_range_into_hierarchy(
    selection: SelectionRange,
    effective_range: Range,
    content_node: &Node,
) -> SelectionRange {
    // Similar to splice_injection_content_into_hierarchy but uses effective_range
    rebuild_with_effective_range(selection, effective_range, content_node)
}

/// Rebuild selection hierarchy, inserting the effective range at the right place
fn rebuild_with_effective_range(
    selection: SelectionRange,
    effective_range: Range,
    content_node: &Node,
) -> SelectionRange {
    // If current selection range is smaller than or equal to effective_range,
    // we need to continue up the chain
    if range_contains(&effective_range, &selection.range) {
        // Current range is inside effective_range
        let new_parent = match selection.parent {
            Some(parent) => {
                let parent_selection = *parent;
                if range_contains(&parent_selection.range, &effective_range)
                    && !ranges_equal(&parent_selection.range, &effective_range)
                {
                    // Parent is larger than effective_range, insert effective_range here
                    let effective_selection = SelectionRange {
                        range: effective_range,
                        parent: Some(Box::new(rebuild_with_effective_range(
                            parent_selection,
                            effective_range,
                            content_node,
                        ))),
                    };
                    Some(Box::new(effective_selection))
                } else {
                    // Keep going up
                    Some(Box::new(rebuild_with_effective_range(
                        parent_selection,
                        effective_range,
                        content_node,
                    )))
                }
            }
            None => {
                // No parent, but we're inside effective_range - add effective_range as parent
                Some(Box::new(SelectionRange {
                    range: effective_range,
                    parent: content_node
                        .parent()
                        .map(|p| Box::new(build_selection_range(p))),
                }))
            }
        };

        SelectionRange {
            range: selection.range,
            parent: new_parent,
        }
    } else {
        // Current range is not inside effective_range, just continue normally
        selection
    }
}

/// Check if cursor byte position is within the effective range after applying offset
fn is_cursor_within_effective_range(
    text: &str,
    content_node: &Node,
    cursor_byte: usize,
    offset: injection::InjectionOffset,
) -> bool {
    let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
    let effective_range = calculate_effective_range_with_text(text, byte_range, offset);
    cursor_byte >= effective_range.start && cursor_byte < effective_range.end
}

/// Check if a node's range is already present in the selection chain
fn is_node_in_selection_chain(selection: &SelectionRange, target_node: &Node) -> bool {
    let target_range = node_to_range(*target_node);
    let mut current = Some(selection);

    while let Some(sel) = current {
        if sel.range == target_range {
            return true;
        }
        current = sel.parent.as_ref().map(|p| p.as_ref());
    }

    false
}

/// Splice the injection content node into the selection hierarchy
///
/// The content_node represents the boundary of the injected region.
/// We need to insert it at the appropriate level in the hierarchy.
fn splice_injection_content_into_hierarchy(
    selection: SelectionRange,
    content_node: Node,
) -> SelectionRange {
    let content_range = node_to_range(content_node);

    // Find the first ancestor in the chain that fully contains the content_node
    // and insert content_node just before it
    rebuild_with_injection_boundary(selection, content_range, &content_node)
}

/// Rebuild selection hierarchy, inserting the injection content node at the right place
fn rebuild_with_injection_boundary(
    selection: SelectionRange,
    content_range: Range,
    content_node: &Node,
) -> SelectionRange {
    // If current selection range is smaller than or equal to content_range,
    // we need to continue up the chain
    if range_contains(&content_range, &selection.range) {
        // Current range is inside content_range
        // Continue building, but when we reach content_node's parent level,
        // insert content_node
        let new_parent = match selection.parent {
            Some(parent) => {
                let parent_selection = *parent;
                if range_contains(&parent_selection.range, &content_range)
                    && !ranges_equal(&parent_selection.range, &content_range)
                {
                    // Parent is larger than content_range, insert content_node here
                    let content_selection = SelectionRange {
                        range: content_range,
                        parent: Some(Box::new(rebuild_with_injection_boundary(
                            parent_selection,
                            content_range,
                            content_node,
                        ))),
                    };
                    Some(Box::new(content_selection))
                } else {
                    // Keep going up
                    Some(Box::new(rebuild_with_injection_boundary(
                        parent_selection,
                        content_range,
                        content_node,
                    )))
                }
            }
            None => {
                // No parent, but we're inside content_range - add content_node as parent
                Some(Box::new(SelectionRange {
                    range: content_range,
                    parent: content_node
                        .parent()
                        .map(|p| Box::new(build_selection_range(p))),
                }))
            }
        };

        SelectionRange {
            range: selection.range,
            parent: new_parent,
        }
    } else {
        // Current range is not inside content_range, just continue normally
        selection
    }
}

/// Check if outer range fully contains inner range
fn range_contains(outer: &Range, inner: &Range) -> bool {
    (outer.start.line < inner.start.line
        || (outer.start.line == inner.start.line && outer.start.character <= inner.start.character))
        && (outer.end.line > inner.end.line
            || (outer.end.line == inner.end.line && outer.end.character >= inner.end.character))
}

/// Check if two ranges are equal
fn ranges_equal(a: &Range, b: &Range) -> bool {
    a.start == b.start && a.end == b.end
}

/// Handle textDocument/selectionRange request
///
/// Returns selection ranges that expand intelligently by syntax boundaries.
///
/// # Arguments
/// * `document` - The document
/// * `positions` - The requested positions
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range(
    document: &DocumentHandle,
    positions: &[Position],
) -> Option<Vec<SelectionRange>> {
    // Delegate to the injection-aware version without injection query
    handle_selection_range_with_injection(document, positions, None, None)
}

/// Handle textDocument/selectionRange request with injection awareness
///
/// Returns selection ranges that expand intelligently by syntax boundaries,
/// taking into account language injections and their offset directives.
///
/// # Arguments
/// * `document` - The document
/// * `positions` - The requested positions
/// * `injection_query` - Optional injection query for detecting language injections
/// * `base_language` - Optional base language of the document
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range_with_injection(
    document: &DocumentHandle,
    positions: &[Position],
    injection_query: Option<&Query>,
    base_language: Option<&str>,
) -> Option<Vec<SelectionRange>> {
    // Create position mapper via document abstraction
    let mapper = document.position_mapper();
    let text = document.text();

    let ranges = positions
        .iter()
        .map(|pos| {
            // Convert position to byte offset
            let byte_offset = mapper.position_to_byte(*pos)?;

            // Get the tree
            let tree = document.tree()?;
            let root = tree.root_node();

            // Convert position to point for tree-sitter
            let point = position_to_point(pos);

            // Find the smallest node containing this position
            let node = root.descendant_for_point_range(point, point)?;

            // Build the selection range hierarchy with injection awareness
            if let Some(lang) = base_language {
                Some(build_selection_range_with_injection_and_offset(
                    node,
                    &root,
                    text,
                    injection_query,
                    lang,
                    byte_offset,
                ))
            } else {
                Some(build_selection_range(node))
            }
        })
        .collect::<Option<Vec<_>>>()?;

    Some(ranges)
}

/// Handle textDocument/selectionRange request with full injection parsing support
///
/// This is the most complete version that parses injected content and builds
/// selection hierarchies from the injected language's AST.
///
/// # Arguments
/// * `document` - The document
/// * `positions` - The requested positions
/// * `injection_query` - Optional injection query for detecting language injections
/// * `base_language` - Optional base language of the document
/// * `coordinator` - Language coordinator for parser configuration
/// * `parser_pool` - Parser pool for acquiring/releasing parsers
///
/// # Returns
/// Selection ranges for each position, or None if unable to compute
pub fn handle_selection_range_with_parsed_injection(
    document: &DocumentHandle,
    positions: &[Position],
    injection_query: Option<&Query>,
    base_language: Option<&str>,
    coordinator: &LanguageCoordinator,
    parser_pool: &mut DocumentParserPool,
) -> Option<Vec<SelectionRange>> {
    let text = document.text();

    let ranges = positions
        .iter()
        .map(|pos| {
            // Get the tree
            let tree = document.tree()?;
            let root = tree.root_node();

            // Convert position to point for tree-sitter
            let point = position_to_point(pos);

            // Find the smallest node containing this position
            let node = root.descendant_for_point_range(point, point)?;

            // Calculate the byte offset for the cursor position
            let cursor_byte_offset = {
                let mapper = crate::text::PositionMapper::new(text);
                mapper.position_to_byte(*pos).unwrap_or(node.start_byte())
            };

            // Build the selection range hierarchy with full injection parsing
            if let Some(lang) = base_language {
                Some(build_selection_range_with_parsed_injection(
                    node,
                    &root,
                    text,
                    injection_query,
                    lang,
                    coordinator,
                    parser_pool,
                    cursor_byte_offset,
                ))
            } else {
                Some(build_selection_range(node))
            }
        })
        .collect::<Option<Vec<_>>>()?;

    Some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Point;

    #[test]
    fn test_position_to_point() {
        let pos = Position::new(5, 10);
        let point = position_to_point(&pos);
        assert_eq!(point.row, 5);
        assert_eq!(point.column, 10);
    }

    #[test]
    fn test_point_to_position() {
        let point = Point::new(3, 7);
        let pos = point_to_position(point);
        assert_eq!(pos.line, 3);
        assert_eq!(pos.character, 7);
    }

    #[test]
    fn test_selection_range_detects_injection() {
        // Test that selection range detects when cursor is inside an injection region
        // and includes the injection content node in the selection hierarchy
        use tree_sitter::{Parser, Query};

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Rust code with regex injection
        let text = r#"fn main() {
    let pattern = Regex::new(r"^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query for Regex::new
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
  (#set! injection.language "regex"))
        "#;

        let injection_query = Query::new(&language, injection_query_str).expect("valid query");

        // Position inside the regex string (the \d part at column 32)
        let cursor_pos = Position::new(1, 32);
        let point = position_to_point(&cursor_pos);

        // Find the node at cursor
        let node = root
            .descendant_for_point_range(point, point)
            .expect("should find node");

        // Call the new injection-aware function
        let selection =
            build_selection_range_with_injection(node, &root, text, Some(&injection_query), "rust");

        // The selection hierarchy should include the injection content node
        // Walk up the parent chain and check that we find a range matching
        // the string_content node (which is the injection.content capture)
        let mut found_injection_content = false;
        let mut current = Some(&selection);

        // The string_content "^\d+$" is at bytes 43-48 (line 1, col 31-36)
        // We need to find this range in the selection hierarchy
        while let Some(sel) = current {
            // Check if this range corresponds to string_content
            // string_content starts at line 1, col 31 and ends at line 1, col 36
            if sel.range.start.line == 1
                && sel.range.start.character == 31
                && sel.range.end.line == 1
                && sel.range.end.character == 36
            {
                found_injection_content = true;
                break;
            }
            current = sel.parent.as_ref().map(|p| p.as_ref());
        }

        assert!(
            found_injection_content,
            "Selection hierarchy should include injection content node (string_content)"
        );
    }

    #[test]
    fn test_selection_range_respects_offset_directive() {
        // Test that the offset directive is correctly parsed and applied
        // to determine whether cursor is within effective injection range.
        //
        // Since both "inside" and "outside" effective range produce similar
        // selection hierarchies when starting from the injection content node itself,
        // this test verifies the offset parsing and effective range calculation
        // by directly testing is_cursor_within_effective_range.
        use crate::text::PositionMapper;
        use tree_sitter::{Parser, Query};

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // Rust code with regex injection - the string content is "_^\d+$"
        let text = r#"fn main() {
    let pattern = Regex::new(r"_^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query with offset directive (0, 2, 0, 0)
        // This means effective range starts 2 bytes after capture start
        // string_content is at bytes 43-49, so effective range is 45-49
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

        // Get the string_content node
        let underscore_pos = Position::new(1, 31);
        let underscore_point = position_to_point(&underscore_pos);
        let underscore_byte = mapper.position_to_byte(underscore_pos).unwrap();

        let string_content_node = root
            .descendant_for_point_range(underscore_point, underscore_point)
            .expect("should find node");

        assert_eq!(string_content_node.kind(), "string_content");
        assert_eq!(string_content_node.start_byte(), 43);
        assert_eq!(string_content_node.end_byte(), 49);

        // Verify offset directive is correctly parsed for pattern 0
        let offset = parse_offset_directive_for_pattern(&injection_query, 0);
        assert!(offset.is_some(), "Offset directive should be found");
        let offset = offset.unwrap();
        assert_eq!(offset.start_row, 0);
        assert_eq!(offset.start_column, 2);
        assert_eq!(offset.end_row, 0);
        assert_eq!(offset.end_column, 0);

        // Test effective range checking
        // Cursor at underscore (byte 43) - should be OUTSIDE effective range (45-49)
        assert!(
            !is_cursor_within_effective_range(text, &string_content_node, underscore_byte, offset),
            "Cursor at byte 43 (underscore) should be OUTSIDE effective range 45-49"
        );

        // Cursor at caret position (byte 45) - should be INSIDE effective range
        let caret_pos = Position::new(1, 33);
        let caret_byte = mapper.position_to_byte(caret_pos).unwrap();
        assert_eq!(caret_byte, 45, "Caret should be at byte 45");

        assert!(
            is_cursor_within_effective_range(text, &string_content_node, caret_byte, offset),
            "Cursor at byte 45 (caret ^) should be INSIDE effective range 45-49"
        );

        // Cursor at position 44 (between underscore and caret) - should be OUTSIDE
        assert!(
            !is_cursor_within_effective_range(text, &string_content_node, 44, offset),
            "Cursor at byte 44 should be OUTSIDE effective range 45-49"
        );

        // Test that the full function correctly returns different results
        // based on whether cursor is inside or outside effective range.
        // Both return `build_selection_range(node)` when injection is not active,
        // so we verify through the internal logic: injection detection returns
        // Some when offset check passes, None-equivalent behavior when it doesn't.

        // Build selection at underscore position (outside effective range)
        let _selection_at_underscore = build_selection_range_with_injection_and_offset(
            string_content_node,
            &root,
            text,
            Some(&injection_query),
            "rust",
            underscore_byte,
        );

        // Build selection at caret position (inside effective range)
        let _selection_at_caret = build_selection_range_with_injection_and_offset(
            string_content_node,
            &root,
            text,
            Some(&injection_query),
            "rust",
            caret_byte,
        );

        // Both produce valid selection hierarchies - the difference is that
        // injection-specific processing only occurs when inside effective range.
        // This test verified the core offset logic; integration tests can
        // verify observable differences with more complex AST structures.
    }

    /// Test that selection range handles nested injections recursively.
    ///
    /// This is the core test for Sprint 5: when cursor is inside a nested injection region,
    /// the selection should expand through ALL injection levels' AST nodes.
    ///
    /// Test scenario:
    /// - Host: Rust code with a raw string literal containing YAML
    /// - First injection: YAML content
    /// - Nested injection: JSON embedded in a YAML value (using a custom injection query)
    /// - Cursor: inside the JSON content
    /// - Expected: Selection hierarchy includes nodes from JSON, YAML, and Rust
    ///
    /// Note: Since we don't have tree-sitter-json, we use a simpler test with YAML
    /// that contains what could be nested content, and verify the recursion mechanism
    /// is correctly invoked (by checking it doesn't crash and produces a valid hierarchy).
    #[test]
    fn test_selection_range_handles_nested_injection() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        // Setup: Create a coordinator with YAML language registered
        // We'll also register an injection query for YAML that could match nested content
        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());
        coordinator.register_language_for_test("rust", tree_sitter_rust::LANGUAGE.into());

        // Register an injection query for YAML that matches double-quoted scalars as "rust"
        // This creates a nested injection: Rust → YAML → Rust
        let yaml_injection_query_str = r#"
((double_quote_scalar) @injection.content
 (#set! injection.language "rust"))
        "#;
        let yaml_lang: tree_sitter::Language = tree_sitter_yaml::LANGUAGE.into();
        let yaml_injection_query =
            Query::new(&yaml_lang, yaml_injection_query_str).expect("valid yaml injection query");
        coordinator.register_injection_query_for_test("yaml", yaml_injection_query);

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Host document: Rust code with YAML that contains a "rust" string
        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        // The YAML contains a double-quoted string that will be injected as Rust
        // YAML content: title: "fn nested() {}"
        // The "fn nested() {}" will be treated as Rust code (nested injection)
        let text = r##"fn main() {
    let yaml = r#"title: "fn nested() {}""#;
}"##;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query for Rust → YAML
        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query =
            Query::new(&rust_language, injection_query_str).expect("valid injection query");

        // Position inside the nested Rust code: "fn nested() {}"
        // Line 0: fn main() {
        // Line 1:     let yaml = r#"title: "fn nested() {}""#;
        //                          ^------- string_content starts here (col 18)
        //                                  title: "fn nested() {}"
        //                                         ^ col 25 is 'f' in 'fn'
        let cursor_pos = Position::new(1, 33); // Inside "fn nested() {}"
        let point = position_to_point(&cursor_pos);

        let node = root
            .descendant_for_point_range(point, point)
            .expect("should find node");

        // Calculate cursor byte offset
        let mapper = crate::text::PositionMapper::new(text);
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();

        // Call the function that should handle nested injections
        let selection = build_selection_range_with_parsed_injection(
            node,
            &root,
            text,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            cursor_byte,
        );

        // Verify: The selection hierarchy should include nodes from ALL levels:
        // - Innermost: Rust AST nodes (from "fn nested() {}")
        // - Middle: YAML AST nodes (from the YAML content)
        // - Outer: Rust AST nodes (from the host document)
        //
        // Count selection levels - with nested injection we should have MORE levels
        // than single-level injection because we're parsing through multiple ASTs
        let mut level_count = 0;
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            level_count += 1;
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // With nested injection (Rust → YAML → Rust):
        // - Inner Rust nodes: identifier → function_item or similar (depends on cursor position)
        // - YAML nodes: double_quote_scalar → flow_node → block_mapping_pair → ...
        // - Outer Rust nodes: string_content → raw_string_literal → let_declaration → ...
        //
        // We expect significantly more levels than single injection (which had ~7)
        // With nested injection we should have ~9+ levels (deduplication removes same-range nodes)
        assert!(
            level_count >= 9,
            "Expected at least 9 selection levels with nested injection (Rust → YAML → Rust), got {}. \
             This indicates nested injection was not properly handled.",
            level_count
        );
    }

    /// Test that selection range parses injected content and builds hierarchy from injected AST.
    ///
    /// This is the core test for Sprint 3: when cursor is inside an injection region,
    /// the selection should expand through the INJECTED language's AST nodes, not just
    /// the host document's content node.
    ///
    /// Test scenario:
    /// - Host: Rust code with a raw string literal
    /// - Content: YAML text inside the string
    /// - Cursor: inside "awesome" in the YAML content
    /// - Expected: Selection hierarchy includes YAML AST nodes (e.g., `double_quote_scalar`)
    #[test]
    fn test_selection_range_parses_injected_content() {
        use crate::language::LanguageCoordinator;
        use tree_sitter::{Parser, Query};

        // Setup: Create a coordinator with YAML language registered
        let coordinator = LanguageCoordinator::new();
        coordinator.register_language_for_test("yaml", tree_sitter_yaml::LANGUAGE.into());

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Host document: Rust code with a string that we'll treat as YAML injection
        // Using Rust as host because we have tree-sitter-rust available
        let mut parser = Parser::new();
        let rust_language = tree_sitter_rust::LANGUAGE.into();
        parser
            .set_language(&rust_language)
            .expect("load rust grammar");

        // The string content `title: "awesome"\narray: ["xxxx"]` will be our "injected" YAML
        // Using r##"..."## to allow r#"..."# inside
        let text = r##"fn main() {
    let yaml = r#"title: "awesome"
array: ["xxxx"]"#;
}"##;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Create injection query that captures the raw_string_literal content as YAML
        let injection_query_str = r#"
(raw_string_literal
  (string_content) @injection.content
  (#set! injection.language "yaml"))
        "#;
        let injection_query =
            Query::new(&rust_language, injection_query_str).expect("valid injection query");

        // Position inside the YAML string - at "awesome"
        // Line 0: fn main() {
        // Line 1:     let yaml = r#"title: "awesome"
        //                          ^------- column 18 is start of string_content
        //                                 ^ column 25 is 't' in 'title', col 32 is 'a' in 'awesome'
        let cursor_pos = Position::new(1, 32); // line 1, col 32 = 'a' in "awesome"
        let point = position_to_point(&cursor_pos);

        let node = root
            .descendant_for_point_range(point, point)
            .expect("should find node");

        // Calculate cursor byte offset
        let mapper = crate::text::PositionMapper::new(text);
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();

        // Call the new function that parses injected content
        // This function should:
        // 1. Detect the injection (string_content with YAML)
        // 2. Parse the injected content as YAML
        // 3. Find the node at cursor in the YAML AST
        // 4. Build selection from YAML node through to host document
        let selection = build_selection_range_with_parsed_injection(
            node,
            &root,
            text,
            Some(&injection_query),
            "rust",
            &coordinator,
            &mut parser_pool,
            cursor_byte,
        );

        // Verify: The selection hierarchy should include YAML-specific nodes
        // We can't directly check node kinds from SelectionRange, but we can verify
        // that we have MORE selection levels than the host document alone would provide.
        // The extra levels come from the injected YAML AST (double_quote_scalar,
        // flow_node, block_mapping_pair, block_mapping, block_node, document, stream)

        // Count selection levels
        let mut level_count = 0;
        let mut curr = Some(&selection);
        while let Some(sel) = curr {
            level_count += 1;
            curr = sel.parent.as_ref().map(|p| p.as_ref());
        }

        // Without injection parsing: string_content → raw_string_literal → let_declaration → ... → source_file
        // With injection parsing: double_quote_scalar → flow_node → block_mapping_pair → ... → string_content → ...
        // We expect MORE levels with injection parsing (deduplication removes same-range nodes)
        assert!(
            level_count >= 7,
            "Expected at least 7 selection levels with injected YAML AST, got {}. \
             This indicates the injected content was not parsed.",
            level_count
        );
    }

    /// Test that build_selection_range deduplicates nodes with identical ranges.
    ///
    /// In Tree-sitter ASTs, it's common for a node and its parent to have the same
    /// byte range (e.g., `identifier` wrapped by `expression` with same range).
    /// LSP spec requires strictly expanding ranges, so we must skip duplicates.
    #[test]
    fn test_selection_range_deduplicates_same_range_nodes() {
        use tree_sitter::Parser;

        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        // In Rust, a simple expression like "foo" inside a function creates a chain
        // where some nodes may have identical ranges (e.g., identifier wrapped by expression)
        // Let's use a simple variable reference in a return statement
        let text = "fn f() { x }";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Find the identifier node for "x"
        let cursor_byte = 9; // position of "x"
        let node = root
            .descendant_for_byte_range(cursor_byte, cursor_byte)
            .expect("should find node");

        assert_eq!(node.kind(), "identifier", "Should find identifier node");

        // Build selection range
        let selection = build_selection_range(node);

        // Collect all ranges in the hierarchy
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

        // Check for duplicates - no two consecutive ranges should be identical
        for i in 1..ranges.len() {
            assert_ne!(
                ranges[i - 1],
                ranges[i],
                "Found duplicate ranges at positions {} and {}: {:?}. \
                 Selection range should deduplicate nodes with identical ranges.",
                i - 1,
                i,
                ranges[i]
            );
        }

        // Also verify we have reasonable number of levels (not too many due to duplicates)
        // The exact count depends on grammar, but with deduplication it should be reasonable
        // If there are duplicates, we'd have extra levels
        assert!(
            ranges.len() <= 8,
            "Expected at most 8 levels (with deduplication), got {}. Ranges: {:?}",
            ranges.len(),
            ranges
        );
    }
}
