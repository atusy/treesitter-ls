//! Code action method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::analysis::handle_code_actions;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn code_action_impl(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document and tree
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text();
        let Some(tree) = doc.tree() else {
            return Ok(None);
        };

        let domain_range = range;

        // Get language for the document
        let language_name = self.get_language_for_document(&uri);

        // Try to get bridged actions from injection region (child language)
        let bridged_actions = if let Some(ref lang) = language_name {
            self.try_bridge_code_action(&uri, text, tree, lang, range)
                .await
        } else {
            None
        };

        // Get capture mappings
        let capture_mappings = self.language.get_capture_mappings();
        let capture_context = language_name.as_deref().map(|ft| (ft, &capture_mappings));

        // Get treesitter-ls actions (parent language)
        let parent_actions = if let Some(lang) = language_name.clone() {
            let highlight_query = self.language.get_highlight_query(&lang);
            let locals_query = self.language.get_locals_query(&lang);
            let injection_query = self.language.get_injection_query(&lang);

            let queries = highlight_query
                .as_ref()
                .map(|hq| (hq.as_ref(), locals_query.as_ref().map(|lq| lq.as_ref())));

            // Build code action options with injection support
            let mut options =
                crate::analysis::refactor::CodeActionOptions::new(&uri, text, tree, domain_range)
                    .with_queries(queries)
                    .with_capture_context(capture_context);

            // Add injection query if available
            if let Some(inj_q) = injection_query.as_ref() {
                options = options.with_injection(inj_q.as_ref());
            }

            // Use coordinator if we can get parser pool lock
            if let Ok(mut pool) = self.parser_pool.lock() {
                options = options.with_coordinator(&self.language, &mut pool);
                handle_code_actions(options)
            } else {
                // Fallback without coordinator
                handle_code_actions(options)
            }
        } else {
            handle_code_actions(crate::analysis::refactor::CodeActionOptions::new(
                &uri,
                text,
                tree,
                domain_range,
            ))
        };

        // Merge actions: child (bridged) first, then parent (treesitter-ls)
        let lsp_response = match (bridged_actions, parent_actions) {
            (Some(mut child), Some(parent)) => {
                // Child actions first, then parent actions
                child.extend(parent);
                Some(child)
            }
            (Some(child), None) => Some(child),
            (None, Some(parent)) => Some(parent),
            (None, None) => None,
        };

        Ok(lsp_response)
    }
}
