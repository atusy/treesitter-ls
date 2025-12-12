//! Parser metadata fetching from nvim-treesitter.
//!
//! This module fetches parser revision information from nvim-treesitter's lockfile
//! and provides repository URLs for common languages.

use std::collections::HashMap;

/// URL for nvim-treesitter lockfile.json on GitHub.
const LOCKFILE_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/lockfile.json";

/// Parser metadata containing repository URL and revision.
#[derive(Debug, Clone)]
pub struct ParserMetadata {
    /// Git repository URL for the parser.
    pub url: String,
    /// Git revision (commit hash or tag).
    pub revision: String,
    /// Optional subdirectory within the repository (for monorepos).
    pub location: Option<String>,
}

/// Error types for metadata operations.
#[derive(Debug)]
pub enum MetadataError {
    /// Language not found in metadata.
    LanguageNotFound(String),
    /// HTTP request failed.
    HttpError(String),
    /// JSON parsing failed.
    ParseError(String),
}

impl std::fmt::Display for MetadataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LanguageNotFound(lang) => {
                write!(
                    f,
                    "Language '{}' not found in nvim-treesitter metadata",
                    lang
                )
            }
            Self::HttpError(msg) => write!(f, "HTTP error: {}", msg),
            Self::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for MetadataError {}

/// Get the repository URL for a language.
///
/// This uses a hardcoded mapping for common languages since parsing parsers.lua
/// would require a Lua parser. The mapping covers the most popular languages.
pub fn get_parser_url(language: &str) -> Option<&'static str> {
    // Mapping of common languages to their parser repository URLs.
    // These are the most commonly used parsers from nvim-treesitter.
    let urls: HashMap<&str, &str> = [
        ("bash", "https://github.com/tree-sitter/tree-sitter-bash"),
        ("c", "https://github.com/tree-sitter/tree-sitter-c"),
        ("cpp", "https://github.com/tree-sitter/tree-sitter-cpp"),
        ("css", "https://github.com/tree-sitter/tree-sitter-css"),
        ("go", "https://github.com/tree-sitter/tree-sitter-go"),
        ("html", "https://github.com/tree-sitter/tree-sitter-html"),
        ("java", "https://github.com/tree-sitter/tree-sitter-java"),
        (
            "javascript",
            "https://github.com/tree-sitter/tree-sitter-javascript",
        ),
        ("json", "https://github.com/tree-sitter/tree-sitter-json"),
        ("lua", "https://github.com/MunifTanjim/tree-sitter-lua"),
        (
            "markdown",
            "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
        ),
        (
            "markdown_inline",
            "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
        ),
        (
            "python",
            "https://github.com/tree-sitter/tree-sitter-python",
        ),
        ("ruby", "https://github.com/tree-sitter/tree-sitter-ruby"),
        ("rust", "https://github.com/tree-sitter/tree-sitter-rust"),
        (
            "toml",
            "https://github.com/tree-sitter-grammars/tree-sitter-toml",
        ),
        (
            "tsx",
            "https://github.com/tree-sitter/tree-sitter-typescript",
        ),
        (
            "typescript",
            "https://github.com/tree-sitter/tree-sitter-typescript",
        ),
        (
            "yaml",
            "https://github.com/tree-sitter-grammars/tree-sitter-yaml",
        ),
    ]
    .into_iter()
    .collect();

    urls.get(language).copied()
}

/// Get the subdirectory location for languages in monorepos.
pub fn get_parser_location(language: &str) -> Option<&'static str> {
    match language {
        "tsx" => Some("tsx"),
        "typescript" => Some("typescript"),
        "markdown" => Some("tree-sitter-markdown"),
        "markdown_inline" => Some("tree-sitter-markdown-inline"),
        _ => None,
    }
}

/// Fetch parser metadata for a language from nvim-treesitter.
///
/// This fetches the revision from lockfile.json and combines it with
/// the hardcoded repository URL.
pub fn fetch_parser_metadata(language: &str) -> Result<ParserMetadata, MetadataError> {
    // Get the repository URL
    let url = get_parser_url(language)
        .ok_or_else(|| MetadataError::LanguageNotFound(language.to_string()))?;

    // Fetch the lockfile
    let response = reqwest::blocking::get(LOCKFILE_URL)
        .map_err(|e| MetadataError::HttpError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(MetadataError::HttpError(format!(
            "HTTP {} fetching lockfile",
            response.status()
        )));
    }

    let lockfile_text = response
        .text()
        .map_err(|e| MetadataError::HttpError(e.to_string()))?;

    // Parse the lockfile JSON
    let lockfile: HashMap<String, serde_json::Value> = serde_json::from_str(&lockfile_text)
        .map_err(|e| MetadataError::ParseError(e.to_string()))?;

    // Get the revision for this language
    let revision = lockfile
        .get(language)
        .and_then(|v| v.get("revision"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| MetadataError::LanguageNotFound(language.to_string()))?;

    Ok(ParserMetadata {
        url: url.to_string(),
        revision: revision.to_string(),
        location: get_parser_location(language).map(String::from),
    })
}

/// List all supported languages.
pub fn list_supported_languages() -> Vec<&'static str> {
    vec![
        "bash",
        "c",
        "cpp",
        "css",
        "go",
        "html",
        "java",
        "javascript",
        "json",
        "lua",
        "markdown",
        "markdown_inline",
        "python",
        "ruby",
        "rust",
        "toml",
        "tsx",
        "typescript",
        "yaml",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_parser_url_returns_url_for_known_language() {
        assert!(get_parser_url("lua").is_some());
        assert!(get_parser_url("rust").is_some());
        assert!(get_parser_url("python").is_some());
    }

    #[test]
    fn test_get_parser_url_returns_none_for_unknown_language() {
        assert!(get_parser_url("nonexistent_language_xyz").is_none());
    }

    #[test]
    fn test_get_parser_location_for_monorepos() {
        assert_eq!(get_parser_location("typescript"), Some("typescript"));
        assert_eq!(get_parser_location("tsx"), Some("tsx"));
        assert_eq!(get_parser_location("lua"), None);
    }

    #[test]
    fn test_list_supported_languages_not_empty() {
        let languages = list_supported_languages();
        assert!(!languages.is_empty());
        assert!(languages.contains(&"lua"));
    }
}
