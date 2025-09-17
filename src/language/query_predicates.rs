use regex::Regex;
use tree_sitter::{Query, QueryCapture, QueryMatch};

fn check_predicate(query: &Query, match_: &QueryMatch, capture: &QueryCapture, text: &str) -> bool {
    let general_predicates = query.general_predicates(match_.pattern_index);

    for predicate in general_predicates {
        if let Some(tree_sitter::QueryPredicateArg::Capture(capture_id)) = predicate.args.first() {
            if *capture_id != capture.index {
                continue;
            }

            let node = capture.node;
            let node_text = &text[node.start_byte()..node.end_byte()];

            match predicate.operator.as_ref() {
                "lua-match?" => {
                    if let Some(tree_sitter::QueryPredicateArg::String(pattern_str)) =
                        predicate.args.get(1)
                    {
                        if let Ok(parsed_pattern) = lua_pattern::parse(pattern_str) {
                            match lua_pattern::try_to_regex(&parsed_pattern, false, false) {
                                Ok(regex_str) => match Regex::new(&regex_str) {
                                    Ok(re) => {
                                        if !re.is_match(node_text) {
                                            return false;
                                        }
                                    }
                                    Err(err) => {
                                        eprintln!(
                                            "Failed to compile regex from lua-pattern: {} ({err})",
                                            regex_str
                                        );
                                    }
                                },
                                Err(err) => {
                                    eprintln!(
                                        "Failed to convert lua-pattern to regex: {} ({err:?})",
                                        pattern_str
                                    );
                                }
                            }
                        } else {
                            eprintln!("Invalid lua-pattern: {}", pattern_str);
                        }
                    }
                }
                "match?" => {
                    if let Some(tree_sitter::QueryPredicateArg::String(pattern_str)) =
                        predicate.args.get(1)
                        && let Ok(re) = Regex::new(pattern_str)
                        && !re.is_match(node_text)
                    {
                        return false;
                    }
                }
                "eq?" => {
                    if let Some(value_arg) = predicate.args.get(1) {
                        match value_arg {
                            tree_sitter::QueryPredicateArg::String(value_str) => {
                                if node_text != value_str.as_ref() {
                                    return false;
                                }
                            }
                            tree_sitter::QueryPredicateArg::Capture(other_capture_id) => {
                                if let Some(other_capture) = match_
                                    .captures
                                    .iter()
                                    .find(|c| c.index == *other_capture_id)
                                {
                                    let other_node = other_capture.node;
                                    let other_text =
                                        &text[other_node.start_byte()..other_node.end_byte()];
                                    if node_text != other_text {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                }
                "not-eq?" => {
                    if let Some(value_arg) = predicate.args.get(1) {
                        match value_arg {
                            tree_sitter::QueryPredicateArg::String(value_str) => {
                                if node_text == value_str.as_ref() {
                                    return false;
                                }
                            }
                            tree_sitter::QueryPredicateArg::Capture(other_capture_id) => {
                                if let Some(other_capture) = match_
                                    .captures
                                    .iter()
                                    .find(|c| c.index == *other_capture_id)
                                {
                                    let other_node = other_capture.node;
                                    let other_text =
                                        &text[other_node.start_byte()..other_node.end_byte()];
                                    if node_text == other_text {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    true
}

pub fn filter_captures<'a>(
    query: &Query,
    match_: &'a QueryMatch<'a, 'a>,
    text: &str,
) -> Vec<QueryCapture<'a>> {
    match_
        .captures
        .iter()
        .filter(|capture| check_predicate(query, match_, capture, text))
        .cloned()
        .collect()
}
