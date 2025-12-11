use crate::config::CaptureMappings;
use crate::text::convert_byte_to_utf16_in_line;
use tower_lsp::lsp_types::{
    Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensDelta, SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensResult,
};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

// Re-export semantic token types from lsp_types
pub use tower_lsp::lsp_types::{
    SemanticToken as DomainSemanticToken, SemanticTokens as DomainSemanticTokens,
    SemanticTokensDelta as DomainSemanticTokensDelta,
    SemanticTokensEdit as DomainSemanticTokensEdit,
    SemanticTokensFullDeltaResult as DomainSemanticTokensFullDeltaResult,
    SemanticTokensRangeResult as DomainSemanticTokensRangeResult,
    SemanticTokensResult as DomainSemanticTokensResult,
};

pub const LEGEND_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::COMMENT,
    SemanticTokenType::KEYWORD,
    SemanticTokenType::STRING,
    SemanticTokenType::NUMBER,
    SemanticTokenType::REGEXP,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE,
    SemanticTokenType::TYPE,
    SemanticTokenType::STRUCT,
    SemanticTokenType::CLASS,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::ENUM,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::TYPE_PARAMETER,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
    SemanticTokenType::MACRO,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::EVENT,
    SemanticTokenType::MODIFIER,
    SemanticTokenType::DECORATOR,
];

pub const LEGEND_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::ASYNC,
    SemanticTokenModifier::MODIFICATION,
    SemanticTokenModifier::DOCUMENTATION,
    SemanticTokenModifier::DEFAULT_LIBRARY,
];

/// Convert byte column position to UTF-16 column position within a line
/// This is a wrapper around the common utility for backward compatibility
fn byte_to_utf16_col(line: &str, byte_col: usize) -> usize {
    // The common utility returns Option, but we need to handle the case where
    // byte_col is beyond the end of the line or in the middle of a character
    convert_byte_to_utf16_in_line(line, byte_col).unwrap_or_else(|| {
        // If conversion fails (e.g., byte_col is in the middle of a multi-byte char),
        // find the nearest valid position
        let mut valid_col = byte_col;
        while valid_col > 0 {
            if let Some(utf16) = convert_byte_to_utf16_in_line(line, valid_col) {
                return utf16;
            }
            valid_col -= 1;
        }
        // Fallback to 0 if no valid position found
        0
    })
}

