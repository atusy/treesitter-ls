use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct HighlightItem {
    #[serde(flatten)]
    pub source: HighlightSource,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum HighlightSource {
    Path { path: String },
    Query { query: String },
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LanguageConfig {
    pub library: Option<String>,
    pub filetypes: Vec<String>,
    pub highlight: Vec<HighlightItem>,
    pub locals: Option<Vec<HighlightItem>>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct TreeSitterSettings {
    pub runtimepath: Option<Vec<String>>,
    pub languages: std::collections::HashMap<String, LanguageConfig>,
}