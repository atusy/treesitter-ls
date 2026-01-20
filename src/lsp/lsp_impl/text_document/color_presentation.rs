//! Color presentation method for Kakehashi.

use tower_lsp_server::jsonrpc::{Id, Result};
use tower_lsp_server::ls_types::*;

use crate::language::InjectionResolver;
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::{Kakehashi, uri_to_url};

impl Kakehashi {
    pub(crate) async fn color_presentation_impl(
        &self,
        params: ColorPresentationParams,
    ) -> Result<Vec<ColorPresentation>> {
        let lsp_uri = params.text_document.uri;
        let range = params.range;
        let color = params.color;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in colorPresentation: {}", lsp_uri.as_str());
            return Ok(vec![]);
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "colorPresentation called for {} at lines {}-{}",
                    uri, range.start.line, range.end.line
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
            return Ok(Vec::new());
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "kakehashi::color_presentation", "No language detected");
            return Ok(Vec::new());
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(Vec::new());
        };

        // Use range.start position to find the injection region
        let mapper = PositionMapper::new(snapshot.text());
        let Some(byte_offset) = mapper.position_to_byte(range.start) else {
            return Ok(Vec::new());
        };

        let Some(resolved) = InjectionResolver::resolve_at_byte_offset(
            &self.region_id_tracker,
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
            byte_offset,
        ) else {
            // Not in an injection region - return empty
            return Ok(Vec::new());
        };

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &resolved.injection_language)
        else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "No bridge server configured for language: {} (host: {})",
                        resolved.injection_language, language_name
                    ),
                )
                .await;
            return Ok(Vec::new());
        };

        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => n,
            // For string IDs or no ID, use 0 as fallback
            _ => 0,
        };

        // Convert Color to JSON Value for bridge
        let color_json = serde_json::to_value(color).unwrap_or_default();

        // Send color presentation request via language server pool
        let response = self
            .language_server_pool
            .send_color_presentation_request(
                &server_config,
                &uri,
                range,
                &color_json,
                &resolved.injection_language,
                &resolved.region.result_id,
                resolved.region.line_range.start,
                &resolved.virtual_content,
                upstream_request_id,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the color presentation response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(Vec::new());
                    }

                    // Parse the result into Vec<ColorPresentation>
                    if let Ok(presentations) =
                        serde_json::from_value::<Vec<ColorPresentation>>(result.clone())
                    {
                        return Ok(presentations);
                    }
                }
                Ok(Vec::new())
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge color presentation request failed: {}", e),
                    )
                    .await;
                Ok(Vec::new())
            }
        }
    }
}