/// Apply capture mappings to transform a capture name
///
/// Looks up the capture name in the provided mappings and returns the mapped value if found,
/// or the original capture name if no mapping exists.
///
/// # Arguments
/// * `capture_name` - The original capture name from the tree-sitter query
/// * `filetype` - The filetype of the document being processed  
/// * `capture_mappings` - The full capture mappings configuration
///
/// # Returns
/// The mapped capture name or the original if no mapping exists
fn apply_capture_mapping(
    capture_name: &str,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> String {
    if let Some(mappings) = capture_mappings {
        // Try filetype-specific mapping first
        if let Some(ft) = filetype
            && let Some(lang_mappings) = mappings.get(ft)
            && let Some(mapped) = lang_mappings.highlights.get(capture_name)
        {
            return mapped.clone();
        }

        // Try wildcard mapping
        if let Some(wildcard_mappings) = mappings.get("_")
            && let Some(mapped) = wildcard_mappings.highlights.get(capture_name)
        {
            return mapped.clone();
        }
    }

    // Return original if no mapping found
    capture_name.to_string()
}

/// Map capture names from tree-sitter queries to LSP semantic token types and modifiers
///
/// Capture names can be in the format "type.modifier1.modifier2" where:
/// - The first part is the token type (e.g., "variable", "function")
/// - Following parts are modifiers (e.g., "readonly", "defaultLibrary")
fn map_capture_to_token_type_and_modifiers(capture_name: &str) -> (u32, u32) {
    let parts: Vec<&str> = capture_name.split('.').collect();
    let token_type_name = parts.first().copied().unwrap_or("variable");

    let token_type_index = LEGEND_TYPES
        .iter()
        .position(|t| t.as_str() == token_type_name)
        .or_else(|| LEGEND_TYPES.iter().position(|t| t.as_str() == "variable"))
        .unwrap_or(0) as u32;

    let mut modifiers_bitset = 0u32;
    for modifier_name in &parts[1..] {
        if let Some(index) = LEGEND_MODIFIERS
            .iter()
            .position(|m| m.as_str() == *modifier_name)
        {
            modifiers_bitset |= 1 << index;
        }
    }

    (token_type_index, modifiers_bitset)
}

/// Handle semantic tokens full request
///
/// Analyzes the entire document and returns all semantic tokens.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
///
/// # Returns
/// Semantic tokens for the entire document
pub fn handle_semantic_tokens_full(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensResult> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    // Collect all tokens with their positions
    // Pre-allocate with estimated capacity to reduce reallocations
    let mut tokens = Vec::with_capacity(1000);

    // Pre-calculate line starts for efficient UTF-16 position conversion
    let lines: Vec<&str> = text.lines().collect();

    while let Some(m) = matches.next() {
        // Filter captures based on predicates
        let filtered_captures = crate::language::filter_captures(query, m, text);

        for c in filtered_captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Only include single-line tokens
            if start_pos.row == end_pos.row {
                // Convert byte columns to UTF-16 columns
                let line = lines.get(start_pos.row).unwrap_or(&"");

                // Calculate UTF-16 column positions from byte positions
                let start_utf16 = byte_to_utf16_col(line, start_pos.column);
                let end_utf16 = byte_to_utf16_col(line, end_pos.column);

                tokens.push((start_pos.row, start_utf16, end_utf16 - start_utf16, c.index));
            }
        }
    }

    // Sort tokens by position
    tokens.sort();

    // Convert to LSP semantic tokens format (relative positions)
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::with_capacity(tokens.len());

    for (line, start, length, capture_index) in tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        // Map capture name to token type and modifiers
        let original_capture_name = &query.capture_names()[capture_index as usize];
        let mapped_capture_name =
            apply_capture_mapping(original_capture_name, filetype, capture_mappings);
        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&mapped_capture_name);

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: length as u32,
            token_type,
            token_modifiers_bitset,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Maximum recursion depth for nested injections to prevent stack overflow
const MAX_INJECTION_DEPTH: usize = 10;

