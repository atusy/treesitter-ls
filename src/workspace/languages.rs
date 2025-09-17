use crate::domain::settings::{CaptureMappings, WorkspaceSettings};
use crate::language::{
    DocumentParserPool, LanguageCoordinator, LanguageLoadResult, LanguageLoadSummary,
};
use std::sync::{Arc, Mutex};
use tree_sitter::{Parser, Query};

pub struct WorkspaceLanguages {
    runtime: LanguageCoordinator,
    parser_pool: Mutex<DocumentParserPool>,
}

impl WorkspaceLanguages {
    pub fn new(runtime: LanguageCoordinator) -> Self {
        let pool = runtime.create_document_parser_pool();
        Self {
            runtime,
            parser_pool: Mutex::new(pool),
        }
    }

    pub fn runtime(&self) -> &LanguageCoordinator {
        &self.runtime
    }

    pub fn load_settings(&self, settings: WorkspaceSettings) -> LanguageLoadSummary {
        self.runtime.load_settings(settings)
    }

    pub fn language_for_path(&self, path: &str) -> Option<String> {
        self.runtime.get_language_for_path(path)
    }

    pub fn ensure_language_loaded(&self, language_id: &str) -> LanguageLoadResult {
        if self.runtime.is_language_loaded(language_id) {
            LanguageLoadResult::success_with(Vec::new())
        } else {
            self.runtime.try_load_language_by_id(language_id)
        }
    }

    pub fn has_queries(&self, language: &str) -> bool {
        self.runtime.has_queries(language)
    }

    pub fn highlight_query(&self, language: &str) -> Option<Arc<Query>> {
        self.runtime.get_highlight_query(language)
    }

    pub fn locals_query(&self, language: &str) -> Option<Arc<Query>> {
        self.runtime.get_locals_query(language)
    }

    pub fn capture_mappings(&self) -> CaptureMappings {
        self.runtime.get_capture_mappings()
    }

    pub fn acquire_parser(&self, language: &str) -> Option<Parser> {
        self.parser_pool.lock().unwrap().acquire(language)
    }

    pub fn release_parser(&self, language: String, parser: Parser) {
        self.parser_pool.lock().unwrap().release(language, parser);
    }
}

impl Default for WorkspaceLanguages {
    fn default() -> Self {
        Self::new(LanguageCoordinator::new())
    }
}
