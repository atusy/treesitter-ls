use std::collections::HashMap;

/// Query source definitions used across the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuerySource {
    Path(String),
    Inline(String),
}

impl QuerySource {
    pub fn path<P: Into<String>>(path: P) -> Self {
        Self::Path(path.into())
    }

    pub fn inline<Q: Into<String>>(query: Q) -> Self {
        Self::Inline(query.into())
    }
}

/// Per-language Tree-sitter language configuration surfaced to the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSettings {
    pub library: Option<String>,
    pub filetypes: Vec<String>,
    pub highlight: Vec<QuerySource>,
    pub locals: Option<Vec<QuerySource>>,
}

impl LanguageSettings {
    pub fn new(
        library: Option<String>,
        filetypes: Vec<String>,
        highlight: Vec<QuerySource>,
        locals: Option<Vec<QuerySource>>,
    ) -> Self {
        Self {
            library,
            filetypes,
            highlight,
            locals,
        }
    }
}

/// Capture mapping for a particular query type.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryTypeMappings {
    pub highlights: HashMap<String, String>,
    pub locals: HashMap<String, String>,
    pub folds: HashMap<String, String>,
}

pub type CaptureMappings = HashMap<String, QueryTypeMappings>;

/// Workspace-wide Tree-sitter configuration as required by the domain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSettings {
    pub search_paths: Vec<String>,
    pub languages: HashMap<String, LanguageSettings>,
    pub capture_mappings: CaptureMappings,
}

impl WorkspaceSettings {
    pub fn new(
        search_paths: Vec<String>,
        languages: HashMap<String, LanguageSettings>,
        capture_mappings: CaptureMappings,
    ) -> Self {
        Self {
            search_paths,
            languages,
            capture_mappings,
        }
    }
}
