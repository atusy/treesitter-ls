pub mod store;

pub(crate) mod model;

// Re-export main types
pub use model::Document;
pub use store::{DocumentHandle, DocumentStore};

use crate::language::LanguageCoordinator;
use url::Url;

/// Get the language for a document from path or stored language_id.
///
/// This function:
/// 1. Tries path-based detection using LanguageCoordinator
/// 2. Falls back to the document's stored language_id
pub(crate) fn get_language_for_document(
    uri: &Url,
    language: &LanguageCoordinator,
    documents: &DocumentStore,
) -> Option<String> {
    // Try path-based detection first
    if let Some(lang) = language.get_language_for_path(uri.path()) {
        return Some(lang);
    }
    // Fall back to document's stored language
    documents
        .get(uri)
        .and_then(|doc| doc.language_id().map(|s| s.to_string()))
}
