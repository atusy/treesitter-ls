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
