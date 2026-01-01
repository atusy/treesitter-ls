//! Diagnostic bridging types for language server bridge.
//!
//! This module contains types for capturing and forwarding publishDiagnostics
//! notifications from bridged language servers to the editor.

use dashmap::DashMap;
use tower_lsp::lsp_types::{Diagnostic, Url};

/// Collects diagnostics from bridged language servers.
///
/// Stores publishDiagnostics notifications keyed by virtual document URI.
/// This allows later retrieval for translation and forwarding to the editor.
///
/// Thread-safe using DashMap for concurrent access from multiple bridge connections.
#[allow(dead_code)] // Used in later subtasks of PBI-135
pub(crate) struct DiagnosticCollector {
    /// Map from virtual document URI to its diagnostics
    diagnostics: DashMap<Url, Vec<Diagnostic>>,
}

#[allow(dead_code)] // Methods used in later subtasks of PBI-135
impl DiagnosticCollector {
    /// Create a new empty DiagnosticCollector.
    pub(crate) fn new() -> Self {
        Self {
            diagnostics: DashMap::new(),
        }
    }

    /// Insert diagnostics for a virtual document URI.
    ///
    /// Replaces any existing diagnostics for this URI.
    pub(crate) fn insert(&self, uri: Url, diagnostics: Vec<Diagnostic>) {
        self.diagnostics.insert(uri, diagnostics);
    }

    /// Get diagnostics for a virtual document URI.
    ///
    /// Returns None if no diagnostics have been stored for this URI.
    pub(crate) fn get(&self, uri: &Url) -> Option<Vec<Diagnostic>> {
        self.diagnostics.get(uri).map(|entry| entry.clone())
    }

    /// Remove diagnostics for a virtual document URI.
    ///
    /// Used when a document is closed or when diagnostics need to be cleared
    /// before re-collection (e.g., on didChange).
    pub(crate) fn remove(&self, uri: &Url) {
        self.diagnostics.remove(uri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url};

    #[test]
    fn diagnostic_collector_stores_diagnostics_by_virtual_uri() {
        let collector = DiagnosticCollector::new();

        let virtual_uri = Url::parse("file:///tmp/treesitter-ls/src/main.rs").unwrap();
        let diagnostics = vec![Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            message: "test error".to_string(),
            ..Default::default()
        }];

        collector.insert(virtual_uri.clone(), diagnostics.clone());

        let retrieved = collector.get(&virtual_uri);
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].message, "test error");
    }

    #[test]
    fn diagnostic_collector_returns_none_for_unknown_uri() {
        let collector = DiagnosticCollector::new();

        let unknown_uri = Url::parse("file:///unknown/path.rs").unwrap();
        let retrieved = collector.get(&unknown_uri);

        assert!(retrieved.is_none());
    }

    #[test]
    fn diagnostic_collector_clears_diagnostics_for_uri() {
        let collector = DiagnosticCollector::new();

        let virtual_uri = Url::parse("file:///tmp/treesitter-ls/src/main.rs").unwrap();
        let diagnostics = vec![Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
            message: "test".to_string(),
            ..Default::default()
        }];

        collector.insert(virtual_uri.clone(), diagnostics);
        collector.remove(&virtual_uri);

        let retrieved = collector.get(&virtual_uri);
        assert!(retrieved.is_none());
    }
}
