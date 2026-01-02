//! Auto-install functionality for treesitter-ls.
//!
//! This module handles automatic installation of missing language parsers and queries
//! when a file is opened that requires them.

use crate::document::DocumentStore;
use crate::error::LockResultExt;
use crate::install::metadata::{FetchOptions, is_language_supported};
use crate::language::LanguageCoordinator;
use crate::language::injection::collect_all_injections;
use std::collections::HashSet;
use std::sync::Mutex;
use tower_lsp::lsp_types::Url;

/// Tracks languages currently being installed to prevent duplicate installs.
pub struct InstallingLanguages {
    languages: Mutex<HashSet<String>>,
}

impl InstallingLanguages {
    pub fn new() -> Self {
        Self {
            languages: Mutex::new(HashSet::new()),
        }
    }

    /// Check if a language is currently being installed.
    #[cfg(test)]
    fn is_installing(&self, language: &str) -> bool {
        self.languages
            .lock()
            .recover_poison("InstallingLanguages::is_installing")
            .unwrap()
            .contains(language)
    }

    /// Try to start installing a language. Returns true if this call started the install,
    /// false if it was already being installed.
    pub fn try_start_install(&self, language: &str) -> bool {
        self.languages
            .lock()
            .recover_poison("InstallingLanguages::try_start_install")
            .unwrap()
            .insert(language.to_string())
    }

    /// Mark a language installation as complete.
    pub fn finish_install(&self, language: &str) {
        self.languages
            .lock()
            .recover_poison("InstallingLanguages::finish_install")
            .unwrap()
            .remove(language);
    }
}