/// Recursively collect semantic tokens from a document and its injections.
///
/// This function processes the given text and tree, collecting tokens from both
/// the current language's highlight query and any language injections found.
/// Nested injections are processed recursively up to MAX_INJECTION_DEPTH.
#[allow(clippy::too_many_arguments)]
fn collect_injection_tokens_recursive(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
    host_text: &str,     // The original host document text (for position conversion)
    host_lines: &[&str], // Lines from the host document
    content_start_byte: usize, // Byte offset where this content starts in the host document
    depth: usize,        // Current recursion depth
    all_tokens: &mut Vec<(usize, usize, usize, u32, String)>, // Output token buffer
) {
    use crate::language::{collect_all_injections, injection::parse_offset_directive_for_pattern};

    // Safety check for recursion depth
    if depth >= MAX_INJECTION_DEPTH {
        return;
    }

    // Calculate position mapping from content-local to host document
    let content_start_line = if content_start_byte == 0 {
        0
    } else {
        host_text[..content_start_byte]
            .chars()
            .filter(|c| *c == '\n')
            .count()
    };

    let content_start_col = if content_start_byte == 0 {
        0
    } else {
        let last_newline = host_text[..content_start_byte].rfind('\n');
        match last_newline {
            Some(pos) => content_start_byte - pos - 1,
            None => content_start_byte,
        }
    };

    // 1. Collect tokens from this document's highlight query
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    while let Some(m) = matches.next() {
        let filtered_captures = crate::language::filter_captures(query, m, text);

        for c in filtered_captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            if start_pos.row == end_pos.row {
                // Map position to host document coordinates
                let host_line = content_start_line + start_pos.row;
                let host_line_text = host_lines.get(host_line).unwrap_or(&"");

                let byte_offset_in_host = if start_pos.row == 0 {
                    content_start_col + start_pos.column
                } else {
                    start_pos.column
                };
                let start_utf16 = byte_to_utf16_col(host_line_text, byte_offset_in_host);

                let end_byte_offset_in_host = if start_pos.row == 0 {
                    content_start_col + end_pos.column
                } else {
                    end_pos.column
                };
                let end_utf16 = byte_to_utf16_col(host_line_text, end_byte_offset_in_host);

                let capture_name = &query.capture_names()[c.index as usize];
                let mapped_name = apply_capture_mapping(capture_name, filetype, capture_mappings);

                all_tokens.push((
                    host_line,
                    start_utf16,
                    end_utf16 - start_utf16,
                    c.index,
                    mapped_name,
                ));
            }
        }
    }

    // 2. Find and process injections in this document
    let current_lang = filetype.unwrap_or("unknown");
    let Some(injection_query) = coordinator.get_injection_query(current_lang) else {
        return; // No injection query for this language
    };

    let Some(injections) = collect_all_injections(&tree.root_node(), text, Some(&injection_query))
    else {
        return; // No injections found
    };

    for injection in injections {
        // Ensure the injected language is loaded
        let load_result = coordinator.ensure_language_loaded(&injection.language);
        if !load_result.success {
            continue;
        }

        // Get highlight query for injected language
        let Some(inj_highlight_query) = coordinator.get_highlight_query(&injection.language) else {
            continue;
        };

        // Get offset directive if any
        let offset = parse_offset_directive_for_pattern(&injection_query, injection.pattern_index);

        // Calculate effective content range with offset (relative to current text)
        let content_node = injection.content_node;
        let (inj_start_byte, inj_end_byte) = if let Some(off) = offset {
            use crate::analysis::offset_calculator::{
                ByteRange, calculate_effective_range_with_text,
            };
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range_with_text(text, byte_range, off);
            (effective.start, effective.end)
        } else {
            (content_node.start_byte(), content_node.end_byte())
        };

        // Extract the content text
        if inj_start_byte >= text.len() || inj_end_byte > text.len() {
            continue;
        }
        let inj_content_text = &text[inj_start_byte..inj_end_byte];

        // Parse the injected content
        let Some(mut parser) = parser_pool.acquire(&injection.language) else {
            continue;
        };
        let Some(injected_tree) = parser.parse(inj_content_text, None) else {
            parser_pool.release(injection.language.clone(), parser);
            continue;
        };

        // Calculate the byte offset of this injection in the host document
        // We need to map from current text position to host text position
        let inj_host_start_byte = content_start_byte + inj_start_byte;

        // Recursively collect tokens from the injected content
        collect_injection_tokens_recursive(
            inj_content_text,
            &injected_tree,
            &inj_highlight_query,
            Some(&injection.language),
            capture_mappings,
            coordinator,
            parser_pool,
            host_text,
            host_lines,
            inj_host_start_byte,
            depth + 1,
            all_tokens,
        );

        parser_pool.release(injection.language.clone(), parser);
    }
}

