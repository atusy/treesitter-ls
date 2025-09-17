pub mod semantic;
pub mod settings;

pub use lsp_types::{
    CodeAction, CodeActionDisabled, CodeActionKind, CodeActionOrCommand, Location, Position, Range,
    SelectionRange, TextEdit, Uri, WorkspaceEdit,
};

pub type DefinitionResponse = lsp_types::GotoDefinitionResponse;

pub use semantic::{
    LEGEND_MODIFIERS, LEGEND_TYPES, SemanticToken, SemanticTokens, SemanticTokensDelta,
    SemanticTokensEdit, SemanticTokensFullDeltaResult, SemanticTokensRangeResult,
    SemanticTokensResult,
};
