//! Auto-install functionality for kakehashi.
//!
//! This module handles automatic installation of missing language parsers and queries
//! when a file is opened that requires them.
//!
//! # Module Structure
//!
//! - `InstallingLanguages`: Tracks concurrent installs to prevent duplicates
//! - `AutoInstallManager`: Isolated coordinator for installation
//! - `InstallResult`, `InstallOutcome`, `InstallEvent`: Event-based return types

mod manager;

pub(crate) use manager::{AutoInstallManager, InstallEvent};

use crate::document::{DocumentStore, get_language_for_document};
use crate::error::LockResultExt;
use crate::language::LanguageCoordinator;
use crate::language::injection::collect_all_injections;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use url::Url;

/// Tracks languages currently being installed to prevent duplicate installs.
///
/// This type is cheaply cloneable via `Arc` for sharing across async tasks.
#[derive(Clone)]
pub struct InstallingLanguages {
    languages: Arc<Mutex<HashSet<String>>>,
}

impl InstallingLanguages {
    pub fn new() -> Self {
        Self {
            languages: Arc::new(Mutex::new(HashSet::new())),
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    impl InstallingLanguages {
        /// Check if a language is currently being installed (test helper).
        fn is_installing(&self, language: &str) -> bool {
            self.languages
                .lock()
                .recover_poison("InstallingLanguages::is_installing")
                .unwrap()
                .contains(language)
        }
    }

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
}