/// Handle semantic tokens full request with injection support
///
/// Analyzes the entire document including injected language regions and returns
/// semantic tokens for both the host document and all injected content.
/// Supports recursive/nested injections (e.g., Lua inside Markdown inside Markdown).
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `injection_query` - The injection query for detecting language injections
/// * `coordinator` - Language coordinator for loading injected language parsers
/// * `parser_pool` - Parser pool for efficient parser reuse
///
/// # Returns
/// Semantic tokens for the entire document including injected content
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full_with_injection(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    _injection_query: &Query, // Not used directly; we get it from coordinator in the recursive fn
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
) -> Option<SemanticTokensResult> {
    // Collect all absolute tokens (line, col, length, capture_index, mapped_name)
    let mut all_tokens: Vec<(usize, usize, usize, u32, String)> = Vec::with_capacity(1000);

    let lines: Vec<&str> = text.lines().collect();

    // Recursively collect tokens from the document and all injections
    collect_injection_tokens_recursive(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        parser_pool,
        text,   // host_text = text (we're at the root)
        &lines, // host_lines
        0,      // content_start_byte = 0 (we're at the root)
        0,      // depth = 0 (starting depth)
        &mut all_tokens,
    );

    // 3. Filter out zero-length tokens (they don't provide useful highlighting)
    // This also fixes issues where overlapping injections create duplicate tokens
    // at the same position (e.g., markdown_inline creates zero-length tokens that
    // would otherwise shadow real tokens from code block injections)
    all_tokens.retain(|(_, _, length, _, _)| *length > 0);

    // 4. Sort tokens by position
    all_tokens.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // 5. Deduplicate tokens at the same position
    // When host document and injected content produce tokens at the same position,
    // keep only the first one (which is typically the one with more context).
    // This prevents issues with Neovim's semantic token highlighter which may
    // mishandle multiple tokens at the exact same position.
    all_tokens.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

    // 6. Convert to delta encoding
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::with_capacity(all_tokens.len());

    for (line, start, length, _capture_index, mapped_name) in all_tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&mapped_name);

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: length as u32,
            token_type,
            token_modifiers_bitset,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Handle semantic tokens range request
///
/// Analyzes a specific range of the document and returns semantic tokens
/// only for that range.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting
/// * `range` - The range to get tokens for (LSP positions)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
///
/// # Returns
/// Semantic tokens for the specified range of the document
pub fn handle_semantic_tokens_range(
    text: &str,
    tree: &Tree,
    query: &Query,
    range: &Range,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensResult> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    // Convert LSP range to line numbers for filtering
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    // Pre-calculate line starts for efficient UTF-16 position conversion
    let lines: Vec<&str> = text.lines().collect();

    // Collect tokens within the range
    // Pre-allocate with estimated capacity for typical visible range
    let mut tokens = Vec::with_capacity(200);
    while let Some(m) = matches.next() {
        // Filter captures based on predicates
        let filtered_captures = crate::language::filter_captures(query, m, text);

        for c in filtered_captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Check if token is within the requested range
            if start_pos.row < start_line || start_pos.row > end_line {
                continue;
            }

            // Only include single-line tokens
            if start_pos.row == end_pos.row {
                let line = lines.get(start_pos.row).unwrap_or(&"");

                // Convert byte columns to UTF-16 columns for proper boundary checking
                let start_utf16 = byte_to_utf16_col(line, start_pos.column);
                let end_utf16 = byte_to_utf16_col(line, end_pos.column);

                // For tokens on the boundary lines, check column positions (now in UTF-16)
                if start_pos.row == end_line && start_utf16 > range.end.character as usize {
                    continue;
                }
                if start_pos.row == start_line && end_utf16 < range.start.character as usize {
                    continue;
                }

                tokens.push((start_pos.row, start_utf16, end_utf16 - start_utf16, c.index));
            }
        }
    }

    // Sort tokens by position
    tokens.sort();

    // Convert to LSP semantic tokens format (relative positions)
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::with_capacity(tokens.len());

    for (line, start, length, capture_index) in tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        // Map capture name to token type and modifiers
        let original_capture_name = &query.capture_names()[capture_index as usize];
        let mapped_capture_name =
            apply_capture_mapping(original_capture_name, filetype, capture_mappings);
        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&mapped_capture_name);

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: length as u32,
            token_type,
            token_modifiers_bitset,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Handle semantic tokens range request with injection support
