//! Inlay hint method for Kakehashi.

use tower_lsp_server::jsonrpc::{Id, Result};
use tower_lsp_server::ls_types::*;

use crate::language::InjectionResolver;
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::{Kakehashi, uri_to_url};

impl Kakehashi {
    pub(crate) async fn inlay_hint_impl(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let lsp_uri = params.text_document.uri;
        let range = params.range;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in inlayHint: {}", lsp_uri.as_str());
            return Ok(None);
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "inlayHint called for {} at lines {}-{}",
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
            return Ok(None);
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "kakehashi::inlay_hint", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Use range.start position to find the injection region
        // Note: This is a simplification - for range spanning multiple regions,
        // we'd need to aggregate results from all regions. For now, we use start position.
        let mapper = PositionMapper::new(snapshot.text());
        let Some(byte_offset) = mapper.position_to_byte(range.start) else {
            return Ok(None);
        };

        let Some(resolved) = InjectionResolver::resolve_at_byte_offset(
            &self.region_id_tracker,
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
            byte_offset,
        ) else {
            // Not in an injection region - return None
            return Ok(None);
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
            return Ok(None);
        };

        // Send inlay hint request via language server pool
        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => n,
            // For string IDs or no ID, use 0 as fallback
            _ => 0,
        };
        let response = self
            .language_server_pool
            .send_inlay_hint_request(
                &server_config,
                &uri,
                range,
                &resolved.injection_language,
                &resolved.region.result_id,
                resolved.region.line_range.start,
                &resolved.virtual_content,
                upstream_request_id,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the inlay hint response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(None);
                    }

                    // Parse the result into Vec<InlayHint>
                    if let Ok(hints) = serde_json::from_value::<Vec<InlayHint>>(result.clone()) {
                        return Ok(Some(hints));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge inlay hint request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
