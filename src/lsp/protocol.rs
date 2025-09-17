use std::collections::HashMap;

use crate::domain::{
    CodeAction as DomainCodeAction, CodeActionOrCommand as DomainCodeActionOrCommand,
    DefinitionResponse, Position as DomainPosition, Range as DomainRange,
    SelectionRange as DomainSelectionRange, SemanticToken as DomainSemanticToken,
    SemanticTokens as DomainSemanticTokens, SemanticTokensDelta as DomainSemanticTokensDelta,
    SemanticTokensEdit as DomainSemanticTokensEdit,
    SemanticTokensFullDeltaResult as DomainSemanticTokensFullDeltaResult,
    SemanticTokensRangeResult as DomainSemanticTokensRangeResult,
    SemanticTokensResult as DomainSemanticTokensResult, WorkspaceEdit as DomainWorkspaceEdit,
};
use tower_lsp::lsp_types::{
    CodeAction as LspCodeAction, CodeActionDisabled as LspCodeActionDisabled, CodeActionKind,
    CodeActionOrCommand as LspCodeActionOrCommand, GotoDefinitionResponse, Location as LspLocation,
    Position as LspPosition, Range as LspRange, SelectionRange as LspSelectionRange,
    SemanticToken as LspSemanticToken, SemanticTokens as LspSemanticTokens,
    SemanticTokensDelta as LspSemanticTokensDelta, SemanticTokensEdit as LspSemanticTokensEdit,
    SemanticTokensFullDeltaResult as LspSemanticTokensFullDeltaResult,
    SemanticTokensRangeResult as LspSemanticTokensRangeResult,
    SemanticTokensResult as LspSemanticTokensResult, TextEdit as LspTextEdit, Url,
    WorkspaceEdit as LspWorkspaceEdit,
};

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
        data: Some(edit.data.into_iter().map(to_lsp_semantic_token).collect()),
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
        DomainSemanticTokensFullDeltaResult::Delta(delta) => {
            LspSemanticTokensFullDeltaResult::TokensDelta(to_lsp_semantic_tokens_delta(delta))
        }
        DomainSemanticTokensFullDeltaResult::NoChange => {
            LspSemanticTokensFullDeltaResult::TokensDelta(LspSemanticTokensDelta {
                result_id: None,
                edits: vec![],
            })
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
    }
}

pub fn to_lsp_semantic_tokens_range_result(
    result: DomainSemanticTokensRangeResult,
) -> LspSemanticTokensRangeResult {
    match result {
        DomainSemanticTokensRangeResult::Tokens(tokens) => {
            LspSemanticTokensRangeResult::Tokens(to_lsp_semantic_tokens(tokens))
        }
    }
}

pub fn to_lsp_definition_response(resp: DefinitionResponse) -> Option<GotoDefinitionResponse> {
    match resp {
        DefinitionResponse::Locations(locations) => {
            let converted: Vec<LspLocation> = locations
                .into_iter()
                .map(|loc| LspLocation {
                    uri: loc.uri,
                    range: to_lsp_range(&loc.range),
                })
                .collect();
            if converted.is_empty() {
                None
            } else {
                Some(GotoDefinitionResponse::Array(converted))
            }
        }
    }
}

pub fn to_lsp_workspace_edit(edit: DomainWorkspaceEdit) -> LspWorkspaceEdit {
    let mut changes: HashMap<Url, Vec<LspTextEdit>> = HashMap::new();
    for doc_edit in edit.document_changes {
        let edits = doc_edit
            .edits
            .into_iter()
            .map(|e| LspTextEdit {
                range: to_lsp_range(&e.range),
                new_text: e.new_text,
            })
            .collect();
        changes.insert(doc_edit.uri, edits);
    }

    LspWorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

pub fn to_lsp_code_action(action: DomainCodeAction) -> LspCodeAction {
    LspCodeAction {
        title: action.title,
        kind: action.kind.map(CodeActionKind::from),
        diagnostics: None,
        edit: action.edit.map(to_lsp_workspace_edit),
        command: None,
        is_preferred: None,
        disabled: action
            .disabled
            .map(|d| LspCodeActionDisabled { reason: d.reason }),
        data: None,
    }
}

pub fn to_lsp_code_action_response(
    actions: Vec<DomainCodeActionOrCommand>,
) -> Vec<LspCodeActionOrCommand> {
    actions
        .into_iter()
        .map(|item| match item {
            DomainCodeActionOrCommand::CodeAction(action) => {
                LspCodeActionOrCommand::CodeAction(to_lsp_code_action(action))
            }
        })
        .collect()
}
