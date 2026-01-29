//! Token collection from tree-sitter queries.
//!
//! This module handles the collection of raw tokens from a single document's
//! highlight query, including multiline token handling and byte-to-UTF16 conversion.

use crate::config::CaptureMappings;
use crate::text::convert_byte_to_utf16_in_line;
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

use super::legend::apply_capture_mapping;

/// Represents a token before delta encoding with all position information.
#[derive(Clone, Debug)]
pub(crate) struct RawToken {
    /// 0-indexed line number in the host document
    pub line: usize,
    /// UTF-16 column position within the line
    pub column: usize,
    /// Length in UTF-16 code units
    pub length: usize,
    /// Mapped capture name (e.g., "keyword", "variable.readonly")
    pub mapped_name: String,
    /// Injection depth (0 = host document)
    pub depth: usize,
}

/// Convert byte column position to UTF-16 column position within a line
/// This is a wrapper around the common utility for backward compatibility
pub(super) fn byte_to_utf16_col(line: &str, byte_col: usize) -> usize {
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
pub(super) fn collect_host_tokens(
    text: &str,
    tree: &Tree,
    query: &Query,
    filetype: Option<&str>,
    capture_mappings: Option<&CaptureMappings>,
    host_text: &str,
    host_lines: &[&str],
    content_start_byte: usize,
    depth: usize,
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

                all_tokens.push(RawToken {
                    line: host_line,
                    column: start_utf16,
                    length: end_utf16 - start_utf16,
                    mapped_name,
                    depth,
                });
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

                all_tokens.push(RawToken {
                    line: host_start_line,
                    column: start_utf16,
                    length: total_length_utf16,
                    mapped_name,
                    depth,
                });
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
                        all_tokens.push(RawToken {
                            line: host_row,
                            column: start_utf16,
                            length: end_utf16 - start_utf16,
                            mapped_name: mapped_name.clone(),
                            depth,
                        });
                    }
                }
            }
        }
    }
}
