use super::languages::WorkspaceLanguages;
use crate::domain::settings::CaptureMappings;
use std::sync::Arc;
use tree_sitter::{Parser, Query};

pub fn language_for_path(languages: &WorkspaceLanguages, path: &str) -> Option<String> {
    languages.language_for_path(path)
}

pub fn ensure_language_loaded(
    languages: &WorkspaceLanguages,
    language_id: &str,
) -> crate::runtime::LanguageLoadResult {
    languages.ensure_language_loaded(language_id)
}

pub fn has_highlight_queries(languages: &WorkspaceLanguages, language: &str) -> bool {
    languages.has_queries(language)
}

pub fn highlight_query(languages: &WorkspaceLanguages, language: &str) -> Option<Arc<Query>> {
    languages.highlight_query(language)
}

pub fn locals_query(languages: &WorkspaceLanguages, language: &str) -> Option<Arc<Query>> {
    languages.locals_query(language)
}

pub fn capture_mappings(languages: &WorkspaceLanguages) -> CaptureMappings {
    languages.capture_mappings()
}

pub fn acquire_parser(languages: &WorkspaceLanguages, language: &str) -> Option<Parser> {
    languages.acquire_parser(language)
}

pub fn release_parser(languages: &WorkspaceLanguages, language: String, parser: Parser) {
    languages.release_parser(language, parser)
}
