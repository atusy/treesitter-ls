// Most conversions are now identity functions since we're using lsp_types directly

pub fn to_domain_position(pos: &tower_lsp::lsp_types::Position) -> tower_lsp::lsp_types::Position {
    *pos
}

pub fn to_domain_range(range: &tower_lsp::lsp_types::Range) -> tower_lsp::lsp_types::Range {
    *range
}

pub fn to_lsp_position(pos: &tower_lsp::lsp_types::Position) -> tower_lsp::lsp_types::Position {
    *pos
}

pub fn to_lsp_range(range: &tower_lsp::lsp_types::Range) -> tower_lsp::lsp_types::Range {
    *range
}

pub fn to_lsp_selection_range(
    range: &tower_lsp::lsp_types::SelectionRange,
) -> tower_lsp::lsp_types::SelectionRange {
    range.clone()
}

pub fn to_lsp_semantic_tokens(
    tokens: tower_lsp::lsp_types::SemanticTokens,
) -> tower_lsp::lsp_types::SemanticTokens {
    tokens
}

pub fn to_lsp_semantic_tokens_full_delta(
    result: tower_lsp::lsp_types::SemanticTokensFullDeltaResult,
) -> tower_lsp::lsp_types::SemanticTokensFullDeltaResult {
    result
}

pub fn to_lsp_semantic_tokens_range_result(
    result: tower_lsp::lsp_types::SemanticTokensRangeResult,
) -> tower_lsp::lsp_types::SemanticTokensRangeResult {
    result
}

pub fn to_lsp_definition_response(
    resp: tower_lsp::lsp_types::GotoDefinitionResponse,
) -> Option<tower_lsp::lsp_types::GotoDefinitionResponse> {
    match &resp {
        tower_lsp::lsp_types::GotoDefinitionResponse::Array(locations) if locations.is_empty() => {
            None
        }
        _ => Some(resp),
    }
}

pub fn to_lsp_code_action(
    action: tower_lsp::lsp_types::CodeAction,
) -> tower_lsp::lsp_types::CodeAction {
    action
}

pub fn to_lsp_code_action_response(
    actions: Vec<tower_lsp::lsp_types::CodeActionOrCommand>,
) -> Vec<tower_lsp::lsp_types::CodeActionOrCommand> {
    actions
}
