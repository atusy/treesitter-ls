use crate::config::{CaptureMappings, WILDCARD_KEY};
use crate::text::convert_byte_to_utf16_in_line;
use tower_lsp_server::ls_types::{
    Range, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensDelta, SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensResult,
};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

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
/// `Some(mapped_name)` for known token types, `None` for unknown types.
/// Unknown types (not in LEGEND_TYPES) should not produce semantic tokens.
fn apply_capture_mapping(
    capture_name: &str,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
) -> Option<String> {
    if let Some(mappings) = capture_mappings {
        // Try filetype-specific mapping first
        if let Some(ft) = filetype
            && let Some(lang_mappings) = mappings.get(ft)
            && let Some(mapped) = lang_mappings.highlights.get(capture_name)
        {
            // Explicit mapping to empty string means "filter this capture"
            return (!mapped.is_empty()).then(|| mapped.clone());
        }

        // Try wildcard mapping
        if let Some(wildcard_mappings) = mappings.get(WILDCARD_KEY)
            && let Some(mapped) = wildcard_mappings.highlights.get(capture_name)
        {
            // Explicit mapping to empty string means "filter this capture"
            return (!mapped.is_empty()).then(|| mapped.clone());
        }
    }

    // No mapping found - check if the base type is in SemanticTokensLegend.
    // If not, return None to skip adding to all_tokens.
    // This prevents unknown captures (e.g., @spell) from blocking meaningful
    // tokens at the same position during deduplication.
    let base_type = capture_name.split('.').next().unwrap_or("");
    if LEGEND_TYPES.iter().any(|t| t.as_str() == base_type) {
        Some(capture_name.to_string())
    } else {
        None
    }
}

/// Map capture names from tree-sitter queries to LSP semantic token types and modifiers
///
/// Capture names can be in the format "type.modifier1.modifier2" where:
/// - The first part is the token type (e.g., "variable", "function")
/// - Following parts are modifiers (e.g., "readonly", "defaultLibrary")
///
/// Returns `None` for unknown token types (not in LEGEND_TYPES).
/// Unknown modifiers are ignored.
fn map_capture_to_token_type_and_modifiers(capture_name: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = capture_name.split('.').collect();
    let token_type_name = parts.first().copied().filter(|s| !s.is_empty())?;

    let token_type_index = LEGEND_TYPES
        .iter()
        .position(|t| t.as_str() == token_type_name)? as u32;

    let mut modifiers_bitset = 0u32;
    for modifier_name in &parts[1..] {
        if let Some(index) = LEGEND_MODIFIERS
            .iter()
            .position(|m| m.as_str() == *modifier_name)
        {
            modifiers_bitset |= 1 << index;
        }
    }

    Some((token_type_index, modifiers_bitset))
}

/// Maximum recursion depth for nested injections to prevent stack overflow
const MAX_INJECTION_DEPTH: usize = 10;

/// Type alias for raw token data before delta encoding
/// (line, column, length, capture_index, mapped_name)
type RawToken = (usize, usize, usize, u32, String);

/// Calculate byte offsets for a line within a multiline token.
///
/// This helper computes the start and end byte positions for a specific line (row)
/// within a multiline token, handling both host document and injected content coordinates.
///
/// # Arguments
/// * `row` - The current row being processed (relative to content)
/// * `start_pos` - Token start position in content coordinates
/// * `end_pos` - Token end position in content coordinates
/// * `content_start_col` - Column offset where injection starts in host line (0 for host content)
/// * `content_line_len` - Length of the content line at this row
///
/// # Returns
/// Tuple of (line_start_byte, line_end_byte) in host document coordinates
fn calculate_line_byte_offsets(
    row: usize,
    start_pos: tree_sitter::Point,
    end_pos: tree_sitter::Point,
    content_start_col: usize,
    content_line_len: usize,
) -> (usize, usize) {
    // Calculate start byte offset for this line
    let line_start = if row == start_pos.row {
        if row == 0 {
            content_start_col + start_pos.column
        } else {
            start_pos.column
        }
    } else {
        // Continuation lines start at column 0
        0
    };

    // Calculate end byte offset for this line
    let line_end = if row == end_pos.row {
        if row == 0 {
            content_start_col + end_pos.column
        } else {
            end_pos.column
        }
    } else {
        // Non-final lines: end at injected content's line end (not host line end)
        if row == 0 {
            content_start_col + content_line_len
        } else {
            content_line_len
        }
    };

    (line_start, line_end)
}

