//! Goto declaration method for TreeSitterLs.

use tower_lsp::jsonrpc::{Id, Result};
use tower_lsp::lsp_types::request::{GotoDeclarationParams, GotoDeclarationResponse};
use tower_lsp::lsp_types::*;

use crate::language::injection::{
    CacheableInjectionRegion, calculate_region_id, find_injection_at_position,
};
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn goto_declaration_impl(
        &self,
        params: GotoDeclarationParams,
    ) -> Result<Option<GotoDeclarationResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "goto_declaration called for {} at line {} col {}",
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
            log::debug!(target: "treesitter_ls::declaration", "No language detected");
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
        let Some((region_index, region)) = find_injection_at_position(&injections, byte_offset)
        else {
            // Not in an injection region - return None
            return Ok(None);
        };

        // Calculate stable region_id for virtual document URI
        let region_id = calculate_region_id(&injections, region_index);

        // Convert to CacheableInjectionRegion to get line_range for position mapping
        let cacheable_region =
            CacheableInjectionRegion::from_region_info(region, &region_id, &text);

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

        // Send declaration request via language server pool
        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => n,
            // For string IDs or no ID, use 0 as fallback
            _ => 0,
        };
        let response = self
            .language_server_pool
            .send_declaration_request(
                &server_config,
                &uri,
                position,
                &region.language,
                &cacheable_region.result_id,
                cacheable_region.line_range.start,
                &virtual_content,
                upstream_request_id,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the declaration response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(None);
                    }

                    // Parse the result into a GotoDeclarationResponse
                    if let Ok(declaration) =
                        serde_json::from_value::<GotoDeclarationResponse>(result.clone())
                    {
                        return Ok(Some(declaration));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge declaration request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
