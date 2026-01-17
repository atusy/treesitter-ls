//! Selection range method for Kakehashi.

use std::time::Duration;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::analysis::handle_selection_range;

use super::super::Kakehashi;

/// Timeout for spawn_blocking parse operations to prevent hangs on pathological inputs.
const PARSE_TIMEOUT: Duration = Duration::from_secs(10);

impl Kakehashi {
    pub(crate) async fn selection_range_impl(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri;
        let positions = params.positions;

        // Get language for document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(None);
        };

        // Ensure language is loaded (handles race condition with didOpen)
        let load_result = self.language.ensure_language_loaded(&language_name);
        if !load_result.success {
            return Ok(None);
        }

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };

        // Check if document has a tree, if not parse it synchronously
        if doc.tree().is_none() {
            let text = doc.text().to_string();
            drop(doc); // Release lock before acquiring parser pool

            // Checkout parser (brief lock)
            let parser = {
                let mut pool = self.parser_pool.lock().await;
                pool.acquire(&language_name)
            };

            let sync_parse_result = if let Some(mut parser) = parser {
                let text_clone = text.clone();
                let language_name_clone = language_name.clone();
                let uri_clone = uri.clone();

                // Parse in spawn_blocking with timeout to avoid blocking tokio worker thread
                let result = tokio::time::timeout(
                    PARSE_TIMEOUT,
                    tokio::task::spawn_blocking(move || {
                        let parse_result = parser.parse(&text_clone, None);
                        (parser, parse_result)
                    }),
                )
                .await;

                // Handle timeout vs successful completion
                let result = match result {
                    Ok(join_result) => match join_result {
                        Ok(result) => Some(result),
                        Err(e) => {
                            log::error!(
                                "Parse task panicked for language '{}' on document {}: {}",
                                language_name_clone,
                                uri_clone,
                                e
                            );
                            None
                        }
                    },
                    Err(_timeout) => {
                        log::warn!(
                            "Parse timeout after {:?} for language '{}' on document {} ({} bytes)",
                            PARSE_TIMEOUT,
                            language_name_clone,
                            uri_clone,
                            text.len()
                        );
                        None
                    }
                };

                if let Some((parser, parse_result)) = result {
                    // Return parser to pool (brief lock)
                    let mut pool = self.parser_pool.lock().await;
                    pool.release(language_name_clone, parser);
                    parse_result
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(tree) = sync_parse_result {
                self.documents
                    .update_document(uri.clone(), text, Some(tree));
            } else {
                return Ok(None);
            }

            // Re-acquire document after update
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(None);
            };

            // Use full injection parsing handler with coordinator and parser pool
            let mut pool = self.parser_pool.lock().await;
            let result = handle_selection_range(&doc, &positions, &self.language, &mut pool);

            return Ok(Some(result));
        }

        // Use full injection parsing handler with coordinator and parser pool
        let mut pool = self.parser_pool.lock().await;
        let result = handle_selection_range(&doc, &positions, &self.language, &mut pool);

        Ok(Some(result))
    }
}