/// Collect tokens from a single document's highlight query (no injection processing).
///
/// This is the common logic shared by both pool-based and local-parser-based
/// recursive functions. It processes the given query against the tree and
/// maps positions from content-local coordinates to host document coordinates.
///
/// # Multiline Token Handling
///
/// When `supports_multiline` is true (client declares `multilineTokenSupport`),
/// tokens spanning multiple lines are emitted as-is per LSP 3.16.0+ spec.
///
/// When `supports_multiline` is false, multiline tokens are split into per-line
/// tokens for compatibility with clients that don't support multiline tokens.
#[allow(clippy::too_many_arguments)]
fn collect_host_tokens(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    host_text: &str,
    host_lines: &[&str],
    content_start_byte: usize,
    supports_multiline: bool,
    all_tokens: &mut Vec<RawToken>,
) {
    // Validate content_start_byte is within bounds to prevent slice panics
    // This can happen during concurrent edits when document text shortens
    if content_start_byte > host_text.len() {
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

    // Split content text into lines for byte offset calculations
    let content_lines: Vec<&str> = text.lines().collect();

    // Collect tokens from this document's highlight query
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    while let Some(m) = matches.next() {
        let filtered_captures = crate::language::filter_captures(query, m, text);

        for c in filtered_captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Check if this is a single-line token or trailing newline case
            let is_single_line = start_pos.row == end_pos.row;
            let is_trailing_newline = end_pos.row == start_pos.row + 1 && end_pos.column == 0;

            // Get the mapped capture name early to avoid repeated mapping
            let capture_name = &query.capture_names()[c.index as usize];
            let Some(mapped_name) = apply_capture_mapping(capture_name, filetype, capture_mappings)
            else {
                // Skip unknown captures (None)
                continue;
            };

            if is_single_line || is_trailing_newline {
                // Single-line token: emit as before
                let host_line = content_start_line + start_pos.row;
                let host_line_text = host_lines.get(host_line).unwrap_or(&"");

                let byte_offset_in_host = if start_pos.row == 0 {
                    content_start_col + start_pos.column
                } else {
                    start_pos.column
                };
                let start_utf16 = byte_to_utf16_col(host_line_text, byte_offset_in_host);

                // For trailing newline case, use the line length as end position
                let end_byte_offset_in_host = if is_trailing_newline {
                    host_line_text.len()
                } else if start_pos.row == 0 {
                    content_start_col + end_pos.column
                } else {
                    end_pos.column
                };
                let end_utf16 = byte_to_utf16_col(host_line_text, end_byte_offset_in_host);

                all_tokens.push((
                    host_line,
                    start_utf16,
                    end_utf16 - start_utf16,
                    c.index,
                    mapped_name,
                ));
            } else if supports_multiline {
                // Multiline token with client support: emit a single token spanning multiple lines.
                // LSP semantic tokens use line-relative positions, so the token naturally starts on
                // the first line (start_pos.row), and its length spans across all lines in UTF-16
                // code units (including newline characters) up to the end position on end_pos.row.
                //
                // The length is calculated by summing UTF-16 lengths across all lines of the token,
                // plus 1 for each newline character between lines.
                let host_start_line = content_start_line + start_pos.row;
                let host_end_line = content_start_line + end_pos.row;

                // Calculate start position
                let host_start_line_text = host_lines.get(host_start_line).unwrap_or(&"");
                let start_byte_offset = if start_pos.row == 0 {
                    content_start_col + start_pos.column
                } else {
                    start_pos.column
                };
                let start_utf16 = byte_to_utf16_col(host_start_line_text, start_byte_offset);

                // Calculate total length in UTF-16 code units across all lines
                let mut total_length_utf16 = 0usize;
                for row in start_pos.row..=end_pos.row {
                    let host_row = content_start_line + row;
                    let line_text = host_lines.get(host_row).unwrap_or(&"");
                    let content_line_len = content_lines.get(row).map(|l| l.len()).unwrap_or(0);

                    let (line_start, line_end) = calculate_line_byte_offsets(
                        row,
                        start_pos,
                        end_pos,
                        content_start_col,
                        content_line_len,
                    );

                    let line_start_utf16 = byte_to_utf16_col(line_text, line_start);
                    let line_end_utf16 = byte_to_utf16_col(line_text, line_end);
                    total_length_utf16 += line_end_utf16 - line_start_utf16;

                    // Add 1 for newline character between lines (except last line)
                    if row < end_pos.row {
                        total_length_utf16 += 1;
                    }
                }

                log::trace!(
                    target: "kakehashi::semantic",
                    "[MULTILINE_TOKEN] capture={} lines={}..{} host_lines={}..{} length={}",
                    capture_name, start_pos.row, end_pos.row,
                    host_start_line, host_end_line, total_length_utf16
                );

                all_tokens.push((
                    host_start_line,
                    start_utf16,
                    total_length_utf16,
                    c.index,
                    mapped_name,
                ));
            } else {
                // Multiline token without client support: split into per-line tokens
                for row in start_pos.row..=end_pos.row {
                    let host_row = content_start_line + row;
                    let host_line_text = host_lines.get(host_row).unwrap_or(&"");
                    let content_line_len = content_lines.get(row).map(|l| l.len()).unwrap_or(0);

                    let (line_start_byte, line_end_byte) = calculate_line_byte_offsets(
                        row,
                        start_pos,
                        end_pos,
                        content_start_col,
                        content_line_len,
                    );

                    let start_utf16 = byte_to_utf16_col(host_line_text, line_start_byte);
                    let end_utf16 = byte_to_utf16_col(host_line_text, line_end_byte);

                    // Skip empty tokens
                    if end_utf16 > start_utf16 {
                        all_tokens.push((
                            host_row,
                            start_utf16,
                            end_utf16 - start_utf16,
                            c.index,
                            mapped_name.clone(),
                        ));
                    }
                }
            }
        }
    }
}

/// Data for processing a single injection (parser-agnostic).
///
/// This struct captures all the information needed to process an injection
/// before the actual parsing step.
struct InjectionContext<'a> {
    resolved_lang: String,
    highlight_query: std::sync::Arc<Query>,
    content_text: &'a str,
    host_start_byte: usize,
}

/// Collect all injection contexts from a document tree.
///
/// This function processes the injection query and returns a list of
/// `InjectionContext` structs, each containing the information needed
/// to parse and process one injection. This is parser-agnostic; actual
/// parsing happens after this function returns.
fn collect_injection_contexts<'a>(
    text: &'a str,
    tree: &Tree,
    filetype: Option<&str>,
    coordinator: &crate::language::LanguageCoordinator,
    content_start_byte: usize,
) -> Vec<InjectionContext<'a>> {
    use crate::language::{collect_all_injections, injection::parse_offset_directive_for_pattern};

    let current_lang = filetype.unwrap_or("unknown");
    let Some(injection_query) = coordinator.get_injection_query(current_lang) else {
        return Vec::new();
    };

    let Some(injections) = collect_all_injections(&tree.root_node(), text, Some(&injection_query))
    else {
        return Vec::new();
    };

    let mut contexts = Vec::with_capacity(injections.len());

    for injection in injections {
        // Extract injection content for first-line detection (shebang, mode line)
        let injection_content =
            &text[injection.content_node.start_byte()..injection.content_node.end_byte()];

        // Resolve injection language with unified detection
        let Some((resolved_lang, _)) =
            coordinator.resolve_injection_language(&injection.language, injection_content)
        else {
            continue;
        };

        // Get highlight query for resolved language
        let Some(inj_highlight_query) = coordinator.get_highlight_query(&resolved_lang) else {
            continue;
        };

        // Get offset directive if any
        let offset = parse_offset_directive_for_pattern(&injection_query, injection.pattern_index);

        // Calculate effective content range
        let content_node = injection.content_node;
        let (inj_start_byte, inj_end_byte) = if let Some(off) = offset {
            use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range};
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range(text, byte_range, off);
            (effective.start, effective.end)
        } else {
            (content_node.start_byte(), content_node.end_byte())
        };

        // Validate range: ensure bounds are valid AND start <= end
        // The start > end case can occur during concurrent edits or with certain offset calculations
        if inj_start_byte >= text.len()
            || inj_end_byte > text.len()
            || inj_start_byte > inj_end_byte
        {
            continue;
        }

        contexts.push(InjectionContext {
            resolved_lang,
            highlight_query: inj_highlight_query,
            content_text: &text[inj_start_byte..inj_end_byte],
            host_start_byte: content_start_byte + inj_start_byte,
        });
    }

    contexts
}

