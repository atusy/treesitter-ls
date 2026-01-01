//! Diagnostic bridging types for language server bridge.
//!
//! This module contains types for capturing and forwarding publishDiagnostics
//! notifications from bridged language servers to the editor.

use crate::language::injection::CacheableInjectionRegion;
use dashmap::DashMap;
use tower_lsp::lsp_types::{Diagnostic, PublishDiagnosticsParams, Url};

/// Registry mapping virtual document URIs to their host documents and injection regions.
///
/// This registry tracks the relationship between virtual documents (temporary files
/// sent to bridged language servers) and their origin in host documents. This enables
/// translating diagnostics from virtual document coordinates back to host document
/// coordinates for display in the editor.
///
/// Thread-safe using DashMap for concurrent access from multiple bridge connections.
pub(crate) struct VirtualToHostRegistry {
    /// Map from virtual document URI to (host document URI, injection region)
    mappings: DashMap<Url, (Url, CacheableInjectionRegion)>,
}

impl VirtualToHostRegistry {
    /// Create a new empty registry.
    pub(crate) fn new() -> Self {
        Self {
            mappings: DashMap::new(),
        }
    }

    /// Register a mapping from virtual URI to host URI and injection region.
    ///
    /// Called when sending didOpen to a bridged language server, so we can
    /// later translate diagnostics back to the host document.
    pub(crate) fn register(
        &self,
        virtual_uri: Url,
        host_uri: Url,
        region: CacheableInjectionRegion,
    ) {
        self.mappings.insert(virtual_uri, (host_uri, region));
    }

    /// Get the host URI and injection region for a virtual URI.
    ///
    /// Returns None if the virtual URI is not registered.
    pub(crate) fn get(&self, virtual_uri: &Url) -> Option<(Url, CacheableInjectionRegion)> {
        self.mappings.get(virtual_uri).map(|entry| entry.clone())
    }

    /// Clear all mappings for a specific host document.
    ///
    /// Used when a host document changes and all its injection regions
    /// need to be re-registered with fresh virtual URIs.
    pub(crate) fn clear_for_host(&self, host_uri: &Url) {
        // Collect virtual URIs to remove (avoid holding locks during iteration)
        let to_remove: Vec<Url> = self
            .mappings
            .iter()
            .filter(|entry| &entry.value().0 == host_uri)
            .map(|entry| entry.key().clone())
            .collect();

        for virtual_uri in to_remove {
            self.mappings.remove(&virtual_uri);
        }
    }

    /// Translate diagnostics from virtual document coordinates to host document coordinates.
    ///
    /// Takes diagnostics from a bridged language server (in virtual document coordinates)
    /// and returns PublishDiagnosticsParams with the host document URI and translated ranges.
    ///
    /// Returns None if the virtual URI is not registered.
    pub(crate) fn translate_diagnostics(
        &self,
        virtual_uri: &Url,
        diagnostics: Vec<Diagnostic>,
    ) -> Option<PublishDiagnosticsParams> {
        let (host_uri, region) = self.get(virtual_uri)?;

        let translated_diagnostics: Vec<Diagnostic> = diagnostics
            .into_iter()
            .map(|d| region.translate_diagnostic(d))
            .collect();

        Some(PublishDiagnosticsParams {
            uri: host_uri,
            diagnostics: translated_diagnostics,
            version: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Diagnostic, Position, Range, Url};

    #[test]
    fn virtual_to_host_registry_stores_and_retrieves_mapping() {
        use crate::language::injection::CacheableInjectionRegion;

        let registry = VirtualToHostRegistry::new();

        let virtual_uri = Url::parse("file:///tmp/treesitter-ls/src/main.rs").unwrap();
        let host_uri = Url::parse("file:///home/user/document.md").unwrap();
        let region = CacheableInjectionRegion {
            language: "rust".to_string(),
            byte_range: 50..150,
            line_range: 3..10,
            result_id: "region-1".to_string(),
            content_hash: 12345,
        };

        registry.register(virtual_uri.clone(), host_uri.clone(), region.clone());

        let retrieved = registry.get(&virtual_uri);
        assert!(retrieved.is_some());
        let (retrieved_host, retrieved_region) = retrieved.unwrap();
        assert_eq!(retrieved_host, host_uri);
        assert_eq!(retrieved_region.result_id, "region-1");
    }

    #[test]
    fn virtual_to_host_registry_returns_none_for_unknown_uri() {
        let registry = VirtualToHostRegistry::new();

        let unknown_uri = Url::parse("file:///unknown/path.rs").unwrap();
        let retrieved = registry.get(&unknown_uri);

        assert!(retrieved.is_none());
    }

    #[test]
    fn virtual_to_host_registry_clears_by_host_uri() {
        use crate::language::injection::CacheableInjectionRegion;

        let registry = VirtualToHostRegistry::new();

        let virtual_uri1 = Url::parse("file:///tmp/treesitter-ls-1/src/main.rs").unwrap();
        let virtual_uri2 = Url::parse("file:///tmp/treesitter-ls-2/src/main.rs").unwrap();
        let host_uri = Url::parse("file:///home/user/document.md").unwrap();
        let other_host_uri = Url::parse("file:///home/user/other.md").unwrap();

        let region1 = CacheableInjectionRegion {
            language: "rust".to_string(),
            byte_range: 50..150,
            line_range: 3..10,
            result_id: "region-1".to_string(),
            content_hash: 12345,
        };
        let region2 = CacheableInjectionRegion {
            language: "lua".to_string(),
            byte_range: 200..300,
            line_range: 15..20,
            result_id: "region-2".to_string(),
            content_hash: 67890,
        };

        registry.register(virtual_uri1.clone(), host_uri.clone(), region1);
        registry.register(virtual_uri2.clone(), other_host_uri.clone(), region2);

        // Clear only the first host document
        registry.clear_for_host(&host_uri);

        // First virtual URI should be gone
        assert!(registry.get(&virtual_uri1).is_none());

        // Second virtual URI should still exist
        assert!(registry.get(&virtual_uri2).is_some());
    }

    #[test]
    fn translate_diagnostics_transforms_virtual_to_host() {
        use crate::language::injection::CacheableInjectionRegion;

        let registry = VirtualToHostRegistry::new();

        let virtual_uri = Url::parse("file:///tmp/treesitter-ls/src/main.rs").unwrap();
        let host_uri = Url::parse("file:///home/user/document.md").unwrap();

        // Injection region starts at line 3 in host document
        let region = CacheableInjectionRegion {
            language: "rust".to_string(),
            byte_range: 50..150,
            line_range: 3..10,
            result_id: "region-1".to_string(),
            content_hash: 12345,
        };

        registry.register(virtual_uri.clone(), host_uri.clone(), region);

        // Virtual diagnostic at line 0 (should translate to host line 3)
        let virtual_diagnostics = vec![Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 4,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            message: "undefined variable".to_string(),
            ..Default::default()
        }];

        let result = registry.translate_diagnostics(&virtual_uri, virtual_diagnostics);

        assert!(result.is_some());
        let params = result.unwrap();
        assert_eq!(params.uri, host_uri);
        assert_eq!(params.diagnostics.len(), 1);
        assert_eq!(params.diagnostics[0].range.start.line, 3);
        assert_eq!(params.diagnostics[0].range.end.line, 3);
    }
}
