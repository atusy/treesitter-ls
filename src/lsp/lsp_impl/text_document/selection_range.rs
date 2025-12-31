//! Selection range method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::analysis::handle_selection_range;
use crate::error::LockResultExt;

use super::super::TreeSitterLs;

impl TreeSitterLs {
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

            let sync_parse_result = {
                let mut pool = self
                    .parser_pool
                    .lock()
                    .recover_poison("selection_range sync_parse")
                    .unwrap();
                if let Some(mut parser) = pool.acquire(&language_name) {
                    let result = parser.parse(&text, None);
                    pool.release(language_name.clone(), parser);
                    result
                } else {
                    None
                }
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
            let mut pool = self
                .parser_pool
                .lock()
                .recover_poison("selection_range parser_pool")
                .unwrap();
            let result = handle_selection_range(&doc, &positions, &self.language, &mut pool);

            return Ok(Some(result));
        }

        // Use full injection parsing handler with coordinator and parser pool
        let mut pool = self
            .parser_pool
            .lock()
            .recover_poison("selection_range parser_pool")
            .unwrap();
        let result = handle_selection_range(&doc, &positions, &self.language, &mut pool);

        Ok(Some(result))
    }
}
