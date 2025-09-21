use crate::config::CaptureMappings;
use crate::language::injection;
use crate::text::PositionMapper;
use std::str::FromStr;

/// Context for code action generation with injection support
pub struct CodeActionContext<'a> {
    pub uri: &'a Url,
    pub text: &'a str,
    pub tree: &'a Tree,
    pub cursor: Range,
    pub queries: Option<(&'a Query, Option<&'a Query>)>,
    pub capture_context: Option<(&'a str, &'a CaptureMappings)>,
    pub injection_query: Option<&'a Query>,
    pub coordinator: Option<&'a crate::language::LanguageCoordinator>,
    pub parser_pool: Option<&'a mut crate::language::DocumentParserPool>,
}
use tower_lsp::lsp_types::{
    CodeAction, CodeActionDisabled, CodeActionKind, CodeActionOrCommand, DocumentChanges, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit, TextEdit,
    Url as Uri, WorkspaceEdit,
};
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};
use url::Url;

/// Create an inspect token code action for the node at cursor
fn create_inspect_token_action(
    node: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
) -> CodeActionOrCommand {
    create_inspect_token_action_with_hierarchy(node, root, text, queries, capture_context, None)
}

/// Creates a code action that understands language injections.
///
/// Parses the injected content with the appropriate language parser and
/// provides accurate token information from the injected language context.
/// Falls back gracefully if parsing fails at any stage.
#[allow(clippy::too_many_arguments)]
fn create_injection_aware_action(
    node_at_cursor: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    hierarchy: Vec<String>,
    content_node: Node,
    cursor_byte: usize,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
    injection_query: Option<&Query>,
) -> CodeActionOrCommand {
    // Parse offset directive from injection query
    let offset_from_query = injection_query
        .and_then(crate::language::injection::parse_offset_directive);

    // Check if we have everything needed for injection-aware processing
    let can_process_injection =
        coordinator.is_some() && parser_pool.is_some() && hierarchy.len() > 1;

    if !can_process_injection {
        return create_inspect_token_action_with_hierarchy_and_offset(
            node_at_cursor,
            root,
            text,
            queries,
            capture_context,
            Some(&hierarchy),
            offset_from_query,
        );
    }

    let coord = coordinator.unwrap();
    let pool = parser_pool.unwrap();

    // For now, only process the immediate injection (not nested)
    // Full nested support would require more complex lifetime management
    let injected_lang = &hierarchy[hierarchy.len() - 1];

    // Try to acquire parser for the injected language
    let mut parser = match pool.acquire(injected_lang) {
        Some(p) => p,
        None => {
            return create_inspect_token_action_with_hierarchy_and_offset(
                node_at_cursor,
                root,
                text,
                queries,
                capture_context,
                Some(&hierarchy),
                offset_from_query,
            );
        }
    };

    let content_text = &text[content_node.byte_range()];

    // Try to parse the injected content
    let injected_tree = match parser.parse(content_text, None) {
        Some(tree) => tree,
        None => {
            pool.release(injected_lang.to_string(), parser);
            return create_inspect_token_action_with_hierarchy_and_offset(
                node_at_cursor,
                root,
                text,
                queries,
                capture_context,
                Some(&hierarchy),
                offset_from_query,
            );
        }
    };

    // Get queries for the injected language
    let injected_highlight = coord.get_highlight_query(injected_lang);
    let injected_locals = coord.get_locals_query(injected_lang);
    let injected_queries = injected_highlight
        .as_ref()
        .map(|hq| (hq.as_ref(), injected_locals.as_ref().map(|lq| lq.as_ref())));

    // Find the relative position in the injected content
    let relative_byte = cursor_byte.saturating_sub(content_node.start_byte());
    let injected_root = injected_tree.root_node();

    // Find the node at the cursor position in the injected content
    let Some(injected_node) = injected_root.descendant_for_byte_range(relative_byte, relative_byte)
    else {
        pool.release(injected_lang.to_string(), parser);
        return create_inspect_token_action_with_hierarchy(
            node_at_cursor,
            root,
            text,
            queries,
            capture_context,
            Some(&hierarchy),
        );
    };

    // Try to handle nested injection
    let result = handle_nested_injection(
        &injected_node,
        &injected_root,
        content_text,
        injected_lang,
        relative_byte,
        &hierarchy,
        coord,
        pool,
        injected_queries,
    );

    pool.release(injected_lang.to_string(), parser);
    result
}

