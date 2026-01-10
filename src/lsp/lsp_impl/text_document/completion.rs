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
        let text = doc.text().to_string();

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
        let Some(tree) = doc.tree().cloned() else {
            return Ok(None);
        };

        // Drop the document reference to avoid holding it across await
        drop(doc);

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            &text,
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            return Ok(None);
        };

        // Convert Position to byte offset
        let mapper = PositionMapper::new(&text);
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

        // Convert to CacheableInjectionRegion to get line_range for position mapping
        let cacheable_region =
            CacheableInjectionRegion::from_region_info(region, "completion-temp", &text);

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

        // Extract the virtual document content (just the injection region)
        let virtual_content = cacheable_region.extract_content(&text).to_string();

        // Send completion request via language server pool
        let response = self
            .language_server_pool
            .send_completion_request(
                &server_config,
                &uri,
                position,
                &region.language,
                &cacheable_region.result_id,
                cacheable_region.line_range.start,
                &virtual_content,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the completion response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(None);
                    }

                    // Try to parse as CompletionList first
                    if let Ok(list) = serde_json::from_value::<CompletionList>(result.clone()) {
                        return Ok(Some(CompletionResponse::List(list)));
                    }

                    // Try to parse as array of CompletionItem
                    if let Ok(items) = serde_json::from_value::<Vec<CompletionItem>>(result.clone())
                    {
                        return Ok(Some(CompletionResponse::Array(items)));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge completion request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