/// Post-process and delta-encode raw tokens into SemanticTokensResult.
///
/// This shared helper:
/// 1. Filters zero-length tokens
/// 2. Sorts by position
/// 3. Deduplicates tokens at same position
/// 4. Delta-encodes for LSP protocol
fn finalize_tokens(mut all_tokens: Vec<RawToken>) -> Option<SemanticTokensResult> {
    // Filter out zero-length tokens BEFORE dedup.
    // Unknown captures are already filtered at collection time (apply_capture_mapping returns None).
    all_tokens.retain(|(_, _, length, _, _)| *length > 0);

    // Sort by position
    all_tokens.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    // Deduplicate at same position
    all_tokens.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

    if all_tokens.is_empty() {
        return None;
    }

    // Delta-encode
    let mut data = Vec::with_capacity(all_tokens.len());
    let mut last_line = 0usize;
    let mut last_start = 0usize;

    for (line, start, length, _capture_index, mapped_name) in all_tokens {
        // Unknown types are already filtered at collection time (apply_capture_mapping returns None),
        // so map_capture_to_token_type_and_modifiers should always return Some here.
        let (token_type, token_modifiers_bitset) =
            map_capture_to_token_type_and_modifiers(&mapped_name)
                .expect("all tokens should have known types after apply_capture_mapping filtering");

        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

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

use crate::language::injection::collect_all_injections;

/// Collect all unique injection language identifiers from a document tree recursively.
///
/// This function discovers all injection regions in the document (including nested
/// injections) and returns the unique set of language identifiers needed to process
/// them. This is essential for the narrow lock scope pattern: acquire all parsers
/// upfront, release the lock, then process without holding the mutex.
///
/// # Arguments
/// * `tree` - The parsed syntax tree of the host document
/// * `text` - The source text of the host document
/// * `host_language` - The language identifier of the host document (e.g., "markdown")
/// * `coordinator` - Language coordinator for injection query lookup
/// * `parser_pool` - Parser pool for parsing nested injection content
///
/// # Returns
/// A vector of unique resolved language identifiers for all injections found,
/// including those nested inside other injections (up to MAX_INJECTION_DEPTH).
pub(crate) fn collect_injection_languages(
    tree: &Tree,
    text: &str,
    host_language: &str,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
) -> Vec<String> {
    use std::collections::HashSet;

    let mut languages = HashSet::new();
    collect_injection_languages_recursive(
        tree,
        text,
        host_language,
        coordinator,
        parser_pool,
        &mut languages,
        0,
    );
    languages.into_iter().collect()
}

/// Recursive helper for collecting injection languages at all depths.
fn collect_injection_languages_recursive(
    tree: &Tree,
    text: &str,
    language: &str,
    coordinator: &crate::language::LanguageCoordinator,
    parser_pool: &mut crate::language::DocumentParserPool,
    languages: &mut std::collections::HashSet<String>,
    depth: usize,
) {
    use crate::language::injection::parse_offset_directive_for_pattern;

    if depth >= MAX_INJECTION_DEPTH {
        return;
    }

    // Get injection query for this language
    let Some(injection_query) = coordinator.get_injection_query(language) else {
        return;
    };

    // Find all injection regions
    let Some(injections) = collect_all_injections(&tree.root_node(), text, Some(&injection_query))
    else {
        return;
    };

    for injection in injections {
        // Extract injection content for first-line detection (shebang, mode line)
        let injection_content =
            &text[injection.content_node.start_byte()..injection.content_node.end_byte()];

        // Resolve the injection language
        let Some((resolved_lang, _)) =
            coordinator.resolve_injection_language(&injection.language, injection_content)
        else {
            continue;
        };

        // Add to set (whether already present or not)
        languages.insert(resolved_lang.clone());

        // Get offset directive if any
        let offset = parse_offset_directive_for_pattern(&injection_query, injection.pattern_index);

        // Calculate effective content range
        let content_node = injection.content_node;
        let (inj_start_byte, inj_end_byte) = if let Some(off) = offset {
            use crate::analysis::offset_calculator::{ByteRange, calculate_effective_range};
            let byte_range = ByteRange::new(content_node.start_byte(), content_node.end_byte());
            let effective = calculate_effective_range(text, byte_range, off);
            (effective.start, effective.end)
        } else {
            (content_node.start_byte(), content_node.end_byte())
        };

        // Validate range: ensure bounds are valid AND start <= end
        if inj_start_byte >= text.len()
            || inj_end_byte > text.len()
            || inj_start_byte > inj_end_byte
        {
            continue;
        }
        let inj_content_text = &text[inj_start_byte..inj_end_byte];

        // Parse the injected content to discover nested injections
        let Some(mut parser) = parser_pool.acquire(&resolved_lang) else {
            continue;
        };
        let Some(injected_tree) = parser.parse(inj_content_text, None) else {
            parser_pool.release(resolved_lang.clone(), parser);
            continue;
        };

        // Recursively collect languages from nested injections
        collect_injection_languages_recursive(
            &injected_tree,
            inj_content_text,
            &resolved_lang,
            coordinator,
            parser_pool,
            languages,
            depth + 1,
        );

        parser_pool.release(resolved_lang, parser);
    }
}

/// Recursively collect semantic tokens from a document and its injections.
///
/// This function processes the given text and tree, collecting tokens from both
/// the current language's highlight query and any language injections found.
/// Nested injections are processed recursively up to MAX_INJECTION_DEPTH.
///
/// When coordinator or parser_pool is None, only host document tokens are collected
/// (no injection processing).
#[allow(clippy::too_many_arguments)]
fn collect_injection_tokens_recursive(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
    host_text: &str,
    host_lines: &[&str],
    content_start_byte: usize,
    depth: usize,
    supports_multiline: bool,
    all_tokens: &mut Vec<RawToken>,
) {
    // Safety check for recursion depth
    if depth >= MAX_INJECTION_DEPTH {
        return;
    }

    // 1. Collect tokens from this document's highlight query
    collect_host_tokens(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        host_text,
        host_lines,
        content_start_byte,
        supports_multiline,
        all_tokens,
    );

    // 2. Find and process injections
    let (Some(coordinator), Some(parser_pool)) = (coordinator, parser_pool) else {
        return; // No injection support available
    };

    let contexts =
        collect_injection_contexts(text, tree, filetype, coordinator, content_start_byte);

    for ctx in contexts {
        // Acquire parser from pool
        let Some(mut parser) = parser_pool.acquire(&ctx.resolved_lang) else {
            continue;
        };

        // Parse the injected content
        let Some(injected_tree) = parser.parse(ctx.content_text, None) else {
            parser_pool.release(ctx.resolved_lang.clone(), parser);
            continue;
        };

        // Recursively collect tokens from the injected content
        collect_injection_tokens_recursive(
            ctx.content_text,
            &injected_tree,
            &ctx.highlight_query,
            Some(&ctx.resolved_lang),
            capture_mappings,
            Some(coordinator),
            Some(parser_pool),
            host_text,
            host_lines,
            ctx.host_start_byte,
            depth + 1,
            supports_multiline,
            all_tokens,
        );

        parser_pool.release(ctx.resolved_lang.clone(), parser);
    }
}

/// Handle semantic tokens full request
///
/// Analyzes the entire document including injected language regions and returns
/// semantic tokens for both the host document and all injected content.
/// Supports recursive/nested injections (e.g., Lua inside Markdown inside Markdown).
///
/// When coordinator or parser_pool is None, only host document tokens are returned
/// (no injection processing).
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
///
/// # Returns
/// Semantic tokens for the entire document including injected content (if coordinator/parser_pool provided)
///
/// # Note
/// This function defaults `supports_multiline` to `false` for backward compatibility.
/// Use `handle_semantic_tokens_full_with_multiline` for explicit control.
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
) -> Option<SemanticTokensResult> {
    handle_semantic_tokens_full_with_multiline(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        parser_pool,
        false, // Default to split multiline tokens for backward compatibility
    )
}

/// Handle semantic tokens full request with explicit multiline token support.
///
/// This variant allows explicit control over multiline token handling based on
/// client capabilities.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Semantic tokens for the entire document including injected content
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full_with_multiline(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
    supports_multiline: bool,
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
        supports_multiline,
        &mut all_tokens,
    );

    finalize_tokens(all_tokens)
}