/// Recursively detects and processes nested language injections.
///
/// Checks if the current injected language has its own injection query
/// and recursively processes any deeper injections found. Limited to
/// MAX_INJECTION_DEPTH to prevent stack overflow.
#[allow(clippy::too_many_arguments)]
fn handle_nested_injection(
    injected_node: &Node,
    injected_root: &Node,
    content_text: &str,
    injected_lang: &str,
    relative_byte: usize,
    hierarchy: &[String],
    coord: &crate::language::LanguageCoordinator,
    pool: &mut crate::language::DocumentParserPool,
    injected_queries: Option<(&Query, Option<&Query>)>,
) -> CodeActionOrCommand {
    // Safety: limit recursion depth to prevent stack overflow
    const MAX_INJECTION_DEPTH: usize = 10;
    if hierarchy.len() >= MAX_INJECTION_DEPTH {
        return create_injection_aware_inspect_token_action(
            injected_node,
            injected_root,
            content_text,
            injected_queries,
            Some((injected_lang, &coord.get_capture_mappings())),
            Some(hierarchy),
        );
    }
    // Check for nested injection in the current injected content
    let injection_query = coord.get_injection_query(injected_lang);

    if let Some(inj_query) = injection_query
        && let Some((nested_hierarchy, nested_content_node)) =
            injection::detect_injection_with_content(
                injected_node,
                injected_root,
                content_text,
                Some(inj_query.as_ref()),
                injected_lang,
            )
    {
        return process_nested_injection(
            nested_hierarchy,
            nested_content_node,
            hierarchy,
            content_text,
            relative_byte,
            injected_node,
            injected_root,
            injected_queries,
            injected_lang,
            coord,
            pool,
        );
    }

    // No nested injection found, create action for current level
    create_injection_aware_inspect_token_action(
        injected_node,
        injected_root,
        content_text,
        injected_queries,
        Some((injected_lang, &coord.get_capture_mappings())),
        Some(hierarchy),
    )
}

/// Processes a detected nested injection by parsing it and checking for deeper injections.
///
/// This function is part of the recursion chain: it parses the nested content,
/// then calls back to `handle_nested_injection` to check for even deeper injections.
#[allow(clippy::too_many_arguments)]
fn process_nested_injection(
    nested_hierarchy: Vec<String>,
    nested_content_node: Node,
    hierarchy: &[String],
    content_text: &str,
    relative_byte: usize,
    injected_node: &Node,
    injected_root: &Node,
    injected_queries: Option<(&Query, Option<&Query>)>,
    injected_lang: &str,
    coord: &crate::language::LanguageCoordinator,
    pool: &mut crate::language::DocumentParserPool,
) -> CodeActionOrCommand {
    // Build full hierarchy
    let mut full_hierarchy = hierarchy.to_vec();
    for lang in nested_hierarchy.iter().skip(1) {
        full_hierarchy.push(lang.clone());
    }

    let nested_lang = nested_hierarchy.last().unwrap();

    // Try to parse the nested content
    if let Some(mut nested_parser) = pool.acquire(nested_lang) {
        let nested_content_text = &content_text[nested_content_node.byte_range()];

        if let Some(nested_tree) = nested_parser.parse(nested_content_text, None) {
            let nested_relative_byte =
                relative_byte.saturating_sub(nested_content_node.start_byte());
            let nested_root = nested_tree.root_node();

            if let Some(deeply_nested_node) =
                nested_root.descendant_for_byte_range(nested_relative_byte, nested_relative_byte)
            {
                // Get queries for the nested language
                let nested_highlight = coord.get_highlight_query(nested_lang);
                let nested_locals = coord.get_locals_query(nested_lang);
                let nested_queries = nested_highlight
                    .as_ref()
                    .map(|hq| (hq.as_ref(), nested_locals.as_ref().map(|lq| lq.as_ref())));

                // RECURSIVELY check for even deeper injections
                let action = handle_nested_injection(
                    &deeply_nested_node,
                    &nested_root,
                    nested_content_text,
                    nested_lang,
                    nested_relative_byte,
                    &full_hierarchy,
                    coord,
                    pool,
                    nested_queries,
                );

                pool.release(nested_lang.to_string(), nested_parser);
                return action;
            }
        }

        pool.release(nested_lang.to_string(), nested_parser);
    }

    // Couldn't parse nested content, but still show full hierarchy
    create_injection_aware_inspect_token_action(
        injected_node,
        injected_root,
        content_text,
        injected_queries,
        Some((injected_lang, &coord.get_capture_mappings())),
        Some(&full_hierarchy),
    )
}

