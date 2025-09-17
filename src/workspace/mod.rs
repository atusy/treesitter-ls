mod document_ops;
mod documents;
mod languages;
mod language_ops;
mod settings;

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
    language_for_path,
    locals_query as load_locals_query,
};
use documents::WorkspaceDocuments;
use languages::WorkspaceLanguages;

use crate::document::Document;
use crate::domain::SemanticTokens;
use crate::domain::settings::{CaptureMappings, WorkspaceSettings};
use crate::runtime::{LanguageEvent, LanguageLoadResult, LanguageLoadSummary};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tree_sitter::{InputEdit, Query};
use url::Url;

pub use settings::{
    SettingsEvent, SettingsEventKind, SettingsLoadOutcome, SettingsSource,
    load_settings as load_settings_from_sources,
};

pub use documents::DocumentRef;

pub struct ParseOutcome {
    pub events: Vec<LanguageEvent>,
}

pub struct Workspace {
    languages: WorkspaceLanguages,
    documents: WorkspaceDocuments,
    root_path: Mutex<Option<PathBuf>>,
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_runtime(crate::runtime::RuntimeCoordinator::new())
    }

    /// Create a workspace using a pre-configured runtime coordinator.
    ///
    /// Useful in tests or alternate frontends that need to customise the
    /// runtime (preloaded languages, alternate search paths, etc.) before
    /// wiring it together with the document store.
    pub fn with_runtime(runtime: crate::runtime::RuntimeCoordinator) -> Self {
        Self {
            languages: WorkspaceLanguages::new(runtime),
            documents: WorkspaceDocuments::new(),
            root_path: Mutex::new(None),
        }
    }

    pub fn languages(&self) -> &WorkspaceLanguages {
        &self.languages
    }

    pub fn documents(&self) -> &WorkspaceDocuments {
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
        process_parse(
            &self.languages,
            &self.documents,
            uri,
            text,
            language_id,
            edits,
        )
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
        let root_path = self.root_path();
        settings::load_settings(root_path.as_deref(), override_settings)
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
        match self.root_path.lock() {
            Ok(mut guard) => *guard = path,
            Err(poisoned) => {
                log::warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in workspace::set_root_path",
                );
                *poisoned.into_inner() = path;
            }
        }
    }

    pub fn root_path(&self) -> Option<PathBuf> {
        match self.root_path.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => {
                log::warn!(
                    target: "treesitter_ls::lock_recovery",
                    "Recovered from poisoned lock in workspace::root_path",
                );
                poisoned.into_inner().clone()
            }
        }
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
        let runtime = crate::runtime::RuntimeCoordinator::new();

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
