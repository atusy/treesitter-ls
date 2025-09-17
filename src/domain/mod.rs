pub mod settings;

use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectionRange {
    pub range: Range,
    pub parent: Option<Box<SelectionRange>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticToken {
    pub delta_line: u32,
    pub delta_start: u32,
    pub length: u32,
    pub token_type: u32,
    pub token_modifiers_bitset: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTokens {
    pub result_id: Option<String>,
    pub data: Vec<SemanticToken>,
}

impl SemanticTokens {
    pub fn new(result_id: Option<String>, data: Vec<SemanticToken>) -> Self {
        Self { result_id, data }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTokensEdit {
    pub start: u32,
    pub delete_count: u32,
    pub data: Vec<SemanticToken>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticTokensDelta {
    pub result_id: Option<String>,
    pub edits: Vec<SemanticTokensEdit>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticTokensFullDeltaResult {
    Tokens(SemanticTokens),
    Delta(SemanticTokensDelta),
    NoChange,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticTokensResult {
    Tokens(SemanticTokens),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticTokensRangeResult {
    Tokens(SemanticTokens),
}

pub const LEGEND_TYPES: &[&str] = &[
    "comment",
    "keyword",
    "string",
    "number",
    "regexp",
    "operator",
    "namespace",
    "type",
    "struct",
    "class",
    "interface",
    "enum",
    "enumMember",
    "typeParameter",
    "function",
    "method",
    "macro",
    "variable",
    "parameter",
    "property",
    "event",
    "modifier",
    "decorator",
];

pub const LEGEND_MODIFIERS: &[&str] = &[
    "declaration",
    "definition",
    "readonly",
    "static",
    "deprecated",
    "abstract",
    "async",
    "modification",
    "documentation",
    "defaultLibrary",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Location {
    pub uri: Url,
    pub range: Range,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DefinitionResponse {
    Locations(Vec<Location>),
}

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
