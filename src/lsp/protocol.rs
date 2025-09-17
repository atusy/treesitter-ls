use std::collections::HashMap;

use serde::de::DeserializeOwned;

use crate::domain::{
    CodeAction as DomainCodeAction, CodeActionOrCommand as DomainCodeActionOrCommand,
    DefinitionResponse as DomainDefinitionResponse, Position as DomainPosition,
    Range as DomainRange, SelectionRange as DomainSelectionRange,
    SemanticToken as DomainSemanticToken, SemanticTokens as DomainSemanticTokens,
    SemanticTokensDelta as DomainSemanticTokensDelta,
    SemanticTokensEdit as DomainSemanticTokensEdit,
    SemanticTokensFullDeltaResult as DomainSemanticTokensFullDeltaResult,
    SemanticTokensRangeResult as DomainSemanticTokensRangeResult,
    SemanticTokensResult as DomainSemanticTokensResult, TextEdit as DomainTextEdit,
    WorkspaceEdit as DomainWorkspaceEdit,
};
use tower_lsp::lsp_types::{
    CodeAction as LspCodeAction, CodeActionDisabled as LspCodeActionDisabled, CodeActionKind,
    CodeActionOrCommand as LspCodeActionOrCommand,
    GotoDefinitionResponse as LspGotoDefinitionResponse, Position as LspPosition,
    Range as LspRange, SelectionRange as LspSelectionRange, SemanticToken as LspSemanticToken,
    SemanticTokens as LspSemanticTokens, SemanticTokensDelta as LspSemanticTokensDelta,
    SemanticTokensEdit as LspSemanticTokensEdit,
    SemanticTokensFullDeltaResult as LspSemanticTokensFullDeltaResult,
    SemanticTokensRangeResult as LspSemanticTokensRangeResult,
    SemanticTokensResult as LspSemanticTokensResult, TextEdit as LspTextEdit, Url,
    WorkspaceEdit as LspWorkspaceEdit,
};

fn convert_via_json<T, U>(value: T) -> Option<U>
where
    T: serde::Serialize,
    U: DeserializeOwned,
{
    serde_json::to_value(value)
        .ok()
        .and_then(|json| serde_json::from_value(json).ok())
}

pub fn to_domain_position(pos: &LspPosition) -> DomainPosition {
    DomainPosition::new(pos.line, pos.character)
}

pub fn to_domain_range(range: &LspRange) -> DomainRange {
    DomainRange::new(
        to_domain_position(&range.start),
        to_domain_position(&range.end),
    )
}

pub fn to_lsp_position(pos: &DomainPosition) -> LspPosition {
    LspPosition::new(pos.line, pos.character)
}

pub fn to_lsp_range(range: &DomainRange) -> LspRange {
    LspRange::new(to_lsp_position(&range.start), to_lsp_position(&range.end))
}

pub fn to_lsp_selection_range(range: &DomainSelectionRange) -> LspSelectionRange {
    LspSelectionRange {
        range: to_lsp_range(&range.range),
        parent: range
            .parent
            .as_ref()
            .map(|parent| Box::new(to_lsp_selection_range(parent))),
    }
}

pub fn to_lsp_semantic_token(token: DomainSemanticToken) -> LspSemanticToken {
    LspSemanticToken {
        delta_line: token.delta_line,
        delta_start: token.delta_start,
        length: token.length,
        token_type: token.token_type,
        token_modifiers_bitset: token.token_modifiers_bitset,
    }
}

pub fn to_lsp_semantic_tokens(tokens: DomainSemanticTokens) -> LspSemanticTokens {
    LspSemanticTokens {
        result_id: tokens.result_id,
        data: tokens.data.into_iter().map(to_lsp_semantic_token).collect(),
    }
}

pub fn to_lsp_semantic_tokens_edit(edit: DomainSemanticTokensEdit) -> LspSemanticTokensEdit {
    LspSemanticTokensEdit {
        start: edit.start,
        delete_count: edit.delete_count,
        data: edit
            .data
            .map(|tokens| tokens.into_iter().map(to_lsp_semantic_token).collect()),
    }
}

pub fn to_lsp_semantic_tokens_delta(delta: DomainSemanticTokensDelta) -> LspSemanticTokensDelta {
    LspSemanticTokensDelta {
        result_id: delta.result_id,
        edits: delta
            .edits
            .into_iter()
            .map(to_lsp_semantic_tokens_edit)
            .collect(),
    }
}