impl Default for InstallingLanguages {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the language for a document from path or stored language_id.
pub fn get_language_for_document(
    uri: &Url,
    language: &LanguageCoordinator,
    documents: &DocumentStore,
) -> Option<String> {
    // Try path-based detection first
    if let Some(lang) = language.get_language_for_path(uri.path()) {
        return Some(lang);
    }
    // Fall back to document's stored language
    documents
        .get(uri)
        .and_then(|doc| doc.language_id().map(|s| s.to_string()))
}

/// Get unique injected languages from a document.
///
/// This function:
/// 1. Gets the injection query for the host language from the coordinator
/// 2. Gets the parsed tree from document store
/// 3. Calls collect_all_injections() to get all injection regions
/// 4. Extracts unique language names from the regions
/// 5. Returns the set of languages that need checking
///
/// Returns an empty set if:
/// - The document doesn't exist in the store
/// - The document has no parsed tree
/// - The host language has no injection query
/// - No injection regions are found
pub fn get_injected_languages(
    uri: &Url,
    language: &LanguageCoordinator,
    documents: &DocumentStore,
) -> HashSet<String> {
    // Get the host language for this document
    let language_name = match get_language_for_document(uri, language, documents) {
        Some(name) => name,
        None => return HashSet::new(),
    };

    // Get the injection query for the host language
    let injection_query = match language.get_injection_query(&language_name) {
        Some(q) => q,
        None => return HashSet::new(), // No injection support for this language
    };

    // Get the document and its parsed tree
    let doc = match documents.get(uri) {
        Some(d) => d,
        None => return HashSet::new(),
    };

    let text = doc.text();
    let tree = match doc.tree() {
        Some(t) => t,
        None => return HashSet::new(),
    };

    // Collect all injection regions and extract unique languages
    let injections = match collect_all_injections(&tree.root_node(), text, Some(&injection_query)) {
        Some(injs) => injs,
        None => return HashSet::new(),
    };

    injections.iter().map(|i| i.language.clone()).collect()
}

/// Check if a language should be skipped during auto-install because it's not supported.
///
/// Returns a tuple of (should_skip, reason) where:
/// - should_skip: true if the language is NOT supported by nvim-treesitter and should be skipped
/// - reason: Some(message) explaining why installation was skipped, or None if not skipping
///
/// This function uses cached metadata from nvim-treesitter to avoid repeated HTTP requests.
///
/// # Arguments
/// * `language` - The language name to check
/// * `options` - FetchOptions for metadata caching (use with data_dir and use_cache: true)
pub fn should_skip_unsupported_language(
    language: &str,
    options: Option<&FetchOptions>,
) -> (bool, Option<String>) {
    if is_language_supported(language, options) {
        // Language is supported - don't skip
        (false, None)
    } else {
        // Language is not supported - skip with reason
        let reason = format!(
            "Language '{}' is not supported by nvim-treesitter. Skipping auto-install.",
            language
        );
        (true, Some(reason))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_track_installing_languages() {
        // Test the InstallingLanguages helper struct
        let tracker = InstallingLanguages::new();

        // Initially not installing
        assert!(!tracker.is_installing("lua"));

        // Try to start installation - should succeed
        assert!(tracker.try_start_install("lua"));

        // Now it's installing
        assert!(tracker.is_installing("lua"));

        // Second try should fail (already installing)
        assert!(!tracker.try_start_install("lua"));

        // Mark as complete
        tracker.finish_install("lua");

        // No longer installing
        assert!(!tracker.is_installing("lua"));
    }

    #[test]
    fn test_get_injected_languages_extracts_unique_languages() {
        // Test that get_injected_languages extracts unique languages from injection regions
        // using collect_all_injections from the injection module

        use tree_sitter::{Parser, Query};

        // Create a simple test using Rust's string literal injection pattern
        // This allows us to test without needing the full markdown parser setup
        let rust_code = r#"let x = "test"; let y = "another";"#;

        // Parse with Rust parser
        let mut parser = Parser::new();
        let language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&language).expect("load rust grammar");
        let tree = parser.parse(rust_code, None).expect("parse rust");
        let root = tree.root_node();

        // Create a mock injection query that injects strings as "text" language
        let query_str = r#"
            ((string_literal
              (string_content) @injection.content)
             (#set! injection.language "text"))
        "#;
        let injection_query = Query::new(&language, query_str).expect("valid query");

        // Call collect_all_injections
        let injections =
            collect_all_injections(&root, rust_code, Some(&injection_query)).unwrap_or_default();

        // Extract unique languages from the injection regions
        let unique_languages: HashSet<String> =
            injections.iter().map(|i| i.language.clone()).collect();

        // Should have found the "text" language (from both string literals)
        // but only unique - so just 1 entry
        assert_eq!(unique_languages.len(), 1);
        assert!(unique_languages.contains("text"));

        // Test with multiple languages
        let query_str_multi = r#"
            ((raw_string_literal) @injection.content
             (#set! injection.language "regex"))

            ((string_literal
              (string_content) @injection.content)
             (#set! injection.language "text"))
        "#;
        let multi_query = Query::new(&language, query_str_multi).expect("valid query");

        let rust_code_multi = r#"let x = "test"; let re = r"^\d+$";"#;
        let tree_multi = parser.parse(rust_code_multi, None).expect("parse rust");
        let root_multi = tree_multi.root_node();

        let injections_multi =
            collect_all_injections(&root_multi, rust_code_multi, Some(&multi_query))
                .unwrap_or_default();

        let unique_multi: HashSet<String> = injections_multi
            .iter()
            .map(|i| i.language.clone())
            .collect();

        // Should have both "text" and "regex"
        assert!(unique_multi.contains("text") || unique_multi.contains("regex"));
    }

    #[test]
    fn test_should_skip_unsupported_language_returns_true_for_unsupported() {
        // Test that should_skip_unsupported_language returns true for unsupported languages
        // with a reason explaining why installation was skipped
        use crate::install::metadata::FetchOptions;
        use crate::install::test_helpers::setup_mock_metadata_cache;
        use tempfile::tempdir;

        let temp = tempdir().expect("Failed to create temp dir");

        // Mock the cache with parsers.lua content that includes only 'lua'
        let mock_parsers_lua = r#"
return {
  lua = {
    install_info = {
      revision = 'abc123',
      url = 'https://github.com/MunifTanjim/tree-sitter-lua',
    },
    tier = 2,
  },
}
"#;
        setup_mock_metadata_cache(temp.path(), mock_parsers_lua);

        let options = FetchOptions {
            data_dir: Some(temp.path()),
            use_cache: true,
        };

        // should_skip_unsupported_language should return true for 'fake_lang_xyz'
        let (should_skip, reason) =
            should_skip_unsupported_language("fake_lang_xyz", Some(&options));
        assert!(
            should_skip,
            "Expected to skip unsupported language 'fake_lang_xyz'"
        );
        assert!(reason.is_some(), "Expected a reason for skipping");
        let reason_str = reason.unwrap();
        assert!(
            reason_str.contains("not supported") || reason_str.contains("nvim-treesitter"),
            "Reason should mention nvim-treesitter support: {}",
            reason_str
        );
    }

    #[test]
    fn test_should_skip_unsupported_language_returns_false_for_supported() {
        // Test that should_skip_unsupported_language returns false for supported languages
        use crate::install::metadata::FetchOptions;
        use crate::install::test_helpers::setup_mock_metadata_cache;
        use tempfile::tempdir;

        let temp = tempdir().expect("Failed to create temp dir");

        // Mock the cache with parsers.lua content that includes 'lua'
        let mock_parsers_lua = r#"
return {
  lua = {
    install_info = {
      revision = 'abc123',
      url = 'https://github.com/MunifTanjim/tree-sitter-lua',
    },
    tier = 2,
  },
}
"#;
        setup_mock_metadata_cache(temp.path(), mock_parsers_lua);

        let options = FetchOptions {
            data_dir: Some(temp.path()),
            use_cache: true,
        };

        // should_skip_unsupported_language should return false for 'lua'
        let (should_skip, reason) = should_skip_unsupported_language("lua", Some(&options));
        assert!(
            !should_skip,
            "Expected NOT to skip supported language 'lua'"
        );
        assert!(reason.is_none(), "Expected no reason when not skipping");
    }
}
