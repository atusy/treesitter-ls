//! Completion method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn completion_impl(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "completion called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
            )
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
            log::debug!(target: "treesitter_ls::completion", "No language detected");
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

        // Convert Position to byte offset
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            return Ok(None);
        };

        // Find which injection region (if any) contains this position
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            // Not in an injection region - return None
            return Ok(None);
        };

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &region.language)
        else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "No bridge server configured for language: {} (host: {})",
                        region.language, language_name
                    ),
                )
                .await;
            return Ok(None);
        };

        // Create cacheable region for position translation
        let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);

        // Extract virtual document content
        let virtual_content = cacheable.extract_content(text).to_owned();

        // Translate host position to virtual position
        let virtual_position = cacheable.translate_host_to_virtual(position);

        // Get language server connection from async pool
        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        // Get shared connection from async pool (multiple callers can use concurrently)
        let conn = match self
            .async_language_server_pool
            .get_connection(&pool_key, &server_config)
            .await
        {
            Some(c) => c,
            None => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to spawn language server: {}", pool_key),
                    )
                    .await;
                return Ok(None);
            }
        };

        // Send didOpen and wait for indexing
        if let Err(e) = conn.did_open_and_wait("rust", &virtual_content).await {
            self.client
                .log_message(MessageType::ERROR, format!("didOpen failed: {}", e))
                .await;
            return Ok(None);
        }

        // Send completion request and await response asynchronously
        let completion = conn.completion(virtual_position).await;

        // Translate completion response ranges back to host document
        let Some(completion_response) = completion else {
            return Ok(None);
        };

        // Helper function to translate a CompletionTextEdit range
        let translate_text_edit = |text_edit: &mut CompletionTextEdit| match text_edit {
            CompletionTextEdit::Edit(edit) => {
                edit.range.start = cacheable.translate_virtual_to_host(edit.range.start);
                edit.range.end = cacheable.translate_virtual_to_host(edit.range.end);
            }
            CompletionTextEdit::InsertAndReplace(edit) => {
                edit.insert.start = cacheable.translate_virtual_to_host(edit.insert.start);
                edit.insert.end = cacheable.translate_virtual_to_host(edit.insert.end);
                edit.replace.start = cacheable.translate_virtual_to_host(edit.replace.start);
                edit.replace.end = cacheable.translate_virtual_to_host(edit.replace.end);
            }
        };

        // Helper function to translate additional_text_edits
        let translate_additional_edits = |edits: &mut Vec<TextEdit>| {
            for edit in edits.iter_mut() {
                edit.range.start = cacheable.translate_virtual_to_host(edit.range.start);
                edit.range.end = cacheable.translate_virtual_to_host(edit.range.end);
            }
        };

        // Translate all ranges in completion items
        let translated_response = match completion_response {
            CompletionResponse::Array(mut items) => {
                for item in items.iter_mut() {
                    if let Some(ref mut text_edit) = item.text_edit {
                        translate_text_edit(text_edit);
                    }
                    if let Some(ref mut additional_edits) = item.additional_text_edits {
                        translate_additional_edits(additional_edits);
                    }
                }
                CompletionResponse::Array(items)
            }
            CompletionResponse::List(mut list) => {
                for item in list.items.iter_mut() {
                    if let Some(ref mut text_edit) = item.text_edit {
                        translate_text_edit(text_edit);
                    }
                    if let Some(ref mut additional_edits) = item.additional_text_edits {
                        translate_additional_edits(additional_edits);
                    }
                }
                CompletionResponse::List(list)
            }
        };

        Ok(Some(translated_response))
    }
}