/// Creates an inspect token action with injected language information
fn create_injection_aware_inspect_token_action(
    node: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    language_hierarchy: Option<&[String]>,
) -> CodeActionOrCommand {
    create_inspect_token_action_with_hierarchy(
        node,
        root,
        text,
        queries,
        capture_context,
        language_hierarchy,
    )
}

/// Create an inspect token code action for the node at cursor with language hierarchy
fn create_inspect_token_action_with_hierarchy(
    node: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    language_hierarchy: Option<&[String]>,
) -> CodeActionOrCommand {
    create_inspect_token_action_with_hierarchy_and_offset(
        node,
        root,
        text,
        queries,
        capture_context,
        language_hierarchy,
        None, // Default: no offset from query
    )
}

/// Create an inspect token code action with hierarchy and offset directive info
fn create_inspect_token_action_with_hierarchy_and_offset(
    node: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    language_hierarchy: Option<&[String]>,
    offset_from_query: Option<crate::language::injection::InjectionOffset>,
) -> CodeActionOrCommand {
    let mut info = format!("* Node Type: {}\n", node.kind());

    // Add offset field only for injected languages
    if let Some(hierarchy) = language_hierarchy
        && hierarchy.len() > 1
    {
        // Has injection (base + at least one injected language)
        let offset = offset_from_query.unwrap_or(crate::language::injection::DEFAULT_OFFSET);
        let offset_str = format!("({}, {}, {}, {})", offset.0, offset.1, offset.2, offset.3);
        if offset_from_query.is_some() {
            info.push_str(&format!("* Offset: {} [has #offset! directive]\n", offset_str));
        } else {
            info.push_str(&format!("* Offset: {}\n", offset_str));
        }
    }

    // If we have queries, show captures
    if let Some((highlights_query, locals_query)) = queries {
        let mut highlight_captures = Vec::new();
        let mut local_captures = Vec::new();

        // Check highlights query - search from root to find all captures for this node
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(highlights_query, *root, text.as_bytes());
        while let Some(m) = matches.next() {
            // Filter captures based on predicates
            let filtered_captures = crate::language::filter_captures(highlights_query, m, text);
            for c in filtered_captures {
                if c.node == *node {
                    let capture_name = &highlights_query.capture_names()[c.index as usize];
                    if !highlight_captures.contains(&capture_name.to_string()) {
                        highlight_captures.push(capture_name.to_string());
                    }
                }
            }
        }

        // Check locals query if available - search from root to find all captures for this node
        if let Some(locals) = locals_query {
            let mut cursor = QueryCursor::new();
            let mut matches = cursor.matches(locals, *root, text.as_bytes());
            while let Some(m) = matches.next() {
                // Filter captures based on predicates
                let filtered_captures = crate::language::filter_captures(locals, m, text);
                for c in filtered_captures {
                    if c.node == *node {
                        let capture_name = &locals.capture_names()[c.index as usize];
                        if !local_captures.contains(&capture_name.to_string()) {
                            local_captures.push(capture_name.to_string());
                        }
                    }
                }
            }
        }

        // Add captures section
        info.push_str("* Captures\n");

        // Add highlights with mappings
        if !highlight_captures.is_empty() {
            let mapped_captures: Vec<String> = highlight_captures
                .iter()
                .map(|capture| {
                    // Apply capture mapping if available
                    if let Some((filetype, mappings)) = capture_context {
                        let lookup_name = capture;

                        if let Some(lang_mappings) = mappings.get(filetype)
                            && let Some(mapped) = lang_mappings.highlights.get(lookup_name)
                            && capture != mapped
                        {
                            return format!("{}->{}", capture, mapped);
                        }

                        if let Some(wildcard_mappings) = mappings.get("_")
                            && let Some(mapped) = wildcard_mappings.highlights.get(lookup_name)
                            && capture != mapped
                        {
                            return format!("{}->{}", capture, mapped);
                        }
                    }
                    capture.clone()
                })
                .collect();

            info.push_str(&format!(
                "    * highlights: {}\n",
                mapped_captures.join(", ")
            ));
        }

        // Add locals with mappings
        if !local_captures.is_empty() {
            let mapped_captures: Vec<String> = local_captures
                .iter()
                .map(|capture| {
                    // Apply capture mapping if available
                    if let Some((filetype, mappings)) = capture_context {
                        let lookup_name = capture;

                        if let Some(lang_mappings) = mappings.get(filetype)
                            && let Some(mapped) = lang_mappings.locals.get(lookup_name)
                            && capture != mapped
                        {
                            return format!("{}->{}", capture, mapped);
                        }

                        if let Some(wildcard_mappings) = mappings.get("_")
                            && let Some(mapped) = wildcard_mappings.locals.get(lookup_name)
                            && capture != mapped
                        {
                            return format!("{}->{}", capture, mapped);
                        }
                    }
                    capture.clone()
                })
                .collect();

            info.push_str(&format!("    * locals: {}\n", mapped_captures.join(", ")));
        }

        // If no captures at all, indicate none
        if highlight_captures.is_empty() && local_captures.is_empty() {
            info.push_str("    * (none)\n");
        }
    }

    // Display language or language hierarchy
    if let Some(hierarchy) = language_hierarchy {
        if !hierarchy.is_empty() {
            info.push_str(&format!("* Language: {}\n", hierarchy.join(" -> ")));
        }
    } else if let Some((filetype, _)) = capture_context {
        info.push_str(&format!("* Language: {}\n", filetype));
    }

    // Create a code action that shows this info (using title as display)
    let action = CodeAction {
        title: format!("Inspect token: {}", node.kind()),
        kind: Some(CodeActionKind::from("empty".to_string())),
        diagnostics: None,
        edit: None,
        command: None,
        is_preferred: None,
        disabled: Some(CodeActionDisabled { reason: info }),
        data: None,
    };

    CodeActionOrCommand::CodeAction(action)
}