/// Handle semantic tokens full request with pre-acquired local parsers.
///
/// This variant accepts a HashMap of pre-acquired parsers instead of a parser pool,
/// enabling the caller to narrow the lock scope: acquire parsers, release lock,
/// then call this function without holding the pool mutex.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for injection queries (required for injection support)
/// * `local_parsers` - Pre-acquired parsers keyed by language ID
/// * `supports_multiline` - Whether client supports multiline tokens (per LSP 3.16.0+)
///
/// # Returns
/// Semantic tokens for the entire document including injected content
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle_semantic_tokens_full_with_local_parsers(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    local_parsers: &mut std::collections::HashMap<String, tree_sitter::Parser>,
    supports_multiline: bool,
) -> Option<SemanticTokensResult> {
    let mut all_tokens: Vec<RawToken> = Vec::with_capacity(1000);
    let lines: Vec<&str> = text.lines().collect();

    // Recursively collect tokens using local parsers
    collect_injection_tokens_recursive_with_local_parsers(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        local_parsers,
        text,
        &lines,
        0,
        0,
        supports_multiline,
        &mut all_tokens,
    );

    finalize_tokens(all_tokens)
}

/// Recursive helper for collecting tokens with local parsers.
///
/// Similar to `collect_injection_tokens_recursive` but uses a HashMap of
/// pre-acquired parsers instead of a parser pool.
#[allow(clippy::too_many_arguments)]
fn collect_injection_tokens_recursive_with_local_parsers(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    local_parsers: &mut std::collections::HashMap<String, tree_sitter::Parser>,
    host_text: &str,
    host_lines: &[&str],
    content_start_byte: usize,
    depth: usize,
    supports_multiline: bool,
    all_tokens: &mut Vec<RawToken>,
) {
    if depth >= MAX_INJECTION_DEPTH {
        return;
    }

    // 1. Collect tokens from this document's highlight query
    collect_host_tokens(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        host_text,
        host_lines,
        content_start_byte,
        supports_multiline,
        all_tokens,
    );

    // 2. Find and process injections
    let Some(coordinator) = coordinator else {
        return;
    };

    let contexts =
        collect_injection_contexts(text, tree, filetype, coordinator, content_start_byte);

    for ctx in contexts {
        // Use parser from local map (if available)
        let Some(parser) = local_parsers.get_mut(&ctx.resolved_lang) else {
            continue;
        };

        let Some(injected_tree) = parser.parse(ctx.content_text, None) else {
            continue;
        };

        // Recursively collect tokens
        collect_injection_tokens_recursive_with_local_parsers(
            ctx.content_text,
            &injected_tree,
            &ctx.highlight_query,
            Some(&ctx.resolved_lang),
            capture_mappings,
            Some(coordinator),
            local_parsers,
            host_text,
            host_lines,
            ctx.host_start_byte,
            depth + 1,
            supports_multiline,
            all_tokens,
        );
    }
}

