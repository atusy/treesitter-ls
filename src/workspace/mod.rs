mod document_ops;
mod language_ops;
mod settings;
mod state;

use document_ops::{
    document_language as resolve_document_language, document_reference,
    document_text as read_document_text, parse_document as process_parse,
    remove_document as detach_document, update_semantic_tokens as store_semantic_tokens,
};
use language_ops::{
    capture_mappings as collect_capture_mappings, create_parser_pool,
    ensure_language_loaded as ensure_runtime_language,
    has_highlight_queries as language_has_queries, highlight_query as load_highlight_query,
    locals_query as load_locals_query,
};
pub use settings::{
    SettingsEvent, SettingsEventKind, SettingsLoadOutcome, SettingsSource,
    load_settings as load_settings_from_sources,
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
        let parser_pool = create_parser_pool(&language);
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
        process_parse(
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
        resolve_document_language(&self.language, &self.documents, uri)
    }

    pub fn has_queries(&self, language: &str) -> bool {
        language_has_queries(&self.language, language)
    }

    pub fn highlight_query(&self, language: &str) -> Option<Arc<Query>> {
        load_highlight_query(&self.language, language)
    }

    pub fn locals_query(&self, language: &str) -> Option<Arc<Query>> {
        load_locals_query(&self.language, language)
    }

    pub fn capture_mappings(&self) -> CaptureMappings {
        collect_capture_mappings(&self.language)
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
        ensure_runtime_language(&self.language, language)
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
        let language = LanguageCoordinator::new();

        let settings = DomainWorkspaceSettings::new(
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
