//! Formatting method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn formatting_impl(
        &self,
        params: DocumentFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(MessageType::INFO, format!("formatting called for {}", uri))
            .await;

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            self.client
                .log_message(MessageType::INFO, "No document found")
                .await;
            return Ok(None);
        };
        let text = doc.text();

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "treesitter_ls::formatting", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Get the parse tree
        let Some(tree) = doc.tree() else {
            return Ok(None);
        };

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            text,
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            return Ok(None);
        };

        // For formatting, we need a position to determine which injection region to format.
        // Since textDocument/formatting doesn't provide a position, we format ALL injection
        // regions and aggregate the results. This is the expected behavior for document-wide
        // formatting.
        let mut all_edits: Vec<TextEdit> = Vec::new();

        for region in &injections {
            // Get bridge server config for this language
            let Some(server_config) =
                self.get_bridge_config_for_language(&language_name, &region.language)
            else {
                continue;
            };

            // Create cacheable region for position translation
            let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);

            // Extract virtual document content
            let virtual_content = cacheable.extract_content(text).to_owned();

            // Get shared connection from async pool
            let pool_key = server_config.cmd.first().cloned().unwrap_or_default();
            let conn = match self
                .async_language_server_pool
                .get_connection(&pool_key, &server_config)
                .await
            {
                Some(c) => c,
                None => {
                    continue;
                }
            };

            // Send didOpen and wait for indexing
            if conn
                .did_open_and_wait("rust", &virtual_content)
                .await
                .is_err()
            {
                continue;
            }

            // Send formatting request and await response asynchronously
            let formatting_result = conn.formatting().await;

            // Translate TextEdit ranges back to host document coordinates
            if let Some(edits) = formatting_result {
                for edit in edits {
                    all_edits.push(TextEdit {
                        range: Range {
                            start: cacheable.translate_virtual_to_host(edit.range.start),
                            end: cacheable.translate_virtual_to_host(edit.range.end),
                        },
                        new_text: edit.new_text,
                    });
                }
            }
        }

        if all_edits.is_empty() {
            Ok(None)
        } else {
            Ok(Some(all_edits))
        }
    }
}
