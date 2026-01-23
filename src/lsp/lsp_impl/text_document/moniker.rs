//! Moniker method for Kakehashi.

use tower_lsp_server::jsonrpc::{Id, Result};
use tower_lsp_server::ls_types::{MessageType, Moniker, MonikerParams};

use crate::language::InjectionResolver;
use crate::lsp::bridge::UpstreamId;
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::{Kakehashi, uri_to_url};

impl Kakehashi {
    pub(crate) async fn moniker_impl(&self, params: MonikerParams) -> Result<Option<Vec<Moniker>>> {
        let lsp_uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in moniker: {}", lsp_uri.as_str());
            return Ok(None);
        };

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "moniker called for {} at line {} col {}",
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
            log::debug!(target: "kakehashi::moniker", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Resolve injection region at position (centralizes 29-86 lines of duplication)
        let mapper = PositionMapper::new(snapshot.text());
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            return Ok(None);
        };

        let Some(resolved) = InjectionResolver::resolve_at_byte_offset(
            self.bridge.region_id_tracker(),
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

        // Send moniker request via language server pool
        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => UpstreamId::Number(n),
            Some(Id::String(s)) => UpstreamId::String(s),
            // For notifications without ID or null ID, use 0 as fallback
            None | Some(Id::Null) => UpstreamId::Number(0),
        };
        let response = self
            .bridge
            .pool()
            .send_moniker_request(
                &server_config,
                &uri,
                position,
                &resolved.injection_language,
                &resolved.region.result_id,
                resolved.region.line_range.start,
                &resolved.virtual_content,
                upstream_request_id,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the moniker response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(None);
                    }

                    // Parse the result into Vec<Moniker>
                    if let Ok(monikers) = serde_json::from_value::<Vec<Moniker>>(result.clone()) {
                        return Ok(Some(monikers));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge moniker request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
