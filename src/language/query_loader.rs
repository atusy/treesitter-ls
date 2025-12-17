use crate::config::{HighlightItem, HighlightSource};
use crate::error::{LspError, LspResult};
use path_clean::PathClean;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Language, Query};

/// Loads Tree-sitter queries from files and configuration
pub struct QueryLoader;

impl QueryLoader {
    /// Parse the `; inherits: lang1,lang2` directive from query content.
    ///
    /// nvim-treesitter queries can inherit from other queries using this directive
    /// on the first line. This function extracts the list of parent languages.
    ///
    /// # Returns
    /// A vector of parent language names (empty if no inheritance).
    pub fn parse_inherits_directive(content: &str) -> Vec<String> {
        let first_line = content.lines().next().unwrap_or("");

        // Pattern: "; inherits: lang1,lang2,..."
        if let Some(rest) = first_line.strip_prefix("; inherits:") {
            rest.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Load query content from highlight items
    pub fn load_query_from_highlight(highlight_items: &[HighlightItem]) -> LspResult<String> {
        let mut combined_query = String::new();

        for item in highlight_items {
            match &item.source {
                HighlightSource::Path { path } => {
                    let normalized_path = PathBuf::from(path).clean();
                    match fs::read_to_string(&normalized_path) {
                        Ok(content) => {
                            combined_query.push_str(&content);
                            combined_query.push('\n');
                        }
                        Err(e) => {
                            return Err(LspError::query(format!(
                                "Failed to read query file {}: {e}",
                                normalized_path.display()
                            )));
                        }
                    }
                }
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
                .join(file_name)
                .clean();
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
    ) -> LspResult<String> {
        match Self::find_query_file(runtime_bases, lang_name, file_name) {
            Some(path) => fs::read_to_string(&path).map_err(|e| {
                LspError::query(format!(
                    "Failed to read query file {}: {}",
                    path.display(),
                    e
                ))
            }),
            None => Err(LspError::query(format!(
                "Query file {} not found for language {} in search paths",
                file_name, lang_name
            ))),
        }
    }

    /// Parse a query string into a Tree-sitter Query
    pub fn parse_query(language: &Language, query_str: &str) -> LspResult<Query> {
        Query::new(language, query_str)
            .map_err(|e| LspError::query(format!("Failed to parse query: {e}")))
    }

    /// Load and parse a highlight query
    pub fn load_highlight_query(
        language: &Language,
        highlight_items: &[HighlightItem],
    ) -> LspResult<Query> {
        let query_str = Self::load_query_from_highlight(highlight_items)?;
        Self::parse_query(language, &query_str)
    }

    /// Load and parse a query from search paths
    pub fn load_query_from_search_paths(
        language: &Language,
        runtime_bases: &[String],
        lang_name: &str,
        file_name: &str,
    ) -> LspResult<Query> {
        let query_str = Self::load_query_file(runtime_bases, lang_name, file_name)?;
        Self::parse_query(language, &query_str)
    }

    /// Resolve library path for a language
    pub fn resolve_library_path(
        library: Option<&String>,
        language: &str,
        search_paths: &Option<Vec<String>>,
    ) -> Option<String> {
        // If explicit library path is provided, normalize and use it
        if let Some(lib) = library {
            let normalized = PathBuf::from(lib).clean();
            return Some(normalized.to_string_lossy().into_owned());
        }

        // Otherwise, search in searchPaths: <base>/parser/
        if let Some(paths) = search_paths {
            for path in paths {
                // Try .so extension first (Linux)
                let so_path = PathBuf::from(path)
                    .join("parser")
                    .join(format!("{language}.so"))
                    .clean();
                if so_path.exists() {
                    return Some(so_path.to_string_lossy().into_owned());
                }

                // Try .dylib extension (macOS)
                let dylib_path = PathBuf::from(path)
                    .join("parser")
                    .join(format!("{language}.dylib"))
                    .clean();
                if dylib_path.exists() {
                    return Some(dylib_path.to_string_lossy().into_owned());
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

    // ============================================================
    // Tests for query inheritance (PBI-020)
    // ============================================================

    #[test]
    fn test_parse_inherits_directive_single() {
        // TypeScript inherits from ecma
        let content = "; inherits: ecma\n\n\"require\" @keyword.import\n";
        let result = QueryLoader::parse_inherits_directive(content);
        assert_eq!(result, vec!["ecma"]);
    }

    #[test]
    fn test_parse_inherits_directive_multiple() {
        // JavaScript inherits from ecma and jsx
        let content = "; inherits: ecma,jsx\n\n(identifier) @variable\n";
        let result = QueryLoader::parse_inherits_directive(content);
        assert_eq!(result, vec!["ecma", "jsx"]);
    }

    #[test]
    fn test_parse_inherits_directive_none() {
        // ecma has no inheritance
        let content = "(identifier) @variable\n";
        let result = QueryLoader::parse_inherits_directive(content);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_inherits_directive_with_spaces() {
        // Handle spaces around language names
        let content = "; inherits: ecma , jsx\n";
        let result = QueryLoader::parse_inherits_directive(content);
        assert_eq!(result, vec!["ecma", "jsx"]);
    }

    #[test]
    fn test_resolve_query_with_inheritance_no_inheritance() {
        // ecma has no inheritance - should return content as-is
        let dir = tempdir().unwrap();
        let base_path = dir.path().to_str().unwrap().to_string();

        // Create ecma query
        let ecma_dir = dir.path().join("queries").join("ecma");
        fs::create_dir_all(&ecma_dir).unwrap();
        fs::write(
            ecma_dir.join("highlights.scm"),
            "(identifier) @variable\n",
        )
        .unwrap();

        let result = QueryLoader::resolve_query_with_inheritance(
            &[base_path],
            "ecma",
            "highlights.scm",
        );
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("(identifier) @variable"));
    }

    #[test]
    fn test_resolve_query_with_inheritance_single_parent() {
        // typescript inherits from ecma
        let dir = tempdir().unwrap();
        let base_path = dir.path().to_str().unwrap().to_string();

        // Create ecma query (base)
        let ecma_dir = dir.path().join("queries").join("ecma");
        fs::create_dir_all(&ecma_dir).unwrap();
        fs::write(
            ecma_dir.join("highlights.scm"),
            "(identifier) @variable\n",
        )
        .unwrap();

        // Create typescript query (inherits ecma)
        let ts_dir = dir.path().join("queries").join("typescript");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(
            ts_dir.join("highlights.scm"),
            "; inherits: ecma\n\n\"require\" @keyword.import\n",
        )
        .unwrap();

        let result = QueryLoader::resolve_query_with_inheritance(
            &[base_path],
            "typescript",
            "highlights.scm",
        );
        assert!(result.is_ok());
        let content = result.unwrap();

        // Should have ecma content first, then typescript
        assert!(content.contains("(identifier) @variable"));
        assert!(content.contains("\"require\" @keyword.import"));

        // ecma content should come before typescript
        let ecma_pos = content.find("(identifier)").unwrap();
        let ts_pos = content.find("\"require\"").unwrap();
        assert!(ecma_pos < ts_pos, "Parent query should come before child");
    }

    #[test]
    fn test_resolve_query_with_inheritance_removes_directive() {
        // The "; inherits:" line should be removed from output
        let dir = tempdir().unwrap();
        let base_path = dir.path().to_str().unwrap().to_string();

        // Create ecma query
        let ecma_dir = dir.path().join("queries").join("ecma");
        fs::create_dir_all(&ecma_dir).unwrap();
        fs::write(ecma_dir.join("highlights.scm"), "(identifier) @variable\n").unwrap();

        // Create typescript query with inherits
        let ts_dir = dir.path().join("queries").join("typescript");
        fs::create_dir_all(&ts_dir).unwrap();
        fs::write(
            ts_dir.join("highlights.scm"),
            "; inherits: ecma\n\"require\" @keyword\n",
        )
        .unwrap();

        let result = QueryLoader::resolve_query_with_inheritance(
            &[base_path],
            "typescript",
            "highlights.scm",
        );
        let content = result.unwrap();

        // The inherits directive should not be in the output
        assert!(!content.contains("; inherits:"));
    }

    #[test]
    fn test_resolve_query_with_inheritance_circular_detection() {
        // a inherits b, b inherits a - should detect and error
        let dir = tempdir().unwrap();
        let base_path = dir.path().to_str().unwrap().to_string();

        let a_dir = dir.path().join("queries").join("lang_a");
        fs::create_dir_all(&a_dir).unwrap();
        fs::write(a_dir.join("highlights.scm"), "; inherits: lang_b\n(a) @a\n").unwrap();

        let b_dir = dir.path().join("queries").join("lang_b");
        fs::create_dir_all(&b_dir).unwrap();
        fs::write(b_dir.join("highlights.scm"), "; inherits: lang_a\n(b) @b\n").unwrap();

        let result = QueryLoader::resolve_query_with_inheritance(
            &[base_path],
            "lang_a",
            "highlights.scm",
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("circular") || err.to_string().contains("Circular"));
    }
}
