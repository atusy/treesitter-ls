use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenType, SemanticTokens, SemanticTokensResult,
};
use tree_sitter::{Query, QueryCursor, StreamingIterator, Tree};

/// LSP semantic token types legend
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

/// Handle semantic tokens full request
///
/// Analyzes the entire document and returns all semantic tokens.
///
/// # Arguments
/// * `text` - The source text
/// * `tree` - The parsed syntax tree
/// * `query` - The tree-sitter query for semantic highlighting
///
/// # Returns
/// Semantic tokens for the entire document
pub fn handle_semantic_tokens_full(
    text: &str,
    tree: &Tree,
    query: &Query,
) -> Option<SemanticTokensResult> {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), text.as_bytes());

    // Collect all tokens with their positions
    let mut tokens = vec![];
    while let Some(m) = matches.next() {
        for c in m.captures {
            let node = c.node;
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Only include single-line tokens
            if start_pos.row == end_pos.row {
                tokens.push((
                    start_pos.row,
                    start_pos.column,
                    end_pos.column - start_pos.column,
                    c.index,
                ));
            }
        }
    }

    // Sort tokens by position
    tokens.sort();

    // Convert to LSP semantic tokens format (relative positions)
    let mut last_line = 0;
    let mut last_start = 0;
    let mut data = Vec::new();

    for (line, start, length, capture_index) in tokens {
        let delta_line = line - last_line;
        let delta_start = if delta_line == 0 {
            start - last_start
        } else {
            start
        };

        // Map capture name to token type
        let token_type_name = &query.capture_names()[capture_index as usize];
        let token_type = LEGEND_TYPES
            .iter()
            .position(|t| t.as_str() == *token_type_name)
            .unwrap_or(0);

        data.push(SemanticToken {
            delta_line: delta_line as u32,
            delta_start: delta_start as u32,
            length: length as u32,
            token_type: token_type as u32,
            token_modifiers_bitset: 0,
        });

        last_line = line;
        last_start = start;
    }

    Some(SemanticTokensResult::Tokens(SemanticTokens {
        result_id: None,
        data,
    }))
}
