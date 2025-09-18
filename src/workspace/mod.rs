mod settings;

pub use settings::{
    SettingsEvent, SettingsEventKind, SettingsLoadOutcome, SettingsSource, load_settings,
};

use crate::config::{CaptureMappings, WorkspaceSettings};
use crate::document::{DocumentHandle, DocumentStore};
use crate::language::{
    DocumentParserPool, LanguageCoordinator, LanguageLoadResult, LanguageLoadSummary,
};
use arc_swap::ArcSwap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tree_sitter::Query;
use url::Url;

pub type DocumentRef<'a> = DocumentHandle<'a>;

pub struct Workspace {
    language: LanguageCoordinator,
    parser_pool: Mutex<DocumentParserPool>,
    documents: DocumentStore,
    root_path: ArcSwap<Option<PathBuf>>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_runtime(LanguageCoordinator::new())
    }

    /// Create a workspace using a pre-configured language coordinator.
    pub fn with_runtime(language: LanguageCoordinator) -> Self {
        let parser_pool = language.create_document_parser_pool();
        Self {
            language,
            parser_pool: Mutex::new(parser_pool),
            documents: DocumentStore::new(),
            root_path: ArcSwap::new(Arc::new(None)),
        }
    }

    pub fn language(&self) -> &LanguageCoordinator {
        &self.language
    }

    pub fn documents(&self) -> &DocumentStore {
        &self.documents
    }

    pub fn parser_pool(&self) -> &Mutex<DocumentParserPool> {
        &self.parser_pool
    }

    pub fn load_settings(&self, settings: WorkspaceSettings) -> LanguageLoadSummary {
        self.language.load_settings(settings)
    }

    pub fn language_for_document(&self, uri: &Url) -> Option<String> {
        // Try path-based detection first
        if let Some(lang) = self.language.get_language_for_path(uri.path()) {
            return Some(lang);
        }
        // Fall back to document's stored language
        self.documents
            .get(uri)
            .and_then(|doc| doc.language_id().map(|s| s.to_string()))
    }

    pub fn has_queries(&self, language: &str) -> bool {
        self.language.has_queries(language)
    }

    pub fn highlight_query(&self, language: &str) -> Option<Arc<Query>> {
        self.language.get_highlight_query(language)
    }

    pub fn locals_query(&self, language: &str) -> Option<Arc<Query>> {
        self.language.get_locals_query(language)
    }

    pub fn capture_mappings(&self) -> CaptureMappings {
        self.language.get_capture_mappings()
    }

    pub fn load_workspace_settings(
        &self,
        override_settings: Option<(SettingsSource, serde_json::Value)>,
    ) -> SettingsLoadOutcome {
        let root_path = self.root_path();
        load_settings(root_path.as_deref(), override_settings)
    }

    pub fn document(&self, uri: &Url) -> Option<DocumentRef<'_>> {
        self.documents.get(uri)
    }

    pub fn ensure_language_loaded(&self, language: &str) -> LanguageLoadResult {
        self.language.ensure_language_loaded(language)
    }

    pub fn set_root_path(&self, path: Option<PathBuf>) {
        self.root_path.store(Arc::new(path));
    }

    pub fn root_path(&self) -> Option<PathBuf> {
        self.root_path.load().as_ref().clone()
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorkspaceSettings;
    use std::collections::HashMap;

    #[test]
    fn workspace_can_inject_runtime() {
        let language = LanguageCoordinator::new();

        let settings = WorkspaceSettings::new(
            vec!["/tmp/treesitter-ls-test".to_string()],
            HashMap::new(),
            HashMap::new(),
        );
        language.load_settings(settings);

        let workspace = Workspace::with_runtime(language);

        let injected_paths = workspace.language().get_search_paths().unwrap();

        assert_eq!(injected_paths, vec!["/tmp/treesitter-ls-test".to_string()]);
    }
}
