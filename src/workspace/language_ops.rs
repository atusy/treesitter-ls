use crate::domain::settings::CaptureMappings;
use crate::language::{DocumentParserPool, LanguageCoordinator, LanguageLoadResult};
use std::sync::Arc;
use std::sync::Mutex;
use tree_sitter::{Parser, Query};

pub fn language_for_path(coordinator: &LanguageCoordinator, path: &str) -> Option<String> {
    coordinator.get_language_for_path(path)
}

pub fn ensure_language_loaded(
    coordinator: &LanguageCoordinator,
    language_id: &str,
) -> LanguageLoadResult {
    coordinator.ensure_language_loaded(language_id)
}

pub fn has_highlight_queries(coordinator: &LanguageCoordinator, language: &str) -> bool {
    coordinator.has_queries(language)
}

pub fn highlight_query(coordinator: &LanguageCoordinator, language: &str) -> Option<Arc<Query>> {
    coordinator.get_highlight_query(language)
}

pub fn locals_query(coordinator: &LanguageCoordinator, language: &str) -> Option<Arc<Query>> {
    coordinator.get_locals_query(language)
}

pub fn injections_query(coordinator: &LanguageCoordinator, language: &str) -> Option<Arc<Query>> {
    coordinator.get_injections_query(language)
}

pub fn capture_mappings(coordinator: &LanguageCoordinator) -> CaptureMappings {
    coordinator.get_capture_mappings()
}

pub fn acquire_parser(parser_pool: &Mutex<DocumentParserPool>, language: &str) -> Option<Parser> {
    parser_pool.lock().unwrap().acquire(language)
}

pub fn release_parser(parser_pool: &Mutex<DocumentParserPool>, language: String, parser: Parser) {
    parser_pool.lock().unwrap().release(language, parser);
}

pub fn create_parser_pool(coordinator: &LanguageCoordinator) -> DocumentParserPool {
    coordinator.create_document_parser_pool()
}