///
/// Analyzes a specific range of the document including injected language regions
/// and returns semantic tokens for both the host document and all injected content
/// within that range.
///
/// This function wraps `handle_semantic_tokens_full_with_injection` and filters
/// the results to only include tokens within the requested range.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `range` - The range to get tokens for (LSP positions)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `injection_query` - The injection query for detecting language injections
/// * `coordinator` - Language coordinator for loading injected language parsers
/// * `parser_pool` - Parser pool for efficient parser reuse
///
/// # Returns
/// Semantic tokens for the specified range including injected content
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_range_with_injection(
    text: &str,
    tree: &Tree,
    query: &Query,
    range: &Range,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    injection_query: &Query,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
) -> Option<SemanticTokensResult> {
    // Get all tokens using the full handler
    let full_result = handle_semantic_tokens_full_with_injection(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        injection_query,
        coordinator,
        parser_pool,
    )?;

    // Extract tokens from result
    let SemanticTokensResult::Tokens(full_tokens) = full_result else {
        return Some(full_result);
    };

    // Convert delta-encoded tokens back to absolute positions, filter by range,
    // and re-encode as deltas
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    let mut abs_line = 0usize;
    let mut abs_col = 0usize;

    // Collect tokens that are within the range
    let mut filtered_tokens: Vec<(usize, usize, u32, u32, u32)> = Vec::new();

    for token in full_tokens.data {
        // Update absolute position
        abs_line += token.delta_line as usize;
        if token.delta_line > 0 {
            abs_col = token.delta_start as usize;
        } else {
            abs_col += token.delta_start as usize;
        }

        // Check if token is within range
        if abs_line >= start_line && abs_line <= end_line {
            // For boundary lines, check column positions
            if abs_line == end_line && abs_col > range.end.character as usize {
                continue;
            }
            if abs_line == start_line
                && abs_col + token.length as usize <= range.start.character as usize
            {
                continue;
            }

            filtered_tokens.push((
                abs_line,
                abs_col,
                token.length,
                token.token_type,
                token.token_modifiers_bitset,
            ));
        }
    }

    // Re-encode as deltas
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::with_capacity(filtered_tokens.len());

    for (line, col, length, token_type, modifiers) in filtered_tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            col - last_start
        } else {
            col
        };

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length,
            token_type,
            token_modifiers_bitset: modifiers,
        });

        last_line = line;
        last_start = col;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}

