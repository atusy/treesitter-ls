use tree_sitter::{Query, QueryCapture, QueryMatch};

/// Check if a predicate matches for a given capture
pub fn check_predicate(
    query: &Query,
    match_: &QueryMatch,
    capture: &QueryCapture,
    text: &str,
) -> bool {
    // Check general predicates (lua-match?, match?, eq?, not-eq?, etc.)
    let general_predicates = query.general_predicates(match_.pattern_index);

    for predicate in general_predicates {
        // Check if this predicate applies to our capture
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
                        // Parse the lua pattern and convert to regex
                        match lua_pattern::parse(pattern_str) {
                            Ok(parsed_pattern) => {
                                // Convert to regex for matching
                                match lua_pattern::try_to_regex(&parsed_pattern, false, false) {
                                    Ok(regex_str) => {
                                        // Use regex to perform the match
                                        if let Ok(re) = regex::Regex::new(&regex_str)
                                            && !re.is_match(node_text)
                                        {
                                            return false;
                                        } else if regex::Regex::new(&regex_str).is_err() {
                                            eprintln!(
                                                "Failed to compile regex from lua-pattern: {}",
                                                regex_str
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "Failed to convert lua-pattern to regex: {} - {:?}",
                                            pattern_str, e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Invalid lua-pattern: {} - {:?}", pattern_str, e);
                            }
                        }
                    }
                }
                "match?" => {
                    if let Some(tree_sitter::QueryPredicateArg::String(pattern_str)) =
                        predicate.args.get(1)
                    {
                        // Use regex for match? predicate
                        if let Ok(re) = regex::Regex::new(pattern_str)
                            && !re.is_match(node_text)
                        {
                            return false;
                        }
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
                                // Compare with another capture
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
                                // Compare with another capture
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
                _ => {
                    // Unknown predicate, ignore
                }
            }
        }
    }

    true
}

/// Filter captures based on query predicates
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