pub fn to_lsp_semantic_tokens_full_delta(
    result: DomainSemanticTokensFullDeltaResult,
) -> LspSemanticTokensFullDeltaResult {
    match result {
        DomainSemanticTokensFullDeltaResult::Tokens(tokens) => {
            LspSemanticTokensFullDeltaResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        DomainSemanticTokensFullDeltaResult::TokensDelta(delta) => {
            LspSemanticTokensFullDeltaResult::TokensDelta(to_lsp_semantic_tokens_delta(delta))
        }
        DomainSemanticTokensFullDeltaResult::PartialTokensDelta { edits } => {
            LspSemanticTokensFullDeltaResult::PartialTokensDelta {
                edits: edits.into_iter().map(to_lsp_semantic_tokens_edit).collect(),
            }
        }
    }
}

pub fn to_lsp_semantic_tokens_result(
    result: DomainSemanticTokensResult,
) -> LspSemanticTokensResult {
    match result {
        DomainSemanticTokensResult::Tokens(tokens) => {
            LspSemanticTokensResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        DomainSemanticTokensResult::Partial(partial) => {
            convert_via_json(DomainSemanticTokensResult::Partial(partial)).unwrap_or_else(|| {
                LspSemanticTokensResult::Tokens(LspSemanticTokens {
                    result_id: None,
                    data: vec![],
                })
            })
        }
    }
}

pub fn to_lsp_semantic_tokens_range_result(
    result: DomainSemanticTokensRangeResult,
) -> LspSemanticTokensRangeResult {
    match result {
        DomainSemanticTokensRangeResult::Tokens(tokens) => {
            LspSemanticTokensRangeResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
        DomainSemanticTokensRangeResult::Partial(partial) => convert_via_json(
            DomainSemanticTokensRangeResult::Partial(partial),
        )
        .unwrap_or_else(|| {
            LspSemanticTokensRangeResult::Tokens(LspSemanticTokens {
                result_id: None,
                data: vec![],
            })
        }),
    }
}

pub fn to_lsp_definition_response(
    resp: DomainDefinitionResponse,
) -> Option<LspGotoDefinitionResponse> {
    let converted: LspGotoDefinitionResponse = convert_via_json(resp)?;
    match &converted {
        LspGotoDefinitionResponse::Array(locations) if locations.is_empty() => None,
        _ => Some(converted),
    }
}

pub fn to_lsp_workspace_edit(edit: DomainWorkspaceEdit) -> LspWorkspaceEdit {
    if let Some(converted) = convert_via_json(edit.clone()) {
        return converted;
    }

    let changes = edit.changes.map(|map| {
        map.into_iter()
            .filter_map(|(uri, edits)| {
                Url::parse(uri.as_str()).ok().map(|url| {
                    let edits = edits
                        .into_iter()
                        .map(|e: DomainTextEdit| LspTextEdit {
                            range: to_lsp_range(&e.range),
                            new_text: e.new_text,
                        })
                        .collect();
                    (url, edits)
                })
            })
            .collect::<HashMap<Url, Vec<LspTextEdit>>>()
    });

    LspWorkspaceEdit {
        changes,
        document_changes: None,
        change_annotations: None,
    }
}

pub fn to_lsp_code_action(action: DomainCodeAction) -> LspCodeAction {
    if let Some(converted) = convert_via_json(action.clone()) {
        return converted;
    }

    let DomainCodeAction {
        title,
        kind,
        diagnostics,
        edit,
        command,
        is_preferred,
        disabled,
        data,
    } = action;

    let kind = kind.map(|kind| CodeActionKind::from(kind.as_str().to_string()));
    let diagnostics = diagnostics.and_then(|value| convert_via_json(value));
    let command = command.and_then(|value| convert_via_json(value));

    LspCodeAction {
        title,
        kind,
        diagnostics,
        edit: edit.map(to_lsp_workspace_edit),
        command,
        is_preferred,
        disabled: disabled.map(|d| LspCodeActionDisabled { reason: d.reason }),
        data,
    }
}

pub fn to_lsp_code_action_response(
    actions: Vec<DomainCodeActionOrCommand>,
) -> Vec<LspCodeActionOrCommand> {
    actions
        .into_iter()
        .filter_map(|item| match item {
            DomainCodeActionOrCommand::CodeAction(action) => Some(
                LspCodeActionOrCommand::CodeAction(to_lsp_code_action(action)),
            ),
            DomainCodeActionOrCommand::Command(command) => {
                convert_via_json(command).map(LspCodeActionOrCommand::Command)
            }
        })
        .collect()
}