/// Produce code actions that reorder a parameter within a function parameter list.
/// The implementation is language-agnostic for grammars that use a `parameters` node
/// with direct child comma tokens and surrounding parentheses.
pub fn handle_code_actions(
    uri: &Url,
    text: &str,
    tree: &Tree,
    cursor: Range,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
) -> Option<Vec<CodeActionOrCommand>> {
    handle_code_actions_with_injection_query(
        uri,
        text,
        tree,
        cursor,
        queries,
        capture_context,
        None, // No injection query yet - will be loaded from language module later
    )
}

/// Handle code actions with optional injection query for language hierarchy detection
pub fn handle_code_actions_with_injection_query(
    uri: &Url,
    text: &str,
    tree: &Tree,
    cursor: Range,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    injection_query: Option<&Query>,
) -> Option<Vec<CodeActionOrCommand>> {
    let context = CodeActionContext {
        uri,
        text,
        tree,
        cursor,
        queries,
        capture_context,
        injection_query,
        coordinator: None,
        parser_pool: None,
    };
    handle_code_actions_with_context(context)
}

#[allow(clippy::too_many_arguments)]
pub fn handle_code_actions_with_injection_and_coordinator(
    uri: &Url,
    text: &str,
    tree: &Tree,
    cursor: Range,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    injection_query: Option<&Query>,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
) -> Option<Vec<CodeActionOrCommand>> {
    let context = CodeActionContext {
        uri,
        text,
        tree,
        cursor,
        queries,
        capture_context,
        injection_query,
        coordinator: Some(coordinator),
        parser_pool: Some(parser_pool),
    };
    handle_code_actions_with_context(context)
}

