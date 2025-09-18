use std::str::FromStr;

use crate::domain::{
    CodeAction, CodeActionDisabled, CodeActionKind, CodeActionOrCommand, Position, Range, TextEdit,
    Uri, WorkspaceEdit,
};
use lsp_types::{
    DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier, TextDocumentEdit,
};
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};
use url::Url;

use crate::domain::settings::CaptureMappings;
use crate::text::{PositionMapper, SimplePositionMapper};

/// Create an inspect token code action for the node at cursor
fn create_inspect_token_action(
    node: &Node,
    root: &Node,
    text: &str,
    queries: Option<(&Query, Option<&Query>)>,
    capture_context: Option<(&str, &CaptureMappings)>,
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

    if let Some((filetype, _)) = capture_context {
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
    let root = tree.root_node();

    // Use the start position of the selection/range as the cursor location
    let mapper = SimplePositionMapper::new(text);
    let cursor_byte = mapper.position_to_byte(cursor.start).unwrap_or(text.len());

    let node_at_cursor = root.descendant_for_byte_range(cursor_byte, cursor_byte)?;

    // Always create inspect token action for the node at cursor
    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    actions.push(create_inspect_token_action(
        &node_at_cursor,
        &root,
        text,
        queries,
        capture_context,
    ));

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
    use crate::domain::CodeActionOrCommand;
    use std::collections::HashMap;
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
}
