use std::collections::HashMap;

use serde::de::DeserializeOwned;

use crate::domain;
use tower_lsp::lsp_types;

fn convert_via_json<T, U>(value: T) -> Option<U>
where
    T: serde::Serialize,
    U: DeserializeOwned,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|json| serde_json::from_value(json).ok())
}

pub fn to_domain_position(pos: &lsp_types::Position) -> domain::Position {
    domain::Position::new(pos.line, pos.character)
}

pub fn to_domain_range(range: &lsp_types::Range) -> domain::Range {
    domain::Range::new(
        to_domain_position(&range.start),
        to_domain_position(&range.end),
    )
}

pub fn to_lsp_position(pos: &domain::Position) -> lsp_types::Position {
    lsp_types::Position::new(pos.line, pos.character)
}

pub fn to_lsp_range(range: &domain::Range) -> lsp_types::Range {
    lsp_types::Range::new(to_lsp_position(&range.start), to_lsp_position(&range.end))
}

pub fn to_lsp_selection_range(range: &domain::SelectionRange) -> lsp_types::SelectionRange {
    lsp_types::SelectionRange {
        range: to_lsp_range(&range.range),
        parent: range
            .parent
            .as_ref()
            .map(|parent| Box::new(to_lsp_selection_range(parent))),
    }
}

pub fn to_lsp_semantic_token(token: domain::SemanticToken) -> lsp_types::SemanticToken {
    lsp_types::SemanticToken {
        delta_line: token.delta_line,
        delta_start: token.delta_start,
        length: token.length,
        token_type: token.token_type,
        token_modifiers_bitset: token.token_modifiers_bitset,
    }
}

pub fn to_lsp_semantic_tokens(tokens: domain::SemanticTokens) -> lsp_types::SemanticTokens {
    lsp_types::SemanticTokens {
        result_id: tokens.result_id,
        data: tokens.data.into_iter().map(to_lsp_semantic_token).collect(),
    }
}

pub fn to_lsp_semantic_tokens_edit(
    edit: domain::SemanticTokensEdit,
) -> lsp_types::SemanticTokensEdit {
    lsp_types::SemanticTokensEdit {
        start: edit.start,
        delete_count: edit.delete_count,
        data: edit
            .data
            .map(|tokens| tokens.into_iter().map(to_lsp_semantic_token).collect()),
    }
}

pub fn to_lsp_semantic_tokens_delta(
    delta: domain::SemanticTokensDelta,
) -> lsp_types::SemanticTokensDelta {
    lsp_types::SemanticTokensDelta {
        result_id: delta.result_id,
        edits: delta
            .edits
            .into_iter()
            .map(to_lsp_semantic_tokens_edit)
            .collect(),
    }
}

pub fn to_lsp_semantic_tokens_full_delta(
    result: domain::SemanticTokensFullDeltaResult,
) -> lsp_types::SemanticTokensFullDeltaResult {
    match result {
        domain::SemanticTokensFullDeltaResult::Tokens(tokens) => {
            lsp_types::SemanticTokensFullDeltaResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        domain::SemanticTokensFullDeltaResult::TokensDelta(delta) => {
            lsp_types::SemanticTokensFullDeltaResult::TokensDelta(to_lsp_semantic_tokens_delta(
                delta,
            ))
        }
        domain::SemanticTokensFullDeltaResult::PartialTokensDelta { edits } => {
            lsp_types::SemanticTokensFullDeltaResult::PartialTokensDelta {
                edits: edits.into_iter().map(to_lsp_semantic_tokens_edit).collect(),
            }
        }
    }
}

pub fn to_lsp_semantic_tokens_result(
    result: domain::SemanticTokensResult,
) -> lsp_types::SemanticTokensResult {
    match result {
        domain::SemanticTokensResult::Tokens(tokens) => {
            lsp_types::SemanticTokensResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        domain::SemanticTokensResult::Partial(partial) => {
            convert_via_json(domain::SemanticTokensResult::Partial(partial)).unwrap_or_else(|| {
                lsp_types::SemanticTokensResult::Tokens(lsp_types::SemanticTokens {
                    result_id: None,
                    data: vec![],
                })
            })
        }
    }
}

pub fn to_lsp_semantic_tokens_range_result(
    result: domain::SemanticTokensRangeResult,
) -> lsp_types::SemanticTokensRangeResult {
    match result {
        domain::SemanticTokensRangeResult::Tokens(tokens) => {
            lsp_types::SemanticTokensRangeResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        domain::SemanticTokensRangeResult::Partial(partial) => convert_via_json(
            domain::SemanticTokensRangeResult::Partial(partial),
        )
        .unwrap_or_else(|| {
            lsp_types::SemanticTokensRangeResult::Tokens(lsp_types::SemanticTokens {
                result_id: None,
                data: vec![],
            })
        }),
    }
}

pub fn to_lsp_definition_response(
    resp: domain::DefinitionResponse,
) -> Option<lsp_types::GotoDefinitionResponse> {
    let converted: lsp_types::GotoDefinitionResponse = convert_via_json(resp)?;
    match &converted {
        lsp_types::GotoDefinitionResponse::Array(locations) if locations.is_empty() => None,
        _ => Some(converted),
    }
}

pub fn to_lsp_workspace_edit(edit: domain::WorkspaceEdit) -> lsp_types::WorkspaceEdit {
    if let Some(converted) = convert_via_json(edit.clone()) {
        return converted;
    }

    let changes = edit.changes.map(|map| {
        map.into_iter()
            .filter_map(|(uri, edits)| {
                lsp_types::Url::parse(uri.as_str()).ok().map(|url| {
                    let edits = edits
                        .into_iter()
                        .map(|e: domain::TextEdit| lsp_types::TextEdit {
                            range: to_lsp_range(&e.range),
                            new_text: e.new_text,
                        })
                        .collect();
                    (url, edits)
                })
            })
            .collect::<HashMap<lsp_types::Url, Vec<lsp_types::TextEdit>>>()
    });

    lsp_types::WorkspaceEdit {
        changes,
        document_changes: None,
        change_annotations: None,
    }
}

pub fn to_lsp_code_action(action: domain::CodeAction) -> lsp_types::CodeAction {
    if let Some(converted) = convert_via_json(action.clone()) {
        return converted;
    }

    let domain::CodeAction {
        title,
        kind,
        diagnostics,
        edit,
        command,
        is_preferred,
        disabled,
        data,
    } = action;

    let kind = kind.map(|kind| lsp_types::CodeActionKind::from(kind.as_str().to_string()));
    let diagnostics = diagnostics.and_then(convert_via_json);
    let command = command.and_then(convert_via_json);

    lsp_types::CodeAction {
        title,
        kind,
        diagnostics,
        edit: edit.map(to_lsp_workspace_edit),
        command,
        is_preferred,
        disabled: disabled.map(|d| lsp_types::CodeActionDisabled { reason: d.reason }),
        data,
    }
}

pub fn to_lsp_code_action_response(
    actions: Vec<domain::CodeActionOrCommand>,
) -> Vec<lsp_types::CodeActionOrCommand> {
    actions
        .into_iter()
        .filter_map(|item| match item {
            domain::CodeActionOrCommand::CodeAction(action) => Some(
                lsp_types::CodeActionOrCommand::CodeAction(to_lsp_code_action(action)),
            ),
            domain::CodeActionOrCommand::Command(command) => {
                convert_via_json(command).map(lsp_types::CodeActionOrCommand::Command)
            }
        })
        .collect()
}
