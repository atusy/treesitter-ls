use crate::domain::workspace_edit::WorkspaceEdit;

#[derive(Clone, Debug, PartialEq)]
pub struct CodeActionDisabled {
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CodeAction {
    pub title: String,
    pub kind: Option<String>,
    pub edit: Option<WorkspaceEdit>,
    pub disabled: Option<CodeActionDisabled>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CodeActionOrCommand {
    CodeAction(CodeAction),
}
