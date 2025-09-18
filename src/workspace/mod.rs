mod document_ops;
mod settings;
mod state;

use document_ops::{
    document_language, document_reference, document_text, parse_document, remove_document,
    update_semantic_tokens,
};
pub use settings::{
    SettingsEvent, SettingsEventKind, SettingsLoadOutcome, SettingsSource, load_settings,
};
use state::WorkspaceState;

use crate::document::{Document, DocumentHandle, DocumentStore};
use crate::domain::SemanticTokens;
use crate::domain::settings::{CaptureMappings, WorkspaceSettings};
use crate::language::{
    DocumentParserPool, LanguageCoordinator, LanguageEvent, LanguageLoadResult, LanguageLoadSummary,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tree_sitter::{InputEdit, Query};
use url::Url;

pub type DocumentRef<'a> = DocumentHandle<'a>;

pub struct ParseOutcome {
    pub events: Vec<LanguageEvent>,
}

pub struct Workspace {
    language: LanguageCoordinator,
    parser_pool: Mutex<DocumentParserPool>,
    documents: DocumentStore,
    state: WorkspaceState,
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
            state: WorkspaceState::new(),
        }
    }

    pub fn language(&self) -> &LanguageCoordinator {
        &self.language
    }

    pub fn documents(&self) -> &DocumentStore {
        &self.documents
    }

    pub fn load_settings(&self, settings: WorkspaceSettings) -> LanguageLoadSummary {
        self.language.load_settings(settings)
    }

    pub fn parse_document(
        &self,
        uri: Url,
        text: String,
        language_id: Option<&str>,
        edits: Vec<InputEdit>,
    ) -> ParseOutcome {
        parse_document(
            &self.language,
            &self.parser_pool,
            &self.documents,
            uri,
            text,
            language_id,
            edits,
        )
    }

    pub fn language_for_document(&self, uri: &Url) -> Option<String> {
        document_language(&self.language, &self.documents, uri)
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
        let root_path = self.state.root_path();
        load_settings(root_path.as_deref(), override_settings)
    }

    pub fn document(&self, uri: &Url) -> Option<DocumentRef<'_>> {
        document_reference(&self.documents, uri)
    }

    pub fn document_text(&self, uri: &Url) -> Option<String> {
        document_text(&self.documents, uri)
    }

    /// Store the latest domain-level semantic tokens snapshot for the document.
    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        update_semantic_tokens(&self.documents, uri, tokens);
    }

    pub fn remove_document(&self, uri: &Url) -> Option<Document> {
        remove_document(&self.documents, uri)
    }

    pub fn ensure_language_loaded(&self, language: &str) -> LanguageLoadResult {
        self.language.ensure_language_loaded(language)
    }

    pub fn set_root_path(&self, path: Option<PathBuf>) {
        self.state.set_root_path(path)
    }

    pub fn root_path(&self) -> Option<PathBuf> {
        self.state.root_path()
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
    use crate::domain::settings::WorkspaceSettings;
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
