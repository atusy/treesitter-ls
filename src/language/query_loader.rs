use crate::config::{HighlightItem, HighlightSource};
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Language, Query};

/// Loads Tree-sitter queries from files and configuration
pub struct QueryLoader;

impl QueryLoader {
    /// Load query content from highlight items
    pub fn load_query_from_highlight(highlight_items: &[HighlightItem]) -> Result<String, String> {
        let mut combined_query = String::new();

        for item in highlight_items {
            match &item.source {
                HighlightSource::Path { path } => match fs::read_to_string(path) {
                    Ok(content) => {
                        combined_query.push_str(&content);
                        combined_query.push('\n');
                    }
                    Err(e) => {
                        return Err(format!("Failed to read query file {path}: {e}"));
                    }
                },
                HighlightSource::Query { query } => {
                    combined_query.push_str(query);
                    combined_query.push('\n');
                }
            }
        }

        Ok(combined_query)
    }

    /// Find a query file in search paths
    pub fn find_query_file(
        runtime_bases: &[String],
        lang_name: &str,
        file_name: &str,
    ) -> Option<PathBuf> {
        for base in runtime_bases {
            let candidate = Path::new(base)
                .join("queries")
                .join(lang_name)
                .join(file_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    /// Load a query file from search paths
    pub fn load_query_file(
        runtime_bases: &[String],
        lang_name: &str,
        file_name: &str,
    ) -> Result<String, String> {
        match Self::find_query_file(runtime_bases, lang_name, file_name) {
            Some(path) => fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read query file {}: {}", path.display(), e)),
            None => Err(format!(
                "Query file {} not found for language {} in search paths",
                file_name, lang_name
            )),
        }
    }

    /// Parse a query string into a Tree-sitter Query
    pub fn parse_query(language: &Language, query_str: &str) -> Result<Query, String> {
        Query::new(language, query_str).map_err(|e| format!("Failed to parse query: {e}"))
    }

    /// Load and parse a highlight query
    pub fn load_highlight_query(
        language: &Language,
        highlight_items: &[HighlightItem],
    ) -> Result<Query, String> {
        let query_str = Self::load_query_from_highlight(highlight_items)?;
        Self::parse_query(language, &query_str)
    }

    /// Load and parse a query from search paths
    pub fn load_query_from_search_paths(
        language: &Language,
        runtime_bases: &[String],
        lang_name: &str,
        file_name: &str,
    ) -> Result<Query, String> {
        let query_str = Self::load_query_file(runtime_bases, lang_name, file_name)?;
        Self::parse_query(language, &query_str)
    }

    /// Resolve library path for a language
    pub fn resolve_library_path(
        library: Option<&String>,
        language: &str,
        search_paths: &Option<Vec<String>>,
    ) -> Option<String> {
        // If explicit library path is provided, use it
        if let Some(lib) = library {
            return Some(lib.clone());
        }

        // Otherwise, search in searchPaths: <base>/parser/
        if let Some(paths) = search_paths {
            for path in paths {
                // Try .so extension first (Linux)
                let so_path = format!("{path}/parser/{language}.so");
                if Path::new(&so_path).exists() {
                    return Some(so_path);
                }

                // Try .dylib extension (macOS)
                let dylib_path = format!("{path}/parser/{language}.dylib");
                if Path::new(&dylib_path).exists() {
                    return Some(dylib_path);
                }

                // Try .dll extension (Windows)
                let dll_path = format!("{path}/parser/{language}.dll");
                if Path::new(&dll_path).exists() {
                    return Some(dll_path);
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_load_query_from_highlight() {
        let items = vec![
            HighlightItem {
                source: HighlightSource::Query {
                    query: "(identifier) @variable".to_string(),
                },
            },
            HighlightItem {
                source: HighlightSource::Query {
                    query: "(string) @string".to_string(),
                },
            },
        ];

        let result = QueryLoader::load_query_from_highlight(&items).unwrap();
        assert!(result.contains("(identifier) @variable"));
        assert!(result.contains("(string) @string"));
    }

    #[test]
    fn test_find_query_file() {
        let dir = tempdir().unwrap();
        let base_path = dir.path().to_str().unwrap().to_string();

        // Create directory structure
        let query_dir = dir.path().join("queries").join("rust");
        fs::create_dir_all(&query_dir).unwrap();

        // Create a query file
        let query_file = query_dir.join("highlights.scm");
        fs::write(&query_file, "(identifier) @variable").unwrap();

        // Test finding the file
        let result = QueryLoader::find_query_file(&[base_path], "rust", "highlights.scm");
        assert!(result.is_some());
        assert_eq!(result.unwrap(), query_file);

        // Test not finding a non-existent file
        let result = QueryLoader::find_query_file(&[], "rust", "highlights.scm");
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_library_path() {
        // Test explicit library path
        let explicit = Some(&"explicit/path.so".to_string());
        let result = QueryLoader::resolve_library_path(explicit, "rust", &None);
        assert_eq!(result, Some("explicit/path.so".to_string()));

        // Test search paths
        let dir = tempdir().unwrap();
        let base_path = dir.path().to_str().unwrap().to_string();

        // Create parser directory
        let parser_dir = dir.path().join("parser");
        fs::create_dir_all(&parser_dir).unwrap();

        // Create a .so file
        let so_file = parser_dir.join("rust.so");
        fs::write(&so_file, "").unwrap();

        let search_paths = Some(vec![base_path]);
        let result = QueryLoader::resolve_library_path(None, "rust", &search_paths);
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("parser/rust.so"));
    }
}