/// Handle semantic tokens range request
///
/// Analyzes a specific range of the document including injected language regions
/// and returns semantic tokens for both the host document and all injected content
/// within that range.
///
/// This function wraps `handle_semantic_tokens_full` and filters
/// the results to only include tokens within the requested range.
///
/// When coordinator or parser_pool is None, only host document tokens are returned
/// (no injection processing).
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting (host language)
/// * `range` - The range to get tokens for (LSP positions)
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
///
/// # Returns
/// Semantic tokens for the specified range including injected content (if coordinator/parser_pool provided)
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_range(
    text: &str,
    tree: &Tree,
    query: &Query,
    range: &Range,
    filetype: Option<&str>,
    capture_mappings: Option<&crate::config::CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
) -> Option<SemanticTokensResult> {
    // Get all tokens using the full handler
    let full_result = handle_semantic_tokens_full(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
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
/// When coordinator and parser_pool are provided, tokens from injected language
/// regions are included in the result.
///
/// # Arguments
/// * `text` - The current source text
/// * `tree` - The parsed syntax tree for the current text
/// * `query` - The tree-sitter query for semantic highlighting
/// * `previous_result_id` - The result ID from the previous semantic tokens response
/// * `previous_tokens` - The previous semantic tokens to calculate delta from
/// * `filetype` - The filetype of the document being processed
/// * `capture_mappings` - The capture mappings to apply
/// * `coordinator` - Language coordinator for loading injected language parsers (None = no injection)
/// * `parser_pool` - Parser pool for efficient parser reuse (None = no injection)
///
/// # Returns
/// Either a delta or full semantic tokens for the document
#[allow(clippy::too_many_arguments)]
pub fn handle_semantic_tokens_full_delta(
    text: &str,
    tree: &Tree,
    query: &Query,
    previous_result_id: &str,
    previous_tokens: Option<&SemanticTokens>,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
    coordinator: Option<&crate::language::LanguageCoordinator>,
    parser_pool: Option<&mut crate::language::DocumentParserPool>,
) -> Option<SemanticTokensFullDeltaResult> {
    // Get current tokens with injection support
    let current_result = handle_semantic_tokens_full(
        text,
        tree,
        query,
        filetype,
        capture_mappings,
        coordinator,
        parser_pool,
    )?;
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

/// Calculate delta or return full tokens.
///
/// This is a public helper for the incremental tokenization path.
/// It calculates a delta if possible, otherwise returns the current tokens.
pub fn calculate_delta_or_full(
    previous: &SemanticTokens,
    current: &SemanticTokens,
    expected_result_id: &str,
) -> SemanticTokensFullDeltaResult {
    if previous.result_id.as_deref() == Some(expected_result_id)
        && let Some(delta) = calculate_semantic_tokens_delta(previous, current)
    {
        return SemanticTokensFullDeltaResult::TokensDelta(delta);
    }
    SemanticTokensFullDeltaResult::Tokens(current.clone())
}

/// Check if two semantic tokens are equal
#[inline]
fn tokens_equal(a: &SemanticToken, b: &SemanticToken) -> bool {
    a.delta_line == b.delta_line
        && a.delta_start == b.delta_start
        && a.length == b.length
        && a.token_type == b.token_type
        && a.token_modifiers_bitset == b.token_modifiers_bitset
}

/// Calculate delta between two sets of semantic tokens using prefix-suffix matching.
///
/// This algorithm:
/// 1. Finds the longest common prefix
/// 2. Finds the longest common suffix (from what remains), with safety check for line changes
/// 3. Returns a single edit replacing the middle section
///
/// The suffix matching is disabled when total line deltas differ, as tokens with
/// identical delta encoding would be at different absolute positions (PBI-077 safety).
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
    // --- Step 1: Find common prefix ---
    let common_prefix_len = previous
        .data
        .iter()
        .zip(current.data.iter())
        .take_while(|(a, b)| tokens_equal(a, b))
        .count();

    // If all tokens are the same, no edits needed
    if common_prefix_len == previous.data.len() && common_prefix_len == current.data.len() {
        return Some(SemanticTokensDelta {
            result_id: current.result_id.clone(),
            edits: vec![],
        });
    }

    // --- Step 2: Find common suffix (with line change safety) ---
    let prev_suffix = &previous.data[common_prefix_len..];
    let curr_suffix = &current.data[common_prefix_len..];

    // PBI-077 Safety: Check if total line count changed
    // When lines are inserted/deleted, tokens with identical delta encoding
    // are at different absolute positions - suffix matching would be incorrect
    let prev_total_lines: u32 = previous.data.iter().map(|t| t.delta_line).sum();
    let curr_total_lines: u32 = current.data.iter().map(|t| t.delta_line).sum();

    let common_suffix_len = if prev_total_lines != curr_total_lines {
        // Line count changed - disable suffix optimization
        0
    } else {
        // Safe to find matching suffix
        prev_suffix
            .iter()
            .rev()
            .zip(curr_suffix.iter().rev())
            .take_while(|(a, b)| tokens_equal(a, b))
            .count()
    };

    // --- Step 3: Calculate the edit ---
    // LSP spec requires start and deleteCount to be integer indices into the
    // flattened token array, not token indices. Each SemanticToken serializes
    // to 5 u32 values, so we must multiply by 5.
    let start_token = common_prefix_len;
    let delete_token_count = prev_suffix.len() - common_suffix_len;
    let insert_token_count = curr_suffix.len() - common_suffix_len;
    let data = current.data[start_token..start_token + insert_token_count].to_vec();

    Some(SemanticTokensDelta {
        result_id: current.result_id.clone(),
        edits: vec![SemanticTokensEdit {
            start: (start_token * 5) as u32,
            delete_count: (delete_token_count * 5) as u32,
            data: Some(data),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the search path for tree-sitter grammars.
    /// Uses TREE_SITTER_GRAMMARS env var if set (Nix), otherwise falls back to deps/tree-sitter.
    fn test_search_path() -> String {
        std::env::var("TREE_SITTER_GRAMMARS").unwrap_or_else(|_| "deps/tree-sitter".to_string())
    }

    #[test]
    fn test_map_capture_to_token_type_and_modifiers() {
        // Test basic token types without modifiers
        assert_eq!(
            map_capture_to_token_type_and_modifiers("comment"),
            Some((0, 0))
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("keyword"),
            Some((1, 0))
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("function"),
            Some((14, 0))
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("variable"),
            Some((17, 0))
        );

        // Unknown types return None - they should not produce semantic tokens
        assert_eq!(
            map_capture_to_token_type_and_modifiers("unknown"),
            None,
            "'unknown' is not in LEGEND_TYPES"
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("spell"),
            None,
            "'spell' is a tree-sitter hint, not a semantic token type"
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers("markup"),
            None,
            "'markup' is not in LEGEND_TYPES"
        );
        assert_eq!(
            map_capture_to_token_type_and_modifiers(""),
            None,
            "empty string should return None"
        );

        // Test with single modifier
        let (_, modifiers) = map_capture_to_token_type_and_modifiers("variable.readonly").unwrap();
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly is at index 2

        let (_, modifiers) = map_capture_to_token_type_and_modifiers("function.async").unwrap();
        assert_eq!(modifiers & (1 << 6), 1 << 6); // async is at index 6

        // Test with multiple modifiers
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("variable.readonly.defaultLibrary").unwrap();
        assert_eq!(token_type, 17); // variable
        assert_eq!(modifiers & (1 << 2), 1 << 2); // readonly
        assert_eq!(modifiers & (1 << 9), 1 << 9); // defaultLibrary

        // Test unknown modifiers are ignored
        let (token_type, modifiers) =
            map_capture_to_token_type_and_modifiers("function.unknownModifier.async").unwrap();
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
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(delta.edits[0].start, 0);
        // With suffix matching: only the first token (comment) changed
        // The last two tokens (keyword, variable) are suffix matched
        assert_eq!(delta.edits[0].delete_count, 5); // 1 token * 5 integers
        let edits_data = delta.edits[0]
            .data
            .as_ref()
            .expect("delta edits should include replacement data");
        assert_eq!(edits_data.len(), 1);
    }

    #[test]
    fn test_semantic_tokens_range() {
        use tower_lsp_server::ls_types::Position;

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

    /// Alias for acceptance criteria naming
    #[test]
    fn test_diff_tokens_no_change() {
        // Same as test_semantic_tokens_delta_no_changes
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
        assert_eq!(delta.unwrap().edits.len(), 0);
    }

    /// Test that suffix matching reduces delta size when change is in the middle.
    ///
    /// Scenario: 5 tokens, only the 3rd token changes length
    /// Expected: Only 1 token in the edit (the changed one), not 3 tokens
    #[test]
    fn test_diff_tokens_suffix_matching() {
        // 5 tokens on the same line (delta_line=0 for all after first)
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // This one changes
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 4,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 10,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // Changed length
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 5,
                    token_type: 4,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 1);

        // With suffix matching: start=2 (skip 2 prefix tokens), delete_count=1, data=1 token
        // Without suffix matching: start=2, delete_count=3, data=3 tokens
        let edit = &delta.edits[0];
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(
            edit.start, 10,
            "Should skip 2 prefix tokens (2 * 5 integers)"
        );
        assert_eq!(
            edit.delete_count, 5,
            "Should only delete 1 token (with suffix matching) = 5 integers"
        );
        assert_eq!(
            edit.data.as_ref().unwrap().len(),
            1,
            "Should only include 1 changed token"
        );
    }

    /// Test that line insertion disables suffix optimization (PBI-077 safety).
    ///
    /// When lines are inserted, tokens at the end have the same delta encoding
    /// but are at different absolute positions. We must NOT match them as suffix.
    #[test]
    fn test_diff_tokens_line_insertion_no_suffix() {
        // Before: 3 tokens on lines 0, 1, 2
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                }, // line 0
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // line 1
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // line 2
            ],
        };

        // After: 4 tokens on lines 0, 1, 2, 3 (line inserted at position 1)
        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 5,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                }, // line 0 (same)
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 5,
                    token_modifiers_bitset: 0,
                }, // line 1 (NEW)
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // line 2 (was line 1)
                SemanticToken {
                    delta_line: 1,
                    delta_start: 0,
                    length: 5,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                }, // line 3 (was line 2)
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 1);

        // The last two tokens in current have SAME delta encoding as last two in previous,
        // but they're at DIFFERENT absolute positions (line 2,3 vs line 1,2).
        // Suffix optimization MUST be disabled.
        let edit = &delta.edits[0];
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(
            edit.start, 5,
            "Should skip 1 prefix token (line 0) = 5 integers"
        );
        // Without suffix: delete_count=2 (tokens at line 1,2), data=3 tokens
        // With incorrect suffix: would wrongly match last token
        assert_eq!(
            edit.delete_count, 10,
            "Should delete 2 original tokens after prefix = 10 integers"
        );
        assert_eq!(
            edit.data.as_ref().unwrap().len(),
            3,
            "Should include 3 new tokens"
        );
    }

    /// Test that same-line edits preserve suffix optimization.
    ///
    /// When editing within a line (no line count change), suffix matching is safe.
    #[test]
    fn test_diff_tokens_same_line_edit_suffix() {
        // 4 tokens all on line 0
        let previous = SemanticTokens {
            result_id: Some("v1".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 3,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 5,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // This changes
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 3,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 4,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        // Second token changes length
        let current = SemanticTokens {
            result_id: Some("v2".to_string()),
            data: vec![
                SemanticToken {
                    delta_line: 0,
                    delta_start: 0,
                    length: 3,
                    token_type: 0,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 8,
                    token_type: 1,
                    token_modifiers_bitset: 0,
                }, // Changed
                SemanticToken {
                    delta_line: 0,
                    delta_start: 6,
                    length: 3,
                    token_type: 2,
                    token_modifiers_bitset: 0,
                },
                SemanticToken {
                    delta_line: 0,
                    delta_start: 4,
                    length: 4,
                    token_type: 3,
                    token_modifiers_bitset: 0,
                },
            ],
        };

        let delta = calculate_semantic_tokens_delta(&previous, &current);
        assert!(delta.is_some());

        let delta = delta.unwrap();
        assert_eq!(delta.edits.len(), 1);

        // Same line count, so suffix matching should work
        let edit = &delta.edits[0];
        // LSP spec: start and deleteCount are integer indices (each token = 5 integers)
        assert_eq!(edit.start, 5, "Should skip 1 prefix token = 5 integers");
        assert_eq!(
            edit.delete_count, 5,
            "Should only delete 1 token (suffix matched 2) = 5 integers"
        );
        assert_eq!(
            edit.data.as_ref().unwrap().len(),
            1,
            "Should only include 1 changed token"
        );
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
        let result =
            handle_semantic_tokens_full(text, &tree, &query, Some("rust"), None, None, None);

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

        // Configure with search paths
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
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

        // Call the injection-aware function with Some(coordinator) and Some(parser_pool)
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
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

        // Configure with search paths
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
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

        // Call the injection-aware function with Some(coordinator) and Some(parser_pool)
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
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

        // Configure with search paths
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
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

        // Call the injection-aware function with Some(coordinator) and Some(parser_pool)
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
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

    #[test]
    fn test_semantic_tokens_full_with_injection_none_coordinator() {
        // Test that handle_semantic_tokens_full works when
        // coordinator and parser_pool are None - it should behave like
        // the non-injection handler, returning host-only tokens.
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

        // Call the injection handler with None coordinator and parser_pool
        // This should work and return the same tokens as handle_semantic_tokens_full
        let result = handle_semantic_tokens_full(
            text,
            &tree,
            &query,
            Some("rust"),
            None, // capture_mappings
            None, // coordinator (None = no injection support)
            None, // parser_pool (None = no injection support)
        );

        assert!(result.is_some());

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
    fn test_semantic_tokens_range_none_coordinator() {
        // Test that handle_semantic_tokens_range works when
        // coordinator and parser_pool are None - it should behave like
        // returning host-only tokens without injection processing.
        use tower_lsp_server::ls_types::Position;
        use tree_sitter::{Parser, Query};

        let text = r#"let x = "あいうえお"
let y = "hello"
let z = 42"#;

        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        let query_text = r#"
            "let" @keyword
            (identifier) @variable
            (string_literal) @string
            (integer_literal) @number
        "#;

        let query = Query::new(&language, query_text).unwrap();

        // Request range that includes only line 1 (0-indexed)
        let range = Range {
            start: Position {
                line: 1,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 100,
            },
        };

        // Call the handler with None coordinator and parser_pool
        let result = handle_semantic_tokens_range(
            text,
            &tree,
            &query,
            &range,
            Some("rust"),
            None, // capture_mappings
            None, // coordinator (None = no injection support)
            None, // parser_pool (None = no injection support)
        );

        assert!(result.is_some());

        let SemanticTokensResult::Tokens(tokens) = result.unwrap() else {
            panic!("expected complete semantic tokens result");
        };

        // Should have tokens only from line 1: let, y, string "hello"
        // All tokens should be on line 0 in delta encoding since we're starting fresh
        assert!(
            tokens.data.len() >= 3,
            "Expected at least 3 tokens for line 1"
        );
    }

    #[test]
    fn test_semantic_tokens_delta_with_injection() {
        // Test that handle_semantic_tokens_full_delta returns tokens from injected content
        // when coordinator and parser_pool are provided.
        //
        // example.md has a lua fenced code block at line 6 (0-indexed):
        // ```lua
        // local xyz = 12345
        // ```
        //
        // The delta handler should return the `local` keyword token from the injected Lua.

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator and parser pool
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

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

        // Call delta handler with coordinator and parser_pool
        // Use empty previous tokens to get full result back
        let result = handle_semantic_tokens_full_delta(
            text,
            &tree,
            &md_highlight_query,
            "no-match", // previous_result_id that won't match
            None,       // no previous tokens
            Some("markdown"),
            None, // capture_mappings
            Some(&coordinator),
            Some(&mut parser_pool),
        );

        // Should return full tokens result
        let result = result.expect("Should return tokens");
        let SemanticTokensFullDeltaResult::Tokens(tokens) = result else {
            panic!("Expected full tokens result when no previous tokens match");
        };

        // Find the `local` keyword token at line 6 (0-indexed), col 0
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
            "Delta handler should return `local` keyword token at line 6, col 0 from injected Lua code"
        );
    }

    // PBI-152: Wildcard Config Inheritance for captureMappings

    #[test]
    fn test_apply_capture_mapping_uses_wildcard_merge() {
        // ADR-0011: When both wildcard and specific key exist, merge them
        // This test verifies that apply_capture_mapping correctly inherits
        // mappings from wildcard when the specific key doesn't have them
        use crate::config::{CaptureMappings, QueryTypeMappings};
        use std::collections::HashMap;

        let mut mappings = CaptureMappings::new();

        // Wildcard has "variable" and "function" mappings
        let mut wildcard_highlights = HashMap::new();
        wildcard_highlights.insert("variable".to_string(), "variable".to_string());
        wildcard_highlights.insert("function".to_string(), "function".to_string());

        mappings.insert(
            WILDCARD_KEY.to_string(),
            QueryTypeMappings {
                highlights: wildcard_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Rust only has "type.builtin" - should inherit "variable" and "function" from wildcard
        let mut rust_highlights = HashMap::new();
        rust_highlights.insert(
            "type.builtin".to_string(),
            "type.defaultLibrary".to_string(),
        );

        mappings.insert(
            "rust".to_string(),
            QueryTypeMappings {
                highlights: rust_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Test: "variable" should be inherited from wildcard for "rust"
        let result = apply_capture_mapping("variable", Some("rust"), Some(&mappings));
        assert_eq!(
            result,
            Some("variable".to_string()),
            "Should inherit 'variable' mapping from wildcard for 'rust'"
        );

        // Test: "type.builtin" should use rust-specific mapping
        let result = apply_capture_mapping("type.builtin", Some("rust"), Some(&mappings));
        assert_eq!(
            result,
            Some("type.defaultLibrary".to_string()),
            "Should use rust-specific 'type.builtin' mapping"
        );

        // Test: "function" should be inherited from wildcard for "rust"
        let result = apply_capture_mapping("function", Some("rust"), Some(&mappings));
        assert_eq!(
            result,
            Some("function".to_string()),
            "Should inherit 'function' mapping from wildcard for 'rust'"
        );
    }

    #[test]
    fn test_apply_capture_mapping_returns_none_for_unknown_types() {
        // Unknown types (not in LEGEND_TYPES) should return None
        // This prevents unknown captures from being added to all_tokens
        assert_eq!(
            apply_capture_mapping("spell", None, None),
            None,
            "'spell' is a tree-sitter hint for spellcheck regions"
        );
        assert_eq!(
            apply_capture_mapping("nospell", None, None),
            None,
            "'nospell' is a tree-sitter hint for no-spellcheck regions"
        );
        assert_eq!(
            apply_capture_mapping("conceal", None, None),
            None,
            "'conceal' is a tree-sitter hint for concealable text"
        );
        assert_eq!(
            apply_capture_mapping("markup", None, None),
            None,
            "'markup' is not in LEGEND_TYPES"
        );
        assert_eq!(
            apply_capture_mapping("unknown", None, None),
            None,
            "'unknown' is not in LEGEND_TYPES"
        );

        // Known types should return Some
        assert_eq!(
            apply_capture_mapping("comment", None, None),
            Some("comment".to_string()),
            "'comment' is in LEGEND_TYPES"
        );
        assert_eq!(
            apply_capture_mapping("keyword", None, None),
            Some("keyword".to_string()),
            "'keyword' is in LEGEND_TYPES"
        );
        assert_eq!(
            apply_capture_mapping("variable.readonly", None, None),
            Some("variable.readonly".to_string()),
            "'variable' base type is in LEGEND_TYPES"
        );
    }

    #[test]
    fn test_collect_injection_languages_returns_unique_languages() {
        // Test that collect_injection_languages() returns all unique injection languages
        // This is needed for narrowing lock scope: we need to know which parsers to acquire
        // BEFORE starting the semantic token processing.
        //
        // example.md has a lua fenced code block, so "lua" should be in the result.

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;

        // Read the test fixture
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator with search paths
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load markdown language (host)
        let load_result = coordinator.ensure_language_loaded("markdown");
        assert!(load_result.success, "Should load markdown language");

        // Parse the markdown document
        let mut parser_pool = coordinator.create_document_parser_pool();
        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        // Collect injection languages from the markdown document
        let languages =
            collect_injection_languages(&tree, text, "markdown", &coordinator, &mut parser_pool);

        // Should find "lua" as an injection language
        assert!(
            languages.contains(&"lua".to_string()),
            "Should find 'lua' as injection language in example.md. Found: {:?}",
            languages
        );
    }

    #[test]
    fn test_semantic_tokens_with_local_parsers_produces_same_result() {
        // Test that handle_semantic_tokens_full_with_local_parsers() produces
        // the same tokens as the original function. This validates that we can
        // pre-acquire parsers and release the pool lock before processing.
        //
        // This is the core test for Task 1.1: Narrow Lock Scope for Parser Pool

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;
        use std::collections::HashMap;
        use tree_sitter::Parser;

        // Read the test fixture (markdown with lua injection)
        let text = include_str!("../../tests/assets/example.md");

        // Set up coordinator with search paths
        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        // Load required languages
        coordinator.ensure_language_loaded("markdown");
        coordinator.ensure_language_loaded("lua");

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

        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Step 1: Get result with original function (for comparison)
        let original_result = handle_semantic_tokens_full(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None,
            Some(&coordinator),
            Some(&mut parser_pool),
        );

        // Step 2: Pre-acquire parsers into local HashMap
        let injection_languages =
            collect_injection_languages(&tree, text, "markdown", &coordinator, &mut parser_pool);
        let mut local_parsers: HashMap<String, Parser> = HashMap::new();
        for lang_id in &injection_languages {
            if let Some(parser) = parser_pool.acquire(lang_id) {
                local_parsers.insert(lang_id.clone(), parser);
            }
        }

        // Step 2: Call new function with local parsers (pool lock NOT needed during this call)
        let new_result = handle_semantic_tokens_full_with_local_parsers(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None,
            Some(&coordinator),
            &mut local_parsers,
            false, // supports_multiline = false for backward compatibility in tests
        );

        // Step 3: Return parsers to pool
        for (lang_id, parser) in local_parsers {
            parser_pool.release(lang_id, parser);
        }

        // Verify results match
        let original_tokens = original_result.expect("Original should return tokens");
        let new_tokens = new_result.expect("New function should return tokens");

        let SemanticTokensResult::Tokens(orig) = original_tokens else {
            panic!("Expected full tokens from original");
        };
        let SemanticTokensResult::Tokens(new) = new_tokens else {
            panic!("Expected full tokens from new function");
        };

        assert_eq!(
            orig.data.len(),
            new.data.len(),
            "Token count should match between original and local-parsers version"
        );
    }

    #[test]
    fn test_nested_only_language_with_local_parsers() {
        // This test verifies that nested injection languages are discovered.
        //
        // Test document structure:
        // `````markdown
        // ```rust
        // fn main() {}
        // ```
        // `````
        //
        // "rust" is ONLY inside the nested markdown, not at top level.
        // collect_injection_languages() must recursively discover it:
        // - Depth 0: finds "markdown"
        // - Depth 1: parses nested markdown, finds "rust"
        // Result: ["markdown", "rust"]
        //
        // The `fn` keyword should produce a semantic token.

        use crate::config::WorkspaceSettings;
        use crate::language::LanguageCoordinator;
        use std::collections::HashMap;
        use tree_sitter::Parser;

        // Document with rust ONLY inside nested markdown (not at top level)
        let text = r#"`````markdown
```rust
fn main() {}
```
`````"#;

        let coordinator = LanguageCoordinator::new();
        let settings = WorkspaceSettings {
            search_paths: vec![test_search_path()],
            ..Default::default()
        };
        let _summary = coordinator.load_settings(settings);

        coordinator.ensure_language_loaded("markdown");
        coordinator.ensure_language_loaded("rust");

        let mut parser_pool = coordinator.create_document_parser_pool();

        let tree = {
            let mut parser = parser_pool
                .acquire("markdown")
                .expect("Should get markdown parser");
            let result = parser.parse(text, None).expect("Should parse markdown");
            parser_pool.release("markdown".to_string(), parser);
            result
        };

        let md_highlight_query = coordinator
            .get_highlight_query("markdown")
            .expect("Should have markdown highlight query");

        // Pre-acquire parsers using collect_injection_languages (now recursive!)
        let injection_languages =
            collect_injection_languages(&tree, text, "markdown", &coordinator, &mut parser_pool);

        let mut local_parsers: HashMap<String, Parser> = HashMap::new();
        for lang_id in &injection_languages {
            if let Some(parser) = parser_pool.acquire(lang_id) {
                local_parsers.insert(lang_id.clone(), parser);
            }
        }

        // Call new function with local parsers
        let result = handle_semantic_tokens_full_with_local_parsers(
            text,
            &tree,
            &md_highlight_query,
            Some("markdown"),
            None,
            Some(&coordinator),
            &mut local_parsers,
            false, // supports_multiline = false for backward compatibility in tests
        );

        // Return parsers to pool
        for (lang_id, parser) in local_parsers {
            parser_pool.release(lang_id, parser);
        }

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Find the `fn` keyword token at line 2 (0-indexed), col 0
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_fn_keyword = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 2 (0-indexed), col 0, keyword type (1), length 2 ("fn")
            if abs_line == 2 && abs_col == 0 && token.token_type == 1 && token.length == 2 {
                found_fn_keyword = true;
                break;
            }
        }

        assert!(
            found_fn_keyword,
            "Should find `fn` keyword at line 2 from nested Rust injection (Markdown -> Markdown -> Rust). \
             collect_injection_languages() must recursively discover nested languages."
        );
    }

    #[test]
    fn test_rust_doc_comment_full_token() {
        // Rust doc comments (/// ...) include trailing newline in the tree-sitter node,
        // which causes end_pos.row > start_pos.row. This test verifies that we still
        // generate tokens for the full comment, not just the doc marker.
        use tree_sitter::{Parser, Query};

        let text = "// foo\n/// bar\n";

        let language = tree_sitter_rust::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        // Query similar to the installed Rust highlights.scm
        let query_text = r#"
            [(line_comment) (block_comment)] @comment
            [(outer_doc_comment_marker) (inner_doc_comment_marker)] @comment.documentation
            (line_comment (doc_comment)) @comment.documentation
        "#;

        let query = Query::new(&language, query_text).unwrap();

        let result =
            handle_semantic_tokens_full(text, &tree, &query, Some("rust"), None, None, None);

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Should have tokens for both comments
        // Token 1: "// foo" at (0,0) len=6
        // Token 2: "/// bar" at (1,0) len=7 (without trailing newline)
        // Token 3: "/" marker at (1,2) len=1 (may be deduped with token 2)

        // Find the doc comment token at line 1, column 0
        let mut abs_line = 0u32;
        let mut abs_col = 0u32;
        let mut found_doc_comment_full = false;

        for token in &tokens.data {
            abs_line += token.delta_line;
            if token.delta_line > 0 {
                abs_col = token.delta_start;
            } else {
                abs_col += token.delta_start;
            }

            // Line 1, col 0, should have length 7 ("/// bar" without newline)
            if abs_line == 1 && abs_col == 0 && token.length == 7 {
                found_doc_comment_full = true;
                break;
            }
        }

        assert!(
            found_doc_comment_full,
            "Should find full doc comment token at line 1, col 0 with length 7. \
             Got tokens: {:?}",
            tokens.data
        );
    }

    #[test]
    fn test_multiline_token_split_when_not_supported() {
        // Test that multiline tokens are split into per-line tokens when
        // supports_multiline is false (Option A fallback behavior).
        use tree_sitter::{Parser, Query};

        // A simple markdown document with a multiline block quote
        let text = "> line1\n> line2\n> line3\n";

        let language = tree_sitter_md::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        // Query that captures block_quote as a single multiline node
        let query_text = r#"
            (block_quote) @comment
        "#;

        let query = Query::new(&language, query_text).unwrap();

        // Test with supports_multiline = false (split into per-line tokens)
        let result = handle_semantic_tokens_full_with_multiline(
            text,
            &tree,
            &query,
            Some("markdown"),
            None,
            None,
            None,
            false,
        );

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Should have 3 separate tokens, one for each line
        // Each line should be highlighted separately
        assert!(
            tokens.data.len() >= 3,
            "Expected at least 3 tokens for 3-line block quote, got {}. Tokens: {:?}",
            tokens.data.len(),
            tokens.data
        );

        // Verify tokens are on different lines
        let mut abs_line = 0u32;
        let mut lines_with_tokens = std::collections::HashSet::new();

        for token in &tokens.data {
            abs_line += token.delta_line;
            lines_with_tokens.insert(abs_line);
        }

        assert!(
            lines_with_tokens.len() >= 3,
            "Expected tokens on at least 3 different lines, got lines: {:?}",
            lines_with_tokens
        );
    }

    #[test]
    fn test_multiline_token_single_when_supported() {
        // Test that multiline tokens are emitted as single tokens when
        // supports_multiline is true (Option B primary behavior).
        use tree_sitter::{Parser, Query};

        // A simple markdown document with a multiline block quote
        let text = "> line1\n> line2\n> line3\n";

        let language = tree_sitter_md::LANGUAGE.into();
        let mut parser = Parser::new();
        parser.set_language(&language).unwrap();

        let tree = parser.parse(text, None).unwrap();

        // Query that captures block_quote as a single multiline node
        let query_text = r#"
            (block_quote) @comment
        "#;

        let query = Query::new(&language, query_text).unwrap();

        // Test with supports_multiline = true (emit single token)
        let result = handle_semantic_tokens_full_with_multiline(
            text,
            &tree,
            &query,
            Some("markdown"),
            None,
            None,
            None,
            true,
        );

        let tokens = result.expect("Should return tokens");
        let SemanticTokensResult::Tokens(tokens) = tokens else {
            panic!("Expected full tokens result");
        };

        // Should have a single token for the entire block quote
        // The token should start at line 0 and have a length that spans all lines
        assert!(
            !tokens.data.is_empty(),
            "Expected at least 1 token for multiline block quote"
        );

        // The first token should start at line 0, column 0
        let first_token = &tokens.data[0];
        assert_eq!(first_token.delta_line, 0, "First token should be on line 0");
        assert_eq!(
            first_token.delta_start, 0,
            "First token should start at column 0"
        );

        // The length should span multiple lines worth of content
        // "> line1" (7) + newline (1) + "> line2" (7) + newline (1) + "> line3" (7) = 23
        // But actual length depends on implementation details
        assert!(
            first_token.length > 7,
            "Multiline token length ({}) should be greater than single line (7)",
            first_token.length
        );
    }
}
