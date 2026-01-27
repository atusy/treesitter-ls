pub mod store;

pub(crate) mod model;

// Re-export main types
pub use model::Document;
pub use store::{DocumentHandle, DocumentStore};

use crate::language::LanguageCoordinator;
use url::Url;

/// Get the language for a document using the full detection chain.
///
/// This function uses LanguageCoordinator::detect_language() which implements
/// the fallback chain: languageId → alias → shebang → extension (ADR-0005).
///
/// This ensures aliases are resolved (e.g., "rmd" → "markdown") even when
/// the document is accessed before didOpen fully completes (race condition).
pub(crate) fn get_language_for_document(
    uri: &Url,
    language: &LanguageCoordinator,
    documents: &DocumentStore,
) -> Option<String> {
    let path = uri.path();

    // Get the document's language_id and content if available
    let (language_id, content) = documents
        .get(uri)
        .map(|doc| {
            (
                doc.language_id().map(|s| s.to_string()),
                doc.text().to_string(),
            )
        })
        .unwrap_or((None, String::new()));

    // ADR-0005: Unified detection chain with alias resolution at each step
    // Priority: languageId → heuristics (first-line, filename) → extension
    // Host document: token=None (no code fence identifier)
    language.detect_language(path, &content, None, language_id.as_deref())
}
