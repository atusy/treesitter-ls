use super::ParseOutcome;
use super::documents::WorkspaceDocuments;
use super::languages::WorkspaceLanguages;
use crate::document::{Document, LanguageLayer, SemanticSnapshot};
use crate::domain::SemanticTokens;
use tree_sitter::InputEdit;
use url::Url;

pub fn parse_document(
    languages: &WorkspaceLanguages,
    documents: &WorkspaceDocuments,
    uri: Url,
    text: String,
    language_id: Option<&str>,
    edits: Vec<InputEdit>,
) -> ParseOutcome {
    let mut events = Vec::new();
    let language_name = languages
        .language_for_path(uri.path())
        .or_else(|| language_id.map(|s| s.to_string()));

    if let Some(language_name) = language_name {
        let load_result = languages.ensure_language_loaded(&language_name);
        events.extend(load_result.events.clone());

        if let Some(mut parser) = languages.acquire_parser(&language_name) {
            let old_tree = if !edits.is_empty() {
                documents.get_edited_tree(&uri, &edits)
            } else {
                documents
                    .get(&uri)
                    .and_then(|doc| doc.layers().root_layer().map(|layer| layer.tree.clone()))
            };

            let parsed_tree = parser.parse(&text, old_tree.as_ref());
            languages.release_parser(language_name.clone(), parser);

            if let Some(tree) = parsed_tree {
                if !edits.is_empty() {
                    documents.update_with_tree(uri.clone(), text, tree);
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
    languages: &WorkspaceLanguages,
    documents: &WorkspaceDocuments,
    uri: &Url,
) -> Option<String> {
    if let Some(lang) = languages.language_for_path(uri.path()) {
        return Some(lang);
    }

    documents
        .get(uri)
        .and_then(|doc| doc.layers().get_language_id().map(|s| s.to_string()))
}

pub fn document_reference<'a>(
    documents: &'a WorkspaceDocuments,
    uri: &Url,
) -> Option<super::DocumentRef<'a>> {
    documents.get(uri)
}

pub fn document_text(documents: &WorkspaceDocuments, uri: &Url) -> Option<String> {
    documents.text(uri)
}

pub fn update_semantic_tokens(documents: &WorkspaceDocuments, uri: &Url, tokens: SemanticTokens) {
    documents.update_semantic_tokens(uri, SemanticSnapshot::new(tokens));
}

pub fn remove_document(documents: &WorkspaceDocuments, uri: &Url) -> Option<Document> {
    documents.remove(uri)
}
