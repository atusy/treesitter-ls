use serde::de::DeserializeOwned;
use serde::Serialize;

use lsp_types as new_lsp;
use tower_lsp::lsp_types as old_lsp;

fn convert<T, U>(value: T) -> Option<U>
where
    T: Serialize,
    U: DeserializeOwned,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|json| serde_json::from_value(json).ok())
}

pub fn to_domain_position(pos: &old_lsp::Position) -> new_lsp::Position {
    new_lsp::Position::new(pos.line, pos.character)
}

pub fn to_domain_range(range: &old_lsp::Range) -> new_lsp::Range {
    new_lsp::Range::new(
        to_domain_position(&range.start),
        to_domain_position(&range.end),
    )
}

pub fn to_lsp_position(pos: &new_lsp::Position) -> old_lsp::Position {
    old_lsp::Position::new(pos.line, pos.character)
}

pub fn to_lsp_range(range: &new_lsp::Range) -> old_lsp::Range {
    old_lsp::Range::new(to_lsp_position(&range.start), to_lsp_position(&range.end))
}

pub fn to_lsp_selection_range(range: &new_lsp::SelectionRange) -> old_lsp::SelectionRange {
    old_lsp::SelectionRange {
        range: to_lsp_range(&range.range),
        parent: range
            .parent
            .as_ref()
            .map(|parent| Box::new(to_lsp_selection_range(parent))),
    }
}

pub fn to_lsp_semantic_token(token: new_lsp::SemanticToken) -> old_lsp::SemanticToken {
    old_lsp::SemanticToken {
        delta_line: token.delta_line,
        delta_start: token.delta_start,
        length: token.length,
        token_type: token.token_type,
        token_modifiers_bitset: token.token_modifiers_bitset,
    }
}

pub fn to_lsp_semantic_tokens(tokens: new_lsp::SemanticTokens) -> old_lsp::SemanticTokens {
    old_lsp::SemanticTokens {
        result_id: tokens.result_id,
        data: tokens.data.into_iter().map(to_lsp_semantic_token).collect(),
    }
}

pub fn to_lsp_semantic_tokens_edit(
    edit: new_lsp::SemanticTokensEdit,
) -> old_lsp::SemanticTokensEdit {
    old_lsp::SemanticTokensEdit {
        start: edit.start,
        delete_count: edit.delete_count,
        data: edit
            .data
            .map(|tokens| tokens.into_iter().map(to_lsp_semantic_token).collect()),
    }
}

pub fn to_lsp_semantic_tokens_delta(
    delta: new_lsp::SemanticTokensDelta,
) -> old_lsp::SemanticTokensDelta {
    old_lsp::SemanticTokensDelta {
        result_id: delta.result_id,
        edits: delta
            .edits
            .into_iter()
            .map(to_lsp_semantic_tokens_edit)
            .collect(),
    }
}

pub fn to_lsp_semantic_tokens_full_delta(
    result: new_lsp::SemanticTokensFullDeltaResult,
) -> old_lsp::SemanticTokensFullDeltaResult {
    match result {
        new_lsp::SemanticTokensFullDeltaResult::Tokens(tokens) => {
            old_lsp::SemanticTokensFullDeltaResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        new_lsp::SemanticTokensFullDeltaResult::TokensDelta(delta) => {
            old_lsp::SemanticTokensFullDeltaResult::TokensDelta(to_lsp_semantic_tokens_delta(delta))
        }
        new_lsp::SemanticTokensFullDeltaResult::PartialTokensDelta { edits } => {
            old_lsp::SemanticTokensFullDeltaResult::PartialTokensDelta {
                edits: edits.into_iter().map(to_lsp_semantic_tokens_edit).collect(),
            }
        }
    }
}

pub fn to_lsp_semantic_tokens_result(
    result: new_lsp::SemanticTokensResult,
) -> old_lsp::SemanticTokensResult {
    match result {
        new_lsp::SemanticTokensResult::Tokens(tokens) => {
            old_lsp::SemanticTokensResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        new_lsp::SemanticTokensResult::Partial(partial) => convert(partial)
            .map(old_lsp::SemanticTokensResult::Partial)
            .unwrap_or_else(|| {
                old_lsp::SemanticTokensResult::Tokens(old_lsp::SemanticTokens {
                    result_id: None,
                    data: vec![],
                })
            }),
    }
}

pub fn to_lsp_semantic_tokens_range_result(
    result: new_lsp::SemanticTokensRangeResult,
) -> old_lsp::SemanticTokensRangeResult {
    match result {
        new_lsp::SemanticTokensRangeResult::Tokens(tokens) => {
            old_lsp::SemanticTokensRangeResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        new_lsp::SemanticTokensRangeResult::Partial(partial) => convert(partial)
            .map(old_lsp::SemanticTokensRangeResult::Partial)
            .unwrap_or_else(|| {
                old_lsp::SemanticTokensRangeResult::Tokens(old_lsp::SemanticTokens {
                    result_id: None,
                    data: vec![],
                })
            }),
    }
}

pub fn to_lsp_definition_response(
    resp: new_lsp::GotoDefinitionResponse,
) -> Option<old_lsp::GotoDefinitionResponse> {
    convert(resp).and_then(|converted: old_lsp::GotoDefinitionResponse| match &converted {
        old_lsp::GotoDefinitionResponse::Array(items) if items.is_empty() => None,
        _ => Some(converted),
    })
}

pub fn to_lsp_workspace_edit(edit: new_lsp::WorkspaceEdit) -> old_lsp::WorkspaceEdit {
    if let Some(converted) = convert(edit.clone()) {
        return converted;
    }

    let changes = edit.changes.map(|map| {
        map.into_iter()
            .filter_map(|(uri, edits)| {
                old_lsp::Url::parse(uri.as_str()).ok().map(|url| {
                    let edits = edits
                        .into_iter()
                        .map(|e| old_lsp::TextEdit {
                            range: to_lsp_range(&e.range),
                            new_text: e.new_text,
                        })
                        .collect();
                    (url, edits)
                })
            })
            .collect()
    });

    old_lsp::WorkspaceEdit {
        changes,
        document_changes: None,
        change_annotations: None,
    }
}

pub fn to_lsp_code_action(action: new_lsp::CodeAction) -> old_lsp::CodeAction {
    if let Some(converted) = convert(action.clone()) {
        return converted;
    }

    old_lsp::CodeAction {
        title: action.title,
        kind: action.kind.map(|kind| old_lsp::CodeActionKind::from(kind.as_str().to_string())),
        diagnostics: action.diagnostics.and_then(convert),
        edit: action.edit.map(to_lsp_workspace_edit),
        command: action.command.and_then(convert),
        is_preferred: action.is_preferred,
        disabled: action
            .disabled
            .map(|d| old_lsp::CodeActionDisabled { reason: d.reason }),
        data: action.data,
    }
}

pub fn to_lsp_code_action_response(
    actions: Vec<new_lsp::CodeActionOrCommand>,
) -> Vec<old_lsp::CodeActionOrCommand> {
    actions
        .into_iter()
        .filter_map(|item| match item {
            new_lsp::CodeActionOrCommand::CodeAction(action) => Some(
                old_lsp::CodeActionOrCommand::CodeAction(to_lsp_code_action(action)),
            ),
            new_lsp::CodeActionOrCommand::Command(command) =>
                convert(command).map(old_lsp::CodeActionOrCommand::Command),
        })
        .collect()
}