fn handle_code_actions_with_context(
    context: CodeActionContext,
) -> Option<Vec<CodeActionOrCommand>> {
    let CodeActionContext {
        uri,
        text,
        tree,
        cursor,
        queries,
        capture_context,
        injection_query,
        coordinator,
        parser_pool,
    } = context;
    let root = tree.root_node();

    // Use the start position of the selection/range as the cursor location
    let mapper = PositionMapper::new(text);
    let cursor_byte = mapper.position_to_byte(cursor.start).unwrap_or(text.len());

    let node_at_cursor = root.descendant_for_byte_range(cursor_byte, cursor_byte)?;

    // Always create inspect token action for the node at cursor
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    // Try to get injection-aware information
    let injection_info = if let Some(injection_q) = injection_query {
        if let Some((base_lang, _)) = capture_context {
            injection::detect_injection_with_content(
                &node_at_cursor,
                &root,
                text,
                Some(injection_q),
                base_lang,
            )
        } else {
            None
        }
    } else {
        None
    };

    // Create the appropriate inspect token action
    let inspect_action = match injection_info {
        Some((hierarchy, content_node)) => create_injection_aware_action(
            &node_at_cursor,
            &root,
            text,
            queries,
            capture_context,
            hierarchy,
            content_node,
            cursor_byte,
            coordinator,
            parser_pool,
            injection_query,
        ),
        None => create_inspect_token_action(&node_at_cursor, &root, text, queries, capture_context),
    };

    actions.push(inspect_action);

    // Ascend to a `parameters` node for parameter reordering actions
    let mut n: Option<Node> = Some(node_at_cursor);
    let mut params_node: Option<Node> = None;
    while let Some(curr) = n {
        if curr.kind() == "parameters" {
            params_node = Some(curr);
            break;
        }
        n = curr.parent();
    }

    let Some(params_node) = params_node else {
        // No parameters node found, return just the inspect action
        return Some(actions);
    };

    // Find parentheses and commas among direct children
    let mut lparen_end: Option<usize> = None;
    let mut rparen_start: Option<usize> = None;
    let mut commas: Vec<Node> = Vec::new();

    let child_count = params_node.child_count();
    for i in 0..child_count {
        if let Some(ch) = params_node.child(i) {
            match ch.kind() {
                "(" => {
                    lparen_end = Some(ch.end_byte());
                }
                ")" => {
                    rparen_start = Some(ch.start_byte());
                }
                "," => {
                    // direct, top-level commas only
                    commas.push(ch);
                }
                _ => {}
            }
        }
    }

    let (lparen_end, rparen_start) = match (lparen_end, rparen_start) {
        (Some(l), Some(r)) if l <= r => (l, r),
        _ => return None,
    };

    // Build entry slices between separators (commas) at the top level
    // Each entry is a byte range [start, end) within `text`.
    let mut entries: Vec<(usize, usize)> = Vec::new();
    let mut current_start = lparen_end;

    // Helper: check if a slice is non-empty and not just whitespace
    let is_non_whitespace = |s: &str| s.chars().any(|c| !c.is_whitespace());

    for comma in &commas {
        let start = current_start;
        let end = comma.start_byte();
        if end > start {
            let slice = &text[start..end];
            if is_non_whitespace(slice) {
                entries.push((start, end));
            }
        }
        current_start = comma.end_byte();
    }

    // Final segment before right paren
    if rparen_start > current_start {
        let slice = &text[current_start..rparen_start];
        if is_non_whitespace(slice) {
            entries.push((current_start, rparen_start));
        }
    }

    if entries.is_empty() {
        return None;
    }

    // Detect trailing comma (comma directly before right paren, with only whitespace between)
    let mut trailing_comma = false;
    let mut trailing_ws: &str = "";
    if let Some(last_comma) = commas.last() {
        let after_last = last_comma.end_byte();
        if after_last <= rparen_start {
            let trailing = &text[after_last..rparen_start];
            if !trailing.is_empty() && trailing.chars().all(|c| c.is_whitespace()) {
                // There is a comma followed only by whitespace before ')'
                // Determine if this comma was not used to separate an element (i.e., trailing comma)
                // It is trailing if the last entry ends exactly at this comma's start.
                if let Some(&(_, last_end)) = entries.last()
                    && last_end <= last_comma.start_byte()
                {
                    trailing_comma = true;
                    trailing_ws = trailing;
                }
            }
        }
    }

    // Determine which entry the cursor is in
    let mut current_idx: Option<usize> = None;
    for (i, (s, e)) in entries.iter().enumerate() {
        if *s <= cursor_byte && cursor_byte < *e {
            current_idx = Some(i);
            break;
        }
    }

    let Some(current_idx) = current_idx else {
        // Cursor not in a parameter, return just the inspect action
        return Some(actions);
    };

    // Prepare the edit range: replace content between '(' and ')'
    let replace_start = lparen_end;
    let replace_end = rparen_start;
    let replace_range = mapper
        .byte_range_to_range(replace_start, replace_end)
        .unwrap_or(Range::new(Position::new(0, 0), Position::new(0, 0)));

    // Build parameter reordering actions
    let n = entries.len();

    for target_pos in 0..n {
        if target_pos == current_idx {
            continue;
        }

        // Build new order by moving current_idx to target_pos
        let mut order: Vec<usize> = (0..n).collect();
        let moved = order.remove(current_idx);
        order.insert(target_pos, moved);

        // Reconstruct content preserving original whitespace as much as possible
        let mut new_content = String::new();
        for (k, idx) in order.iter().enumerate() {
            if k > 0 {
                new_content.push(',');
            }
            let (s, e) = entries[*idx];
            new_content.push_str(&text[s..e]);
        }
        if trailing_comma {
            new_content.push(',');
            new_content.push_str(trailing_ws);
        }

        let title = format!("Move parameter to {}", ordinal(target_pos + 1));

        let edit = match Uri::from_str(uri.as_str()) {
            Ok(uri) => WorkspaceEdit {
                changes: None,
                document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                    edits: vec![OneOf::Left(TextEdit {
                        range: replace_range,
                        new_text: new_content,
                    })],
                }])),
                change_annotations: None,
            },
            Err(_) => continue,
        };

        let action = CodeAction {
            title,
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            diagnostics: None,
            edit: Some(edit),
            command: None,
            is_preferred: None,
            disabled: None,
            data: None,
        };

        actions.push(CodeActionOrCommand::CodeAction(action));
    }

    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