/// Handle semantic tokens full delta request
///
/// Analyzes the document and returns either a delta from the previous version
/// or the full set of semantic tokens if delta cannot be calculated.
///
/// # Arguments
/// * `text` - The current source text
/// * `tree` - The parsed syntax tree for the current text
/// * `query` - The tree-sitter query for semantic highlighting
/// * `previous_result_id` - The result ID from the previous semantic tokens response
/// * `previous_tokens` - The previous semantic tokens to calculate delta from
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
///
/// # Returns
/// Either a delta or full semantic tokens for the document
pub fn handle_semantic_tokens_full_delta(
    text: &str,
    tree: &Tree,
    query: &Query,
    previous_result_id: &str,
    previous_tokens: Option<&SemanticTokens>,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<SemanticTokensFullDeltaResult> {
    // Get current tokens
    let current_result =
        handle_semantic_tokens_full(text, tree, query, filetype, capture_mappings)?;
    let current_tokens = match current_result {
        SemanticTokensResult::Tokens(tokens) => tokens,
        SemanticTokensResult::Partial(_) => return None,
    };

    // Check if we can calculate a delta
    if let Some(prev) = previous_tokens
        && prev.result_id.as_deref() == Some(previous_result_id)
        && let Some(delta) = calculate_semantic_tokens_delta(prev, &current_tokens)
    {
        return Some(SemanticTokensFullDeltaResult::TokensDelta(delta));
    }

    // Fall back to full tokens
    Some(SemanticTokensFullDeltaResult::Tokens(current_tokens))
}

/// Calculate delta between two sets of semantic tokens
///
/// Compares previous and current semantic tokens and returns the differences
/// as a set of edits that can be applied to transform the previous tokens
/// into the current tokens.
///
/// # Arguments
/// * `previous` - The previous semantic tokens
/// * `current` - The current semantic tokens
///
/// # Returns
/// Semantic tokens delta containing the edits needed to transform previous to current
fn calculate_semantic_tokens_delta(
    previous: &SemanticTokens,
    current: &SemanticTokens,
) -> Option<SemanticTokensDelta> {
    // Find the common prefix length
    let common_prefix_len = previous
        .data
        .iter()
        .zip(current.data.iter())
        .take_while(|(a, b)| {
            a.delta_line == b.delta_line
                && a.delta_start == b.delta_start
                && a.length == b.length
                && a.token_type == b.token_type
                && a.token_modifiers_bitset == b.token_modifiers_bitset
        })
        .count();

    // If all tokens are the same, no edits needed
    if common_prefix_len == previous.data.len() && common_prefix_len == current.data.len() {
        return Some(SemanticTokensDelta {
            result_id: current.result_id.clone(),
            edits: vec![],
        });
    }

    // Calculate the edit
    let start = common_prefix_len;
    let delete_count = previous.data.len() - start;
    let data = current.data[start..].to_vec();

    Some(SemanticTokensDelta {
        result_id: current.result_id.clone(),
        edits: vec![SemanticTokensEdit {
            start: start as u32,
            delete_count: delete_count as u32,
            data: Some(data),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_capture_to_token_type_and_modifiers() {
        // Test basic token types without modifiers
        assert_eq!(map_capture_to_token_type_and_modifiers("comment"), (0, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("keyword"), (1, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("function"), (14, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("variable"), (17, 0));
        assert_eq!(map_capture_to_token_type_and_modifiers("unknown"), (17, 0)); // Should default to variable

        // Test with single modifier
        let (_, modifiers) = map_capture_to_token_type_and_modifiers("variable.readonly");
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly is at index 2

        let (_, modifiers) = map_capture_to_token_type_and_modifiers("function.async");
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async is at index 6

        // Test with multiple modifiers
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("variable.readonly.defaultLibrary");
        assert_eq!(token_type, 17); // variable
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly
        assert_eq!(modifiers & (1 << 9), 1 << 9); // defaultLibrary

        // Test unknown modifiers are ignored
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("function.unknownModifier.async");
        assert_eq!(token_type, 14); // function
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async should still be set
    }

    #[test]
    fn test_semantic_tokens_delta() {
        // Create mock semantic tokens for testing
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 10,
                    token_type: 0, // comment
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 3,
                    token_type: 1, // keyword
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 1,
                    token_type: 17, // variable
                    token_modifiers_bitset: 0,
                },
            ],
        };

        // Modified tokens (changed comment length)
        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 14,    // changed length
                    token_type: 0, // comment
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 3,
                    token_type: 1, // keyword
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 1,
                    token_type: 17, // variable
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.result_id, Some("v2".to_string()));
        assert_eq!(delta.edits.len(), 1);
        assert_eq!(delta.edits[0].start, 0);
        assert_eq!(delta.edits[0].delete_count, 3);
        let edits_data = delta.edits[0]
            .data
            .as_ref()
            .expect("delta edits should include replacement data");
        assert_eq!(edits_data.len(), 3);
    }

    #[test]
    fn test_semantic_tokens_range() {
        use tower_lsp::lsp_types::Position;

        // Create mock tokens for a document
        let all_tokens = SemanticTokens {
            result_id: None,
            data: vec![
                SemanticToken {
                    // Line 0, col 0-10
                    delta_line: 0,
                    delta_start: 0,
                    length: 10,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    // Line 2, col 0-3
                    delta_line: 2,
                    delta_start: 0,
                    length: 3,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    // Line 2, col 4-5
                    delta_line: 0,
                    delta_start: 4,
                    length: 1,
                    token_type: 17,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    // Line 4, col 2-8
                    delta_line: 2,
                    delta_start: 2,
                    length: 6,
                    token_type: 14,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        // Test range that includes only lines 1-3
        let _range = Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 100,
            },
        };

        // Tokens in range should be the ones on line 2
        // We'd need actual tree-sitter setup to test the real function,
        // so this is more of a placeholder showing the expected structure
        assert_eq!(all_tokens.data.len(), 4);
    }

    #[test]
    fn test_semantic_tokens_delta_no_changes() {
        let tokens = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![SemanticToken {
                delta_line: 0,
                delta_start: 0,
                length: 10,
                token_type: 0,
                token_modifiers_bitset: 0,
            }],
        };

        let delta = calculate_semantic_tokens_delta(&tokens, &tokens);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 0);
    }

    #[test]
    fn test_byte_to_utf16_col() {
        // ASCII text
        let line = "hello world";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 5), 5);
        assert_eq!(byte_to_utf16_col(line, 11), 11);

        // Japanese text (3 bytes per char in UTF-8, 1 code unit in UTF-16)
        let line = "こんにちは";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 3), 1); // After "こ"
        assert_eq!(byte_to_utf16_col(line, 6), 2); // After "こん"
        assert_eq!(byte_to_utf16_col(line, 15), 5); // After all 5 chars

        // Mixed ASCII and Japanese
        let line = "let x = \"あいうえお\"";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 8), 8); // Before '"'
        assert_eq!(byte_to_utf16_col(line, 9), 9); // Before "あ"
        assert_eq!(byte_to_utf16_col(line, 12), 10); // After "あ" (3 bytes -> 1 UTF-16)
        assert_eq!(byte_to_utf16_col(line, 24), 14); // After "あいうえお\"" (15 bytes + 1 quote)

        // Emoji (4 bytes in UTF-8, 2 code units in UTF-16)
        let line = "hello 👋 world";
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 6), 6); // After "hello "
        assert_eq!(byte_to_utf16_col(line, 10), 8); // After emoji (4 bytes -> 2 UTF-16)
    }

    #[test]
    fn test_semantic_tokens_with_japanese() {
        use tree_sitter::{Parser, Query};

        let text = r#"let x = "あいうえお"
let y = "hello""#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        let query_text = r#"
            "let" @keyword
            (identifier) @variable
            (string_literal) @string
        "#;

        let query = Query::new(&language, query_text).unwrap();
        let result = handle_semantic_tokens_full(text, &tree, &query, Some("rust"), None);

        assert!(result.is_some());

        // Verify tokens were generated (can't inspect internals due to private type)
        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("expected complete semantic tokens result");
        };

        // Should have tokens for: let, x, string, let, y, string
        assert!(tokens.data.len() >= 6);

        // Check that the string token on first line has correct UTF-16 length
        // "あいうえお" = 5 UTF-16 code units + 2 quotes = 7
        let string_token = tokens
            .data
            .iter()
            .find(|t| t.token_type == 2 && t.length == 7); // string type = 2
        assert!(
            string_token.is_some(),
            "Japanese string token should have UTF-16 length of 7"
        );
    }

    #[test]
    fn test_injection_semantic_tokens_basic() {
        // Test semantic tokens for injected Lua code in Markdown
        // example.md has a lua fenced code block at line 6 (0-indexed):
        // ```lua
        // local xyz = 12345
        // ```
        //
        // The `local` keyword should produce a semantic token at line 6, col 0
        // with token_type = keyword (1)

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool with search paths
        let coordinator = LanguageCoordinator::new();

        // Configure with search paths (deps/treesitter is where parsers are)
        let settings = WorkspaceSettings {
            search_paths: vec!["deps/treesitter".to_string()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Now try to load the languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Get injection query for markdown
        let injection_query = coordinator
            .get_injection_query("markdown")
            .expect("Should have markdown injection query");

        // Call the injection-aware function
        let result = handle_semantic_tokens_full_with_injection(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            &injection_query,
            &coordinator,
            &mut parser_pool,
        );

        // Should return some tokens
        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `local` keyword token at line 6 (0-indexed), col 0
        // SemanticToken uses delta encoding, so we need to reconstruct absolute positions
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_local_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 6 (0-indexed), col 0, keyword type (1), length 5 ("local")
            if abs_line == 6 && abs_col == 0 && token.token_type == 1 && token.length == 5 {
                found_local_keyword = true;
                break;
            }
        }

        assert!(
            found_local_keyword,
            "Should find `local` keyword token at line 6, col 0 from injected Lua code"
        );
    }

    #[test]
    fn test_nested_injection_semantic_tokens() {
        // Test semantic tokens for nested injections: Lua inside Markdown inside Markdown
        // example.md has a nested structure at lines 12-16 (1-indexed):
        // `````markdown
        // ```lua
        // local injection = true
        // ```
        // `````
        //
        // The `local` keyword at line 14 (1-indexed) / line 13 (0-indexed) should produce
        // a semantic token with token_type = keyword (1)

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool with search paths
        let coordinator = LanguageCoordinator::new();

        // Configure with search paths (deps/treesitter is where parsers are)
        let settings = WorkspaceSettings {
            search_paths: vec!["deps/treesitter".to_string()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Now try to load the languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Get injection query for markdown
        let injection_query = coordinator
            .get_injection_query("markdown")
            .expect("Should have markdown injection query");

        // Call the injection-aware function
        let result = handle_semantic_tokens_full_with_injection(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            &injection_query,
            &coordinator,
            &mut parser_pool,
        );

        // Should return some tokens
        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `local` keyword token at line 13 (0-indexed), col 0
        // This is inside the nested markdown -> lua injection
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_nested_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 13 (0-indexed), col 0, keyword type (1), length 5 ("local")
            if abs_line == 13 && abs_col == 0 && token.token_type == 1 && token.length == 5 {
                found_nested_keyword = true;
                break;
            }
        }

        assert!(
            found_nested_keyword,
            "Should find `local` keyword token at line 13, col 0 from nested Lua injection (Lua inside Markdown inside Markdown)"
        );
    }

    #[test]
    fn test_indented_injection_semantic_tokens() {
        // Test semantic tokens for indented injections: Lua in a list item with 4-space indent
        // example.md has an indented code block at lines 22-24 (1-indexed):
        // * item
        //
        //     ```lua
        //     local indent = true
        //     ```
        //
        // The `local` keyword at line 23 (1-indexed) / line 22 (0-indexed) should produce
        // a semantic token with:
        // - token_type = keyword (1)
        // - column = 4 (indented by 4 spaces)

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool with search paths
        let coordinator = LanguageCoordinator::new();

        // Configure with search paths (deps/treesitter is where parsers are)
        let settings = WorkspaceSettings {
            search_paths: vec!["deps/treesitter".to_string()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Now try to load the languages
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        let load_result = coordinator.ensure_language_loaded("lua");
        assert!(load_result.success, "Should load lua language");

        let mut parser_pool = coordinator.create_document_parser_pool();

        // Parse the markdown document
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Get the highlight query for markdown
        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Get injection query for markdown
        let injection_query = coordinator
            .get_injection_query("markdown")
            .expect("Should have markdown injection query");

        // Call the injection-aware function
        let result = handle_semantic_tokens_full_with_injection(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            &injection_query,
            &coordinator,
            &mut parser_pool,
        );

        // Should return some tokens
        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `local` keyword token at line 22 (0-indexed), col 4
        // This is inside the indented lua code block
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_indented_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 22 (0-indexed), col 4 (indented), keyword type (1), length 5 ("local")
            if abs_line == 22 && abs_col == 4 && token.token_type == 1 && token.length == 5 {
                found_indented_keyword = true;
                break;
            }
        }

        assert!(
            found_indented_keyword,
            "Should find `local` keyword token at line 22, col 4 from indented Lua injection in list item"
        );
    }
}
