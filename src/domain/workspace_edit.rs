use crate::domain::position::Range;
use url::Url;

#[derive(Clone, Debug, PartialEq)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentEdits {
    pub uri: Url,
    pub edits: Vec<TextEdit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkspaceEdit {
    pub document_changes: Vec<DocumentEdits>,
}