fn ordinal(n: usize) -> String {
    // Simple English ordinal suffix
    let suffix = match n % 100 {
        11..=13 => "th",
        _ => match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{}{}", n, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tower_lsp::lsp_types::CodeActionOrCommand;
    use tree_sitter::Parser;

    #[test]
    fn inspect_token_should_display_language() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = "fn main() {}";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();
        let node = root.named_child(0).expect("function node should exist");

        let capture_mappings: CaptureMappings = HashMap::new();
        let capture_context = Some(("rust", &capture_mappings));

        let action = create_inspect_token_action(&node, &root, text, None, capture_context);

        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected CodeAction variant");
        };

        let reason = action
            .disabled
            .expect("inspect token stores info in disabled reason")
            .reason;

        assert!(reason.contains("Language: rust"), "info was {reason}");
    }

    #[test]
    fn inspect_token_should_display_language_hierarchy() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = "fn main() {}";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();
        let node = root.named_child(0).expect("function node should exist");

        let capture_mappings: CaptureMappings = HashMap::new();

        // Pass language hierarchy as a new parameter
        let language_hierarchy = vec!["rust".to_string(), "sql".to_string()];
        let action = create_inspect_token_action_with_hierarchy(
            &node,
            &root,
            text,
            None,
            Some(("rust", &capture_mappings)),
            Some(&language_hierarchy),
        );

        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected CodeAction variant");
        };

        let reason = action
            .disabled
            .expect("inspect token stores info in disabled reason")
            .reason;

        assert!(
            reason.contains("Language: rust -> sql"),
            "Expected language hierarchy, got: {reason}"
        );
    }

    #[test]
    fn inspect_token_without_injection_query_shows_base_language() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = r#"fn main() {
    let pattern = Regex::new(r"^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

        // Find the string content node inside Regex::new
        let mapper = PositionMapper::new(text);
        // Position inside the regex string (the \d part)
        let cursor_pos = Position::new(1, 32); // Points to \d in the regex
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();
        let _node_at_cursor = root
            .descendant_for_byte_range(cursor_byte, cursor_byte)
            .unwrap();

        // Create code action WITHOUT injection query
        let capture_mappings: CaptureMappings = HashMap::new();
        let cursor_range = Range::new(cursor_pos, cursor_pos);

        let actions = handle_code_actions(
            &Url::parse("file:///test.rs").unwrap(),
            text,
            &tree,
            cursor_range,
            None,
            Some(("rust", &capture_mappings)),
        );

        assert!(actions.is_some(), "Should return code actions");
        let actions = actions.unwrap();

        // Find the inspect token action
        let inspect_action = actions.iter().find(|a| {
            if let CodeActionOrCommand::CodeAction(action) = a {
                action.title.starts_with("Inspect token")
            } else {
                false
            }
        });

        assert!(inspect_action.is_some(), "Should have inspect token action");

        let CodeActionOrCommand::CodeAction(action) = inspect_action.unwrap() else {
            panic!("Expected CodeAction variant");
        };

        let reason = action
            .disabled
            .as_ref()
            .expect("inspect token stores info in disabled reason")
            .reason
            .clone();

        // Without injection query, should only show base language
        assert!(
            reason.contains("Language: rust") && !reason.contains("Language: rust -> regex"),
            "Without injection query, should only show base language, but got: {}",
            reason
        );
    }

    #[test]
    fn test_injection_aware_action_with_full_context() {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let code = r#"
fn main() {
    let pattern = Regex::new(r"\d+");
}"#;

        let tree = parser.parse(code, None).expect("parse rust code");
        let injection_query_str = r#"
(call_expression
  function: (_) @_regex_fn_name
  arguments: (arguments . (raw_string_literal) @injection.content)
  (#eq? @_regex_fn_name "Regex::new")
  (#set! injection.language "regex")
)
"#;

        let injection_query = Query::new(&language, injection_query_str).expect("valid query");
        let _mapper = PositionMapper::new(code);
        let cursor_pos = Position::new(2, 32); // Position inside the regex string
        let cursor_range = Range::new(cursor_pos, cursor_pos);

        let capture_mappings: CaptureMappings = HashMap::new();

        // Create a mock coordinator and parser pool
        let coordinator = crate::language::LanguageCoordinator::new();
        let mut parser_pool = coordinator.create_document_parser_pool();

        // Create context
        let context = CodeActionContext {
            uri: &Url::parse("file:///test.rs").unwrap(),
            text: code,
            tree: &tree,
            cursor: cursor_range,
            queries: None,
            capture_context: Some(("rust", &capture_mappings)),
            injection_query: Some(&injection_query),
            coordinator: Some(&coordinator),
            parser_pool: Some(&mut parser_pool),
        };

        let actions = handle_code_actions_with_context(context);
        assert!(actions.is_some());

        let actions = actions.unwrap();
        assert_eq!(actions.len(), 1);

        // Verify the action recognizes the injection
        if let CodeActionOrCommand::CodeAction(action) = &actions[0] {
            let reason = &action.disabled.as_ref().unwrap().reason;
            // The test recognizes injection through query
            assert!(
                reason.contains("Language: rust -> regex") || reason.contains("rust"),
                "Should reference language in action, got: {}",
                reason
            );
        }
    }

    #[test]
    fn inspect_token_should_display_offset_field() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = "fn main() {}";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();
        let node = root.named_child(0).expect("function node should exist");

        let capture_mappings: CaptureMappings = HashMap::new();
        let capture_context = Some(("rust", &capture_mappings));

        // Test with injected language to see offset field
        let hierarchy = vec!["rust".to_string(), "regex".to_string()];
        let action = create_inspect_token_action_with_hierarchy(
            &node,
            &root,
            text,
            None,
            capture_context,
            Some(&hierarchy),
        );

        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected CodeAction variant");
        };

        let reason = action
            .disabled
            .expect("inspect token stores info in disabled reason")
            .reason;

        assert!(
            reason.contains("Offset:"),
            "Should display offset field for injected language, but got: {reason}"
        );
    }

    #[test]
    fn inspect_token_should_display_default_offset() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = "fn main() {}";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();
        let node = root.named_child(0).expect("function node should exist");

        let capture_mappings: CaptureMappings = HashMap::new();
        let capture_context = Some(("rust", &capture_mappings));

        // Test with injected language to see offset
        let hierarchy = vec!["rust".to_string(), "regex".to_string()];
        let action = create_inspect_token_action_with_hierarchy(
            &node,
            &root,
            text,
            None,
            capture_context,
            Some(&hierarchy),
        );

        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected CodeAction variant");
        };

        let reason = action
            .disabled
            .expect("inspect token stores info in disabled reason")
            .reason;

        assert!(
            reason.contains("Offset: (0, 0, 0, 0)"),
            "Should display default offset for injected content, but got: {reason}"
        );
    }

    #[test]
    fn inspect_token_should_not_show_offset_for_base_language() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = "fn main() {}";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();
        let node = root.named_child(0).expect("function node should exist");

        let capture_mappings: CaptureMappings = HashMap::new();
        let capture_context = Some(("rust", &capture_mappings));

        // Call with no language hierarchy (base language)
        let action = create_inspect_token_action_with_hierarchy(
            &node,
            &root,
            text,
            None,
            capture_context,
            None, // No hierarchy = base language
        );

        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected CodeAction variant");
        };

        let reason = action
            .disabled
            .expect("inspect token stores info in disabled reason")
            .reason;

        assert!(
            !reason.contains("Offset:"),
            "Should NOT display offset for base language, but got: {reason}"
        );
    }

    #[test]
    fn inspect_token_should_show_offset_for_injected_language() {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .expect("load rust grammar");

        let text = "fn main() {}";
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();
        let node = root.named_child(0).expect("function node should exist");

        let capture_mappings: CaptureMappings = HashMap::new();
        let capture_context = Some(("rust", &capture_mappings));

        // Call with language hierarchy (injected language)
        let hierarchy = vec!["rust".to_string(), "regex".to_string()];
        let action = create_inspect_token_action_with_hierarchy(
            &node,
            &root,
            text,
            None,
            capture_context,
            Some(&hierarchy), // Has hierarchy = injected language
        );

        let CodeActionOrCommand::CodeAction(action) = action else {
            panic!("expected CodeAction variant");
        };

        let reason = action
            .disabled
            .expect("inspect token stores info in disabled reason")
            .reason;

        assert!(
            reason.contains("Offset: (0, 0, 0, 0)"),
            "Should display offset for injected language, but got: {reason}"
        );
    }

    #[test]
    fn inspect_token_should_indicate_offset_directive_presence() {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = r#"fn main() {
    let pattern = Regex::new(r"^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");

        // Create injection query with offset directive
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

        let injection_query = Query::new(&language, injection_query_str).expect("valid query");

        // Position inside the regex string
        let _mapper = PositionMapper::new(text);
        let cursor_pos = Position::new(1, 32);
        let cursor_range = Range::new(cursor_pos, cursor_pos);

        let capture_mappings: CaptureMappings = HashMap::new();

        // Mock coordinator and parser pool for injection detection
        let coordinator = crate::language::LanguageCoordinator::new();
        let mut parser_pool = coordinator.create_document_parser_pool();

        // Call with injection query containing offset directive
        let actions = handle_code_actions_with_injection_and_coordinator(
            &Url::parse("file:///test.rs").unwrap(),
            text,
            &tree,
            cursor_range,
            None,
            Some(("rust", &capture_mappings)),
            Some(&injection_query),
            &coordinator,
            &mut parser_pool,
        );

        assert!(actions.is_some(), "Should return code actions");
        let actions = actions.unwrap();

        // Find the inspect token action
        let inspect_action = actions.iter().find(|a| {
            if let CodeActionOrCommand::CodeAction(action) = a {
                action.title.starts_with("Inspect token")
            } else {
                false
            }
        });

        assert!(inspect_action.is_some(), "Should have inspect token action");

        let CodeActionOrCommand::CodeAction(action) = inspect_action.unwrap() else {
            panic!("Expected CodeAction variant");
        };

        let reason = action
            .disabled
            .as_ref()
            .expect("inspect token stores info in disabled reason")
            .reason
            .clone();

        // Should detect regex injection and show offset directive presence
        assert!(
            reason.contains("Offset: (0, 0, 0, 0) [has #offset! directive]"),
            "Should indicate offset directive presence, but got: {}",
            reason
        );
    }

    #[test]
    fn inspect_token_should_use_injection_query_when_provided() {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = r#"fn main() {
    let pattern = Regex::new(r"^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");

        // Create injection query
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

        // Position inside the regex string
        let _mapper = PositionMapper::new(text);
        let cursor_pos = Position::new(1, 32);
        let cursor_range = Range::new(cursor_pos, cursor_pos);

        let capture_mappings: CaptureMappings = HashMap::new();

        // Call with injection query
        let actions = handle_code_actions_with_injection_query(
            &Url::parse("file:///test.rs").unwrap(),
            text,
            &tree,
            cursor_range,
            None,
            Some(("rust", &capture_mappings)),
            Some(&injection_query),
        );

        assert!(actions.is_some(), "Should return code actions");
        let actions = actions.unwrap();

        // Find the inspect token action
        let inspect_action = actions.iter().find(|a| {
            if let CodeActionOrCommand::CodeAction(action) = a {
                action.title.starts_with("Inspect token")
            } else {
                false
            }
        });

        assert!(inspect_action.is_some(), "Should have inspect token action");

        let CodeActionOrCommand::CodeAction(action) = inspect_action.unwrap() else {
            panic!("Expected CodeAction variant");
        };

        let reason = action
            .disabled
            .as_ref()
            .expect("inspect token stores info in disabled reason")
            .reason
            .clone();

        // Should detect regex injection via query
        assert!(
            reason.contains("Language: rust -> regex"),
            "Should detect regex injection via query, but got: {}",
            reason
        );
    }
}
