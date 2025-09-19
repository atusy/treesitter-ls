use crate::config::CaptureMappings;
use crate::text::PositionMapper;
use std::str::FromStr;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionDisabled, CodeActionKind, CodeActionOrCommand, DocumentChanges, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, TextDocumentEdit, TextEdit,
    Url as Uri, WorkspaceEdit,
};
use tree_sitter::{Node, Query, QueryCursor, QueryMatch, StreamingIterator, Tree};
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

/// Create an inspect token code action for the node at cursor with language hierarchy
fn create_inspect_token_action_with_hierarchy(
    node: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
    language_hierarchy: Option<&[String]>,
) -> CodeActionOrCommand {
    let mut info = format!("* Node Type: {}\n", node.kind());

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

/// Detect if we're inside an injected language region using injection queries
fn detect_injection_via_query(
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
        for capture in match_.captures {
            let capture_name = query.capture_names().get(capture.index as usize)?;

            // Check if this capture is @injection.content and contains our node
            if *capture_name == "injection.content" {
                // Check if our node is within this capture's range
                let captured_node = capture.node;
                if node.start_byte() >= captured_node.start_byte()
                    && node.end_byte() <= captured_node.end_byte()
                {
                    // Look for the language in query properties or captures
                    if let Some(language) = extract_injection_language(query, match_, text) {
                        return Some(vec![base_language.to_string(), language]);
                    }
                }
            }
        }
    }

    None
}

/// Extract the injection language from query properties or captures
fn extract_injection_language(
    query: &Query,
    match_: &QueryMatch,
    text: &str,
) -> Option<String> {
    // First check for #set! injection.language "..."
    for prop in query.property_settings(match_.pattern_index) {
        if prop.key.as_ref() == "injection.language"
            && let Some(value) = &prop.value
        {
            return Some(value.as_ref().to_string());
        }
    }

    // Handle dynamic language capture (@injection.language)
    for capture in match_.captures {
        let capture_name = query.capture_names().get(capture.index as usize)?;
        if *capture_name == "injection.language" {
            let lang_text = &text[capture.node.byte_range()];
            return Some(lang_text.to_string());
        }
    }

    None
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
    let root = tree.root_node();

    // Use the start position of the selection/range as the cursor location
    let mapper = PositionMapper::new(text);
    let cursor_byte = mapper.position_to_byte(cursor.start).unwrap_or(text.len());

    let node_at_cursor = root.descendant_for_byte_range(cursor_byte, cursor_byte)?;

    // Detect language injection using query if available
    let language_hierarchy = if let Some(injection_q) = injection_query {
        if let Some((base_lang, _)) = capture_context {
            detect_injection_via_query(&node_at_cursor, &root, text, Some(injection_q), base_lang)
        } else {
            None
        }
    } else {
        None
    };

    // Always create inspect token action for the node at cursor
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();

    if let Some(hierarchy) = language_hierarchy {
        actions.push(create_inspect_token_action_with_hierarchy(
            &node_at_cursor,
            &root,
            text,
            queries,
            capture_context,
            Some(&hierarchy),
        ));
    } else {
        actions.push(create_inspect_token_action(
            &node_at_cursor,
            &root,
            text,
            queries,
            capture_context,
        ));
    }

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
        let mapper = PositionMapper::new(text);
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


    #[test]
    fn inspect_token_should_detect_injection_via_query() {
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");

        let text = r#"fn main() {
    let pattern = Regex::new(r"^\d+$").unwrap();
}"#;
        let tree = parser.parse(text, None).expect("parse rust");
        let root = tree.root_node();

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

        // Find the string content node inside Regex::new
        let mapper = PositionMapper::new(text);
        // Position inside the regex string (the \d part)
        let cursor_pos = Position::new(1, 32); // Points to \d in the regex
        let cursor_byte = mapper.position_to_byte(cursor_pos).unwrap();
        let node_at_cursor = root
            .descendant_for_byte_range(cursor_byte, cursor_byte)
            .unwrap();

        // Detect injection using query
        let hierarchy = detect_injection_via_query(
            &node_at_cursor,
            &root,
            text,
            Some(&injection_query),
            "rust",
        );

        assert!(hierarchy.is_some(), "Should detect injection");
        let hierarchy = hierarchy.unwrap();
        assert_eq!(hierarchy, vec!["rust".to_string(), "regex".to_string()]);
    }
}
