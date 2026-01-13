//! Goto implementation method for TreeSitterLs.

use tower_lsp::jsonrpc::{Id, Result};
use tower_lsp::lsp_types::request::{GotoImplementationParams, GotoImplementationResponse};
use tower_lsp::lsp_types::*;

use crate::language::injection::{
    CacheableInjectionRegion, calculate_region_id, find_injection_at_position,
};
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn goto_implementation_impl(
        &self,
        params: GotoImplementationParams,
    ) -> Result<Option<GotoImplementationResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "goto_implementation called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
            )
            .await;

        // Get document snapshot (minimizes lock duration)
        let (snapshot, missing_message) = match self.documents.get(&uri) {
            None => (None, Some("No document found")),
            Some(doc) => match doc.snapshot() {
                None => (None, Some("Document not fully initialized")),
                Some(snapshot) => (Some(snapshot), None),
            },
            // doc automatically dropped here, lock released
        };
        if let Some(message) = missing_message {
            self.client.log_message(MessageType::INFO, message).await;
            return Ok(None);
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "treesitter_ls::implementation", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };


        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &snapshot.tree().root_node(),
            snapshot.text(),
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            return Ok(None);
        };

        // Convert Position to byte offset
        let mapper = PositionMapper::new(snapshot.text());
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
            CacheableInjectionRegion::from_region_info(region, &region_id, snapshot.text());

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
        let virtual_content = cacheable_region.extract_content(snapshot.text()).to_string();

        // Send implementation request via language server pool
        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => n,
            // For string IDs or no ID, use 0 as fallback
            _ => 0,
        };
        let response = self
            .language_server_pool
            .send_implementation_request(
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
                // Parse the implementation response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(None);
                    }

                    // Parse the result into a GotoImplementationResponse
                    if let Ok(implementation) =
                        serde_json::from_value::<GotoImplementationResponse>(result.clone())
                    {
                        return Ok(Some(implementation));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge implementation request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
