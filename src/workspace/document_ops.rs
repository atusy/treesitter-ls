use super::ParseOutcome;
use super::language_ops;
use crate::document::{Document, DocumentHandle, DocumentStore, LanguageLayer, SemanticSnapshot};
use crate::domain::SemanticTokens;
use crate::language::{DocumentParserPool, LanguageCoordinator};
use std::sync::Mutex;
use tree_sitter::InputEdit;
use url::Url;

pub fn parse_document(
    coordinator: &LanguageCoordinator,
    parser_pool: &Mutex<DocumentParserPool>,
    documents: &DocumentStore,
    uri: Url,
    text: String,
    language_id: Option<&str>,
    edits: Vec<InputEdit>,
) -> ParseOutcome {
    let mut events = Vec::new();
    let language_name = language_ops::language_for_path(coordinator, uri.path())
        .or_else(|| language_id.map(|s| s.to_string()));

    if let Some(language_name) = language_name {
        let load_result = language_ops::ensure_language_loaded(coordinator, &language_name);
        events.extend(load_result.events.clone());

        if let Some(mut parser) = language_ops::acquire_parser(parser_pool, &language_name) {
            let old_tree = if !edits.is_empty() {
                documents.get_edited_tree(&uri, &edits)
            } else {
                documents
                    .get(&uri)
                    .and_then(|doc| doc.layers().root_layer().map(|layer| layer.tree.clone()))
            };

            let parsed_tree = parser.parse(&text, old_tree.as_ref());
            language_ops::release_parser(parser_pool, language_name.clone(), parser);

            if let Some(tree) = parsed_tree {
                if !edits.is_empty() {
                    documents.update_document_with_tree(uri.clone(), text, tree);
                } else {
                    let root_layer = Some(LanguageLayer::root(language_name.clone(), tree));
                    documents.insert(uri.clone(), text, root_layer);
                }

                return ParseOutcome { events };
            }
        }
    }

    documents.insert(uri, text, None);
    ParseOutcome { events }
}

pub fn document_language(
    coordinator: &LanguageCoordinator,
    documents: &DocumentStore,
    uri: &Url,
) -> Option<String> {
    if let Some(lang) = language_ops::language_for_path(coordinator, uri.path()) {
        return Some(lang);
    }

    documents
        .get(uri)
        .and_then(|doc| doc.layers().get_language_id().map(|s| s.to_string()))
}

pub fn document_reference<'a>(
    documents: &'a DocumentStore,
    uri: &Url,
) -> Option<DocumentHandle<'a>> {
    documents.get(uri)
}

pub fn document_text(documents: &DocumentStore, uri: &Url) -> Option<String> {
    documents.get_document_text(uri)
}

pub fn update_semantic_tokens(documents: &DocumentStore, uri: &Url, tokens: SemanticTokens) {
    documents.update_semantic_tokens(uri, SemanticSnapshot::new(tokens));
}

pub fn remove_document(documents: &DocumentStore, uri: &Url) -> Option<Document> {
    documents.remove(uri)
}
