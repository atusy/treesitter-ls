//! Parser metadata fetching from nvim-treesitter.
//!
//! This module fetches parser information dynamically from nvim-treesitter's
//! parsers.lua file, supporting all languages that nvim-treesitter supports (300+ languages).
//!
//! The main branch of nvim-treesitter uses a consolidated format where each language
//! entry contains url, revision, and location all in one place.

use regex::Regex;
use std::collections::HashMap;

/// URL for nvim-treesitter parsers.lua on GitHub (main branch).
const PARSERS_LUA_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/main/lua/nvim-treesitter/parsers.lua";

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

/// Fetch and parse parsers.lua to extract all parser information.
///
/// The main branch format is:
/// ```lua
/// return {
///   lua = {
///     install_info = {
///       revision = 'abc123...',
///       url = 'https://github.com/...',
///       location = 'optional/subdir',  -- optional
///     },
///     ...
///   },
/// }
/// ```
fn fetch_parsers_lua() -> Result<HashMap<String, ParserMetadata>, MetadataError> {
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
/// Handles the main branch format where languages are direct table keys.
fn parse_parsers_lua(content: &str) -> Result<HashMap<String, ParserMetadata>, MetadataError> {
    let mut parsers = HashMap::new();

    // Pattern to match parser entries in main branch format: lang = { ... }
    // The pattern matches language names at the start of a line (with optional indentation)
    // followed by = {
    let lang_pattern =
        Regex::new(r#"(?m)^\s*([a-zA-Z][a-zA-Z0-9_]*)\s*=\s*\{"#).expect("valid regex for lang pattern");

    // Find all language names first
    let lang_names: Vec<String> = lang_pattern
        .captures_iter(content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        // Filter out non-language keys like "return", "install_info", etc.
        .filter(|name| !is_reserved_key(name))
        .collect();

    // For each language, find its block and extract metadata
    for lang in lang_names {
        if let Some(info) = extract_parser_metadata(content, &lang) {
            parsers.insert(lang, info);
        }
    }

    Ok(parsers)
}

/// Check if a key is a reserved/internal key (not a language name)
fn is_reserved_key(name: &str) -> bool {
    matches!(
        name,
        "return"
            | "install_info"
            | "maintainers"
            | "requires"
            | "tier"
            | "readme_note"
            | "experimental"
            | "filetype"
    )
}

/// Extract parser metadata for a specific language from parsers.lua content.
fn extract_parser_metadata(content: &str, language: &str) -> Option<ParserMetadata> {
    // Find the start of this language's block
    // Use word boundary to avoid matching substrings (e.g., "c" matching "cpp")
    let block_start_pattern = format!(r#"(?m)^\s*{}\s*=\s*\{{"#, regex::escape(language));
    let block_start_re = Regex::new(&block_start_pattern).ok()?;

    let block_start = block_start_re.find(content)?;
    let start_pos = block_start.start();

    // Find the end of this block by counting braces
    let block_content = find_matching_brace(&content[start_pos..])?;

    // Extract URL (required)
    let url_re = Regex::new(r#"url\s*=\s*'([^']+)'"#).ok()?;
    let url = url_re
        .captures(block_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())?;

    // Extract revision (required) - in main branch, revision is inside install_info
    let revision_re = Regex::new(r#"revision\s*=\s*'([^']+)'"#).ok()?;
    let revision = revision_re
        .captures(block_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())?;

    // Extract location (optional)
    let location_re = Regex::new(r#"location\s*=\s*'([^']+)'"#).ok()?;
    let location = location_re
        .captures(block_content)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string());

    Some(ParserMetadata {
        url,
        revision,
        location,
    })
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
/// This fetches parsers.lua which contains url, revision, and location
/// all in one place (main branch format).
pub fn fetch_parser_metadata(language: &str) -> Result<ParserMetadata, MetadataError> {
    let parsers = fetch_parsers_lua()?;

    parsers
        .get(language)
        .cloned()
        .ok_or_else(|| MetadataError::LanguageNotFound(language.to_string()))
}

/// List all supported languages by fetching from nvim-treesitter.
///
/// This returns all languages that nvim-treesitter supports (300+ languages).
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
    fn test_parse_parsers_lua_main_branch_format() {
        // Test the main branch format (no "list." prefix, single quotes, revision inside)
        let content = r#"
return {
  lua = {
    install_info = {
      revision = 'abc123def456',
      url = 'https://github.com/MunifTanjim/tree-sitter-lua',
    },
    maintainers = { '@someone' },
    tier = 2,
  },
  typescript = {
    install_info = {
      location = 'typescript',
      revision = 'def789ghi012',
      url = 'https://github.com/tree-sitter/tree-sitter-typescript',
    },
    maintainers = { '@someone' },
    tier = 2,
  },
}
"#;

        let result = parse_parsers_lua(content).expect("should parse");

        assert!(result.contains_key("lua"));
        let lua = result.get("lua").unwrap();
        assert_eq!(lua.url, "https://github.com/MunifTanjim/tree-sitter-lua");
        assert_eq!(lua.revision, "abc123def456");
        assert!(lua.location.is_none());

        assert!(result.contains_key("typescript"));
        let ts = result.get("typescript").unwrap();
        assert_eq!(
            ts.url,
            "https://github.com/tree-sitter/tree-sitter-typescript"
        );
        assert_eq!(ts.revision, "def789ghi012");
        assert_eq!(ts.location, Some("typescript".to_string()));
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
    fn test_extract_parser_metadata() {
        let content = r#"
  rust = {
    install_info = {
      revision = 'abc123',
      url = 'https://github.com/tree-sitter/tree-sitter-rust',
    },
    tier = 1,
  },
"#;

        let info = extract_parser_metadata(content, "rust").expect("should extract");
        assert_eq!(info.url, "https://github.com/tree-sitter/tree-sitter-rust");
        assert_eq!(info.revision, "abc123");
        assert!(info.location.is_none());
    }

    #[test]
    fn test_extract_parser_metadata_with_location() {
        let content = r#"
  markdown = {
    install_info = {
      location = 'tree-sitter-markdown',
      revision = 'xyz789',
      url = 'https://github.com/tree-sitter-grammars/tree-sitter-markdown',
    },
    tier = 2,
  },
"#;

        let info = extract_parser_metadata(content, "markdown").expect("should extract");
        assert_eq!(
            info.url,
            "https://github.com/tree-sitter-grammars/tree-sitter-markdown"
        );
        assert_eq!(info.revision, "xyz789");
        assert_eq!(info.location, Some("tree-sitter-markdown".to_string()));
    }

    #[test]
    fn test_is_reserved_key() {
        assert!(is_reserved_key("return"));
        assert!(is_reserved_key("install_info"));
        assert!(is_reserved_key("maintainers"));
        assert!(!is_reserved_key("lua"));
        assert!(!is_reserved_key("rust"));
    }
}
