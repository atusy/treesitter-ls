pub use lsp_types::{
    SemanticToken, SemanticTokens, SemanticTokensDelta, SemanticTokensEdit,
    SemanticTokensFullDeltaResult, SemanticTokensRangeResult, SemanticTokensResult,
};

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
