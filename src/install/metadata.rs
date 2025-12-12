//! Parser metadata fetching from nvim-treesitter.
//!
//! This module fetches parser information dynamically from nvim-treesitter's
//! parsers.lua and lockfile.json files, supporting all languages that
//! nvim-treesitter supports (200+ languages).

use regex::Regex;
use std::collections::HashMap;

/// URL for nvim-treesitter lockfile.json on GitHub.
const LOCKFILE_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/lockfile.json";

/// URL for nvim-treesitter parsers.lua on GitHub.
const PARSERS_LUA_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/master/lua/nvim-treesitter/parsers.lua";

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

/// Parsed parser information from parsers.lua
#[derive(Debug, Clone)]
struct ParsedParserInfo {
    url: String,
    location: Option<String>,
}

/// Fetch and parse parsers.lua to extract URL and location information.
///
/// This parses the Lua file using regex patterns to extract:
/// - `url = "..."` - the repository URL
/// - `location = "..."` - optional subdirectory for monorepos
fn fetch_parsers_lua() -> Result<HashMap<String, ParsedParserInfo>, MetadataError> {
    let response = reqwest::blocking::get(PARSERS_LUA_URL)
        .map_err(|e| MetadataError::HttpError(e.to_string()))?;

    if !response.status().is_success() {
        return Err(MetadataError::HttpError(format!(
            "HTTP {} fetching parsers.lua",
            response.status()
        )));
    }

    let content = response
        .text()
        .map_err(|e| MetadataError::HttpError(e.to_string()))?;

    parse_parsers_lua(&content)
}

/// Parse the parsers.lua content to extract parser information.
///
/// The file has entries like:
/// ```lua
/// list.lua = {
///   install_info = {
///     url = "https://github.com/MunifTanjim/tree-sitter-lua",
///     files = { "src/parser.c", "src/scanner.c" },
///   },
///   ...
/// }
/// ```
fn parse_parsers_lua(content: &str) -> Result<HashMap<String, ParsedParserInfo>, MetadataError> {
    let mut parsers = HashMap::new();

    // Pattern to match parser entries: list.LANG = { ... }
    // We need to find each language block and extract url and location
    let lang_pattern =
        Regex::new(r#"list\.([a-zA-Z0-9_]+)\s*=\s*\{"#).expect("valid regex for lang pattern");

    // Find all language names first
    let lang_names: Vec<String> = lang_pattern
        .captures_iter(content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();

    // For each language, find its block and extract url/location
    for lang in lang_names {
        if let Some(info) = extract_parser_info(content, &lang) {
            parsers.insert(lang, info);
        }
    }

    Ok(parsers)
}

/// Extract parser info (url, location) for a specific language from parsers.lua content.
fn extract_parser_info(content: &str, language: &str) -> Option<ParsedParserInfo> {
    // Find the start of this language's block
    let block_start_pattern = format!(r#"list\.{}\s*=\s*\{{"#, regex::escape(language));
    let block_start_re = Regex::new(&block_start_pattern).ok()?;

    let block_start = block_start_re.find(content)?;
    let start_pos = block_start.start();

    // Find the end of this block by counting braces
    let block_content = find_matching_brace(&content[start_pos..])?;

    // Extract URL
    let url_re = Regex::new(r#"url\s*=\s*"([^"]+)""#).ok()?;
    let url = url_re
        .captures(block_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())?;

    // Extract location (optional)
    let location_re = Regex::new(r#"location\s*=\s*"([^"]+)""#).ok()?;
    let location = location_re
        .captures(block_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string());

    Some(ParsedParserInfo { url, location })
}

/// Find the content within matching braces starting from the first `{`.
fn find_matching_brace(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let mut depth = 0;
    let mut end = start;

    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    if depth == 0 {
        Some(&s[start..end])
    } else {
        None
    }
}

/// Fetch parser metadata for a language from nvim-treesitter.
///
/// This fetches both parsers.lua (for URL and location) and lockfile.json
/// (for revision), supporting all languages that nvim-treesitter supports.
pub fn fetch_parser_metadata(language: &str) -> Result<ParserMetadata, MetadataError> {
    // Fetch parsers.lua to get URL and location
    let parsers = fetch_parsers_lua()?;

    let parser_info = parsers
        .get(language)
        .ok_or_else(|| MetadataError::LanguageNotFound(language.to_string()))?;

    // Fetch the lockfile for revision
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
        url: parser_info.url.clone(),
        revision: revision.to_string(),
        location: parser_info.location.clone(),
    })
}

/// List all supported languages by fetching from nvim-treesitter.
///
/// This returns all languages that nvim-treesitter supports (200+ languages).
pub fn list_supported_languages() -> Result<Vec<String>, MetadataError> {
    let parsers = fetch_parsers_lua()?;
    let mut languages: Vec<String> = parsers.keys().cloned().collect();
    languages.sort();
    Ok(languages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_parsers_lua_basic() {
        let content = r#"
list.lua = {
  install_info = {
    url = "https://github.com/MunifTanjim/tree-sitter-lua",
    files = { "src/parser.c", "src/scanner.c" },
  },
}

list.typescript = {
  install_info = {
    url = "https://github.com/tree-sitter/tree-sitter-typescript",
    location = "typescript",
    files = { "src/parser.c", "src/scanner.c" },
  },
}
"#;

        let result = parse_parsers_lua(content).expect("should parse");

        assert!(result.contains_key("lua"));
        assert_eq!(
            result.get("lua").unwrap().url,
            "https://github.com/MunifTanjim/tree-sitter-lua"
        );
        assert!(result.get("lua").unwrap().location.is_none());

        assert!(result.contains_key("typescript"));
        assert_eq!(
            result.get("typescript").unwrap().url,
            "https://github.com/tree-sitter/tree-sitter-typescript"
        );
        assert_eq!(
            result.get("typescript").unwrap().location,
            Some("typescript".to_string())
        );
    }

    #[test]
    fn test_find_matching_brace() {
        let s = "{ foo { bar } baz }";
        let result = find_matching_brace(s);
        assert_eq!(result, Some("{ foo { bar } baz }"));

        let s2 = "prefix { inner } suffix";
        let result2 = find_matching_brace(s2);
        assert_eq!(result2, Some("{ inner }"));
    }

    #[test]
    fn test_extract_parser_info() {
        let content = r#"
list.rust = {
  install_info = {
    url = "https://github.com/tree-sitter/tree-sitter-rust",
    files = { "src/parser.c", "src/scanner.c" },
  },
}
"#;

        let info = extract_parser_info(content, "rust").expect("should extract");
        assert_eq!(info.url, "https://github.com/tree-sitter/tree-sitter-rust");
        assert!(info.location.is_none());
    }

    #[test]
    fn test_extract_parser_info_with_location() {
        let content = r#"
list.markdown = {
  install_info = {
    url = "https://github.com/tree-sitter-grammars/tree-sitter-markdown",
    location = "tree-sitter-markdown",
    files = { "src/parser.c" },
  },
}
"#;

        let info = extract_parser_info(content, "markdown").expect("should extract");
        assert_eq!(
            info.url,
            "https://github.com/tree-sitter-grammars/tree-sitter-markdown"
        );
        assert_eq!(info.location, Some("tree-sitter-markdown".to_string()));
    }
}
