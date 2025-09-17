mod documents;
mod languages;
mod settings;

use documents::WorkspaceDocuments;
use languages::WorkspaceLanguages;

use crate::document::{Document, LanguageLayer, SemanticSnapshot};
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
        let mut events = Vec::new();
        let language_name = self
            .languages
            .language_for_path(uri.path())
            .or_else(|| language_id.map(|s| s.to_string()));

        if let Some(language_name) = language_name {
            let load_result = self.languages.ensure_language_loaded(&language_name);
            events.extend(load_result.events.clone());

            if let Some(mut parser) = self.languages.acquire_parser(&language_name) {
                let old_tree = if !edits.is_empty() {
                    self.documents.get_edited_tree(&uri, &edits)
                } else {
                    self.documents
                        .get(&uri)
                        .and_then(|doc| doc.layers().root_layer().map(|layer| layer.tree.clone()))
                };

                let parsed_tree = parser.parse(&text, old_tree.as_ref());
                self.languages.release_parser(language_name.clone(), parser);

                if let Some(tree) = parsed_tree {
                    if !edits.is_empty() {
                        self.documents.update_with_tree(uri.clone(), text, tree);
                    } else {
                        let root_layer = Some(LanguageLayer::root(language_name.clone(), tree));
                        self.documents.insert(uri.clone(), text, root_layer);
                    }

                    return ParseOutcome { events };
                }
            }
        }

        self.documents.insert(uri, text, None);
        ParseOutcome { events }
    }

    pub fn language_for_document(&self, uri: &Url) -> Option<String> {
        if let Some(lang) = self.languages.language_for_path(uri.path()) {
            return Some(lang);
        }

        self.documents
            .get(uri)
            .and_then(|doc| doc.layers().get_language_id().map(|s| s.to_string()))
    }

    pub fn has_queries(&self, language: &str) -> bool {
        self.languages.has_queries(language)
    }

    pub fn highlight_query(&self, language: &str) -> Option<Arc<Query>> {
        self.languages.highlight_query(language)
    }

    pub fn locals_query(&self, language: &str) -> Option<Arc<Query>> {
        self.languages.locals_query(language)
    }

    pub fn capture_mappings(&self) -> CaptureMappings {
        self.languages.capture_mappings()
    }

    pub fn load_workspace_settings(
        &self,
        override_settings: Option<(SettingsSource, serde_json::Value)>,
    ) -> SettingsLoadOutcome {
        let root_path = self.root_path();
        settings::load_settings(root_path.as_deref(), override_settings)
    }

    pub fn document(&self, uri: &Url) -> Option<DocumentRef<'_>> {
        self.documents.get(uri)
    }

    pub fn document_text(&self, uri: &Url) -> Option<String> {
        self.documents.text(uri)
    }

    /// Store the latest domain-level semantic tokens snapshot for the document.
    pub fn update_semantic_tokens(&self, uri: &Url, tokens: SemanticTokens) {
        self.documents
            .update_semantic_tokens(uri, SemanticSnapshot::new(tokens));
    }

    pub fn remove_document(&self, uri: &Url) -> Option<Document> {
        self.documents.remove(uri)
    }

    pub fn ensure_language_loaded(&self, language: &str) -> LanguageLoadResult {
        self.languages.ensure_language_loaded(language)
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
