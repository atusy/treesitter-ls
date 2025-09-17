mod document_ops;
mod language_ops;
mod languages;
mod settings;
mod state;

use document_ops::{
    document_language as resolve_document_language,
    document_reference,
    document_text as read_document_text,
    parse_document as process_parse,
    remove_document as detach_document,
    update_semantic_tokens as store_semantic_tokens,
};
use language_ops::{
    capture_mappings as collect_capture_mappings,
    ensure_language_loaded as ensure_runtime_language,
    has_highlight_queries as language_has_queries,
    highlight_query as load_highlight_query,
    locals_query as load_locals_query,
};
use languages::WorkspaceLanguages;
use settings::{
    load_settings as load_settings_from_sources, SettingsEvent, SettingsEventKind,
    SettingsLoadOutcome, SettingsSource,
};
use state::WorkspaceState;

use crate::document::{Document, DocumentHandle, DocumentStore};
use crate::domain::SemanticTokens;
use crate::domain::settings::{CaptureMappings, WorkspaceSettings};
use crate::language::{LanguageCoordinator, LanguageEvent, LanguageLoadResult, LanguageLoadSummary};
use std::path::PathBuf;
use std::sync::Arc;
use tree_sitter::{InputEdit, Query};
use url::Url;

pub type DocumentRef<'a> = DocumentHandle<'a>;

pub struct ParseOutcome {
    pub events: Vec<LanguageEvent>,
}

pub struct Workspace {
    languages: WorkspaceLanguages,
    documents: DocumentStore,
    state: WorkspaceState,
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_runtime(LanguageCoordinator::new())
    }

    /// Create a workspace using a pre-configured language coordinator.
    pub fn with_runtime(runtime: LanguageCoordinator) -> Self {
        Self {
            languages: WorkspaceLanguages::new(runtime),
            documents: DocumentStore::new(),
            state: WorkspaceState::new(),
        }
    }

    pub fn languages(&self) -> &WorkspaceLanguages {
        &self.languages
    }

    pub fn documents(&self) -> &DocumentStore {
        &self.documents
    }

    pub fn load_settings(&self, settings: WorkspaceSettings) -> LanguageLoadSummary {
        self.languages.load_settings(settings)
    }

    pub fn parse_document(
        &self,
        uri: Url,
        text: String,
        language_id: Option<&str>,
        edits: Vec<InputEdit>,
    ) -> ParseOutcome {
        process_parse(&self.languages, &self.documents, uri, text, language_id, edits)
    }

    pub fn language_for_document(&self, uri: &Url) -> Option<String> {
        resolve_document_language(&self.languages, &self.documents, uri)
    }

    pub fn has_queries(&self, language: &str) -> bool {
        language_has_queries(&self.languages, language)
    }

    pub fn highlight_query(&self, language: &str) -> Option<Arc<Query>> {
        load_highlight_query(&self.languages, language)
    }

    pub fn locals_query(&self, language: &str) -> Option<Arc<Query>> {
        load_locals_query(&self.languages, language)
    }

    pub fn capture_mappings(&self) -> CaptureMappings {
        collect_capture_mappings(&self.languages)
    }

    pub fn load_workspace_settings(
        &self,
        override_settings: Option<(SettingsSource, serde_json::Value)>,
    ) -> SettingsLoadOutcome {
        let root_path = self.state.root_path();
        load_settings_from_sources(root_path.as_deref(), override_settings)
    }

    pub fn document(&self, uri: &Url) -> Option<DocumentRef<'_>> {
        document_reference(&self.documents, uri)
    }

    pub fn document_text(&self, uri: &Url) -> Option<String> {
        read_document_text(&self.documents, uri)
    }

    /// Store the latest domain-level semantic tokens snapshot for the document.
    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        store_semantic_tokens(&self.documents, uri, tokens);
    }

    pub fn remove_document(&self, uri: &Url) -> Option<Document> {
        detach_document(&self.documents, uri)
    }

    pub fn ensure_language_loaded(&self, language: &str) -> LanguageLoadResult {
        ensure_runtime_language(&self.languages, language)
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
    use crate::domain::settings::WorkspaceSettings as DomainWorkspaceSettings;
    use std::collections::HashMap;

    #[test]
    fn workspace_can_inject_runtime() {
        let runtime = crate::language::LanguageCoordinator::new();

        let settings = DomainWorkspaceSettings::new(
            vec!["/tmp/treesitter-ls-test".to_string()],
            HashMap::new(),
            HashMap::new(),
        );
        runtime.load_settings(settings);

        let workspace = Workspace::with_runtime(runtime);

        let injected_paths = workspace.languages().runtime().get_search_paths().unwrap();

        assert_eq!(injected_paths, vec!["/tmp/treesitter-ls-test".to_string()]);
    }
}
